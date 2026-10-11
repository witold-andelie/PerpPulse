//! Explicit historical Envio serving. A fixed archive never certifies live freshness.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::accounting::account_wallet;
use crate::analytics::{analyze, CoverageBasis};
use crate::envio::{
    AccountEventSlice, EnvioClient, EnvioCoverage, IndexedPoint, ProtocolEventSlice,
};
use crate::error::{DataQualityError, Result};
use crate::events::{CanonicalEvent, LifecycleKind};
use crate::identity::AsOf;
use crate::registry::ProtocolRegistry;
use crate::serve::{attach_intelligence, run_service, wallet_value, ApiSnapshot};

pub struct HistoricalConfig {
    pub client: EnvioClient,
    pub registry: ProtocolRegistry,
    pub accounts: Vec<u64>,
    pub rpc_url: String,
    pub start_block: u64,
    pub end_block: u64,
    pub expected_event_count: usize,
    /// Reviewed SDK observations at this exact cutoff, bound to the registry.
    pub registry_proof: Value,
}

impl HistoricalConfig {
    fn validate(&self) -> Result<()> {
        self.registry.validate()?;
        if self.start_block < self.registry.deployed_at_block
            || self.end_block < self.start_block
            || !(1..=50_000).contains(&self.expected_event_count)
            || self.accounts.is_empty()
            || self.accounts.len() > 20
            || self.accounts.iter().copied().collect::<BTreeSet<_>>().len() != self.accounts.len()
        {
            return Err(DataQualityError::msg(
                "invalid fixed historical window, event budget or account selection",
            ));
        }
        Ok(())
    }
}

fn verify_registry(
    config: &HistoricalConfig,
    slice: &ProtocolEventSlice,
    cutoff: &AsOf,
) -> Result<()> {
    let proof = &config.registry_proof;
    if proof["version"] != "historical-market-proof-v1"
        || proof["status"] != "matched"
        || proof["chainId"] != cutoff.chain_id
        || proof["asOfBlock"] != cutoff.block_number
        || proof["asOfBlockHash"] != cutoff.block_hash
        || proof["asOfTimestampMs"] != cutoff.timestamp_ms
        || proof["registryInputsHash"] != crate::evidence::digest(&config.registry)?
        || proof["canonicalInputsHash"] != crate::evidence::digest(&slice.events)?
        || proof["collateralDecimals"] != config.registry.collateral.decimals
    {
        return Err(DataQualityError::msg(
            "historical registry proof differs from the registry or cutoff",
        ));
    }
    let rows = proof["markets"]
        .as_array()
        .ok_or_else(|| DataQualityError::msg("historical market proof is missing"))?;
    let mut proven = BTreeSet::new();
    for row in rows {
        let id = row["perpetualId"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| DataQualityError::msg("invalid historical market proof identity"))?;
        let spec = config.registry.market(id)?;
        if !proven.insert(id)
            || row["priceDecimals"] != spec.price_decimals
            || row["sizeDecimals"] != spec.size_decimals
            || row["initMarginFracHdths"] != spec.init_margin_frac_hdths
            || row["maintMarginFracHdths"] != spec.maint_margin_frac_hdths
        {
            return Err(DataQualityError::msg(
                "historical market scales or margins differ from the proof",
            ));
        }
    }
    if slice
        .events
        .iter()
        .filter_map(|e| e.perpetual_id)
        .any(|id| !proven.contains(&id))
    {
        return Err(DataQualityError::msg(
            "historical event market has no same-cutoff parameter proof",
        ));
    }
    Ok(())
}

/// Read and validate an independently supplied Monad block header.
pub fn block_header(endpoint: &str, block: u64) -> Result<AsOf> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let chain: Value = agent
        .post(endpoint)
        .send_json(json!({"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}))
        .map_err(|_| DataQualityError::msg("historical RPC chain identity request failed"))?
        .body_mut()
        .read_json()
        .map_err(|_| DataQualityError::msg("invalid historical RPC chain identity response"))?;
    if chain["result"]
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        != Some(143)
    {
        return Err(DataQualityError::msg(
            "historical RPC is not Monad chain 143",
        ));
    }
    let response: Value = agent
        .post(endpoint)
        .send_json(
            json!({"jsonrpc":"2.0","id":2,"method":"eth_getBlockByNumber",
            "params":[format!("0x{block:x}"),false]}),
        )
        .map_err(|_| DataQualityError::msg("historical RPC block request failed"))?
        .body_mut()
        .read_json()
        .map_err(|_| DataQualityError::msg("invalid historical RPC block response"))?;
    let header = &response["result"];
    let number = |field: &str| {
        header[field]
            .as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .and_then(|s| u64::from_str_radix(s, 16).ok())
    };
    if response.get("error").is_some() || number("number") != Some(block) {
        return Err(DataQualityError::msg(
            "historical RPC block identity is missing or incorrect",
        ));
    }
    let hash = header["hash"]
        .as_str()
        .filter(|s| {
            s.len() == 66 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .ok_or_else(|| DataQualityError::msg("historical RPC block hash is invalid"))?;
    let timestamp = number("timestamp")
        .and_then(|s| s.checked_mul(1000))
        .and_then(|s| i64::try_from(s).ok())
        .ok_or_else(|| DataQualityError::msg("historical RPC timestamp is invalid"))?;
    AsOf::new(143, block, hash, timestamp, None)
}

fn verify_scope(config: &HistoricalConfig, coverage: &EnvioCoverage) -> Result<()> {
    if coverage.evidence.chain_id != config.registry.chain_id
        || coverage.evidence.start_block != config.start_block
        || coverage.evidence.processed_block != config.end_block
        || coverage.events_processed != config.expected_event_count as u64
        || coverage.latest_event.block_number < config.start_block
        || coverage.latest_event.block_number > config.end_block
    {
        return Err(DataQualityError::msg(
            "historical source no longer matches the pinned start, end or event count",
        ));
    }
    Ok(())
}

/// Build historical facts through the existing canonical ledger and analytics engine.
pub fn build_historical_snapshot(
    config: &HistoricalConfig,
    slice: &ProtocolEventSlice,
    start: &AsOf,
    cutoff: &AsOf,
) -> Result<ApiSnapshot> {
    config.validate()?;
    verify_scope(config, &slice.coverage)?;
    verify_registry(config, slice, cutoff)?;
    if slice.events.len() != config.expected_event_count
        || start.block_number != config.start_block
        || cutoff.block_number != config.end_block
        || start.chain_id != config.registry.chain_id
        || cutoff.chain_id != config.registry.chain_id
        || cutoff.log_index.is_some()
        || start.timestamp_ms > cutoff.timestamp_ms
    {
        return Err(DataQualityError::msg(
            "historical event count or independent header scope differs",
        ));
    }
    for event in &slice.events {
        event.validate()?;
        if event.chain_id != cutoff.chain_id
            || !cutoff.includes(event.block_number, event.log_index)
            || event.block_number < start.block_number
            || event.timestamp_ms < start.timestamp_ms
            || event.timestamp_ms > cutoff.timestamp_ms
            || !event
                .contract_address
                .eq_ignore_ascii_case(&config.registry.exchange_address)
            || (event.block_number == start.block_number
                && (event.block_hash != start.block_hash
                    || event.timestamp_ms != start.timestamp_ms))
            || (event.block_number == cutoff.block_number
                && (event.block_hash != cutoff.block_hash
                    || event.timestamp_ms != cutoff.timestamp_ms))
        {
            return Err(DataQualityError::msg(
                "historical event is outside the independently verified window",
            ));
        }
    }
    let latest = &slice.coverage.latest_event;
    if !slice.events.iter().any(|event| {
        event.event_id().is_ok_and(|id| id.key() == latest.id)
            && event.block_number == latest.block_number
            && event.block_hash == latest.block_hash
            && event.log_index == latest.log_index
            && event.timestamp_ms == latest.timestamp_ms
    }) {
        return Err(DataQualityError::msg(
            "historical latest-event anchor is absent from canonical inputs",
        ));
    }
    let analytics = analyze(
        &slice.events,
        &config.registry,
        cutoff,
        &CoverageBasis {
            start_block: config.start_block,
            deployed_at_block: config.registry.deployed_at_block,
            start_timestamp_ms: Some(start.timestamp_ms),
        },
        None,
        "protocol",
    )?;
    let mut wallets = Vec::new();
    let mut inspected = BTreeMap::new();
    let mut marks = BTreeMap::new();
    let mut checkpoints = Vec::new();
    for account in &config.accounts {
        let events: Vec<_> = slice
            .events
            .iter()
            .filter(|e| e.account_id == Some(*account))
            .cloned()
            .collect();
        let selected = AccountEventSlice {
            account_id: *account,
            coverage: slice.coverage.clone(),
            as_of: cutoff.clone(),
            events,
        };
        let eligibility = selected.replay_eligibility(&config.registry)?;
        let mut wallet = if eligibility.eligible {
            let initial = crate::ledger::replay(&selected.events, &config.registry, cutoff)?;
            let market_ids: BTreeSet<_> = initial
                .open_positions()
                .iter()
                .map(|p| p.position_id.perpetual_id)
                .collect();
            let mut inputs = selected.events.clone();
            inputs.extend(
                slice
                    .events
                    .iter()
                    .filter(|e| {
                        e.account_id.is_none()
                            && e.perpetual_id.is_some_and(|id| market_ids.contains(&id))
                            && matches!(
                                e.kind,
                                LifecycleKind::MarkUpdated
                                    | LifecycleKind::MarketFunding
                                    | LifecycleKind::FundingScaleUpdated
                            )
                    })
                    .cloned(),
            );
            let ledger = if market_ids.is_empty() {
                initial
            } else {
                crate::ledger::replay_with_funding_coverage(
                    &inputs,
                    &config.registry,
                    cutoff,
                    &crate::funding_checkpoint::FundingCoverage {
                        start_block: config.start_block,
                        end_block: config.end_block,
                        market_ids: market_ids.iter().copied().collect(),
                    },
                )?
            };
            let observed_marks: Vec<_> = ledger.market_marks.values().cloned().collect();
            let accounting = account_wallet(&ledger, *account, cutoff, &observed_marks, false)?;
            let mut value = wallet_value(&accounting, true);
            value["marketInputs"] = json!(market_ids
                .iter()
                .map(|id| crate::funding::timeline(&ledger.events, *id, cutoff))
                .collect::<Result<Vec<_>>>()?);
            value["marketInputProfile"] = json!("canonical-v3");
            value["quality"] = json!("historical-covered-account");
            marks.extend(ledger.market_marks.clone());
            for event in inputs {
                inspected.insert(event.event_id()?.key(), event);
            }
            value
        } else {
            for event in &selected.events {
                inspected.insert(event.event_id()?.key(), event.clone());
            }
            json!({"accountId":account,"owner":null,"freeBalance":null,"realizedPnl":null,
                "unrealizedPnl":null,"fees":null,"realizedFunding":null,"positions":[],
                "quality":"incomplete-history","warnings":[eligibility.reason]})
        };
        wallet["replayEligible"] = json!(eligibility.eligible);
        wallet["replayBasis"] = json!(eligibility.basis);
        wallet["balanceNote"] = json!("Historical account scope; exact free balance and lifetime totals require complete independent coverage.");
        checkpoints.push(
            json!({"accountId":account,"positions":wallet["positions"].as_array()
            .map(|rows| rows.iter().map(|p| json!({"perpetualId":p["perpetualId"],
                "checkpoint":p["fundingCheckpoint"]})).collect::<Vec<_>>())}),
        );
        wallets.push(wallet);
    }
    let latest = &slice.coverage.latest_event;
    for event in slice.events.iter().filter(|e| {
        e.kind == LifecycleKind::PositionLiquidated
            || (e.block_number == latest.block_number && e.log_index == latest.log_index)
    }) {
        inspected.insert(event.event_id()?.key(), event.clone());
    }
    let mut visible: Vec<CanonicalEvent> = inspected.into_values().collect();
    visible.sort_by_key(|e| (e.block_number, e.log_index));
    let evidence = visible
        .iter()
        .map(crate::live::event_value)
        .collect::<Result<Vec<_>>>()?;
    let mut manifest = crate::evidence::manifest(
        &slice.events,
        cutoff,
        &slice.coverage.evidence,
        "Envio fixed historical protocol window",
    )?;
    let window = analytics
        .window("coverage")
        .ok_or_else(|| DataQualityError::msg("missing historical coverage window"))?;
    manifest["protocolEventCount"] = json!(window.event_count);
    manifest["protocolEventIdsHash"] = json!(window.event_ids_hash);
    manifest["registryInputsHash"] = json!(crate::evidence::digest(&config.registry)?);
    manifest["historicalRegistryProofHash"] =
        json!(crate::evidence::digest(&config.registry_proof)?);
    manifest["registryVerification"] = json!({"status":"matched", "scope":"All event-market native scales and cutoff margin parameters; not SDK mark or funding adoption."});
    manifest["limitations"] = json!([
        "Fixed historical Envio scope; no current freshness is claimed.",
        "Independent boundary headers and indexed anchors are checked; this is not an independent certificate of every upstream log.",
        "Protocol-wide state and uncovered lifetime totals remain unavailable."
    ]);
    manifest["marketMarksHash"] = json!(crate::evidence::digest(&marks)?);
    manifest["fundingCheckpointsHash"] = json!(crate::evidence::digest(&checkpoints)?);
    manifest["inspectableEventCount"] = json!(visible.len());
    manifest["historicalWindow"] = json!({"startBlock":config.start_block,"endBlock":config.end_block,
        "startBlockHash":start.block_hash,"endBlockHash":cutoff.block_hash,
        "independentlyVerifiedHeaders":true,"currentFreshnessClaimed":false});
    let mut snapshot = ApiSnapshot {
        fixture_name: "Envio historical Monad window".to_string(), chain_id: config.registry.chain_id,
        exchange_address: config.registry.exchange_address.clone(), as_of_block: cutoff.block_number,
        as_of_timestamp_ms: cutoff.timestamp_ms, start_block: config.start_block,
        processed_block: config.end_block, source_note: format!(
            "Historical Envio window with {} canonical events. Account and supporting market evidence are inspectable. Full protocol state and missing lifetime facts remain unavailable.", config.expected_event_count),
        protocol: crate::live::protocol_value(&analytics),
        coverage: json!({"chainId":config.registry.chain_id,"startBlock":config.start_block,
            "endBlock":config.end_block,"processedBlock":config.end_block,"asOfBlock":cutoff.block_number,
            "asOfTimestampMs":cutoff.timestamp_ms,"startTimestampMs":start.timestamp_ms,
            "eventCount":slice.events.len(),"inspectableEventCount":visible.len(),
            "quality":"verified-historical-window","coverageLagBlocks":null,
            "eventSilenceBlocks":config.end_block-latest.block_number,"isLive":false,
            "rangeNote":"Fixed historical scope; 24h, 7d and 30d windows may be incomplete. No current freshness is claimed."}),
        wallets: json!(wallets), events: json!(evidence), mode: "historical".to_string(),
        manifest, context: Value::Null, events_available: true,
        analytics: Value::Null, signals: Value::Null, cohort: Value::Null,
    };
    attach_intelligence(
        &mut snapshot,
        &config.registry,
        Some(&analytics),
        "historical-protocol-and-watchlist",
    )?;
    Ok(snapshot)
}

fn load(config: &HistoricalConfig) -> Result<(ApiSnapshot, IndexedPoint, AsOf)> {
    config.validate()?;
    let start = block_header(&config.rpc_url, config.start_block)?;
    let cutoff = block_header(&config.rpc_url, config.end_block)?;
    let slice = config
        .client
        .fetch_archived_protocol_at(&config.registry, &cutoff, 50_000)?;
    eprintln!(
        "Historical Envio scope: {} events; market IDs {:?}",
        slice.events.len(),
        slice
            .events
            .iter()
            .filter_map(|e| e.perpetual_id)
            .collect::<BTreeSet<_>>()
    );
    let snapshot = build_historical_snapshot(config, &slice, &start, &cutoff)?;
    if block_header(&config.rpc_url, config.end_block)? != cutoff
        || block_header(&config.rpc_url, config.start_block)? != start
    {
        return Err(DataQualityError::msg(
            "independent historical headers changed during acquisition",
        ));
    }
    verify_scope(
        config,
        &config.client.fetch_coverage(config.registry.chain_id)?,
    )?;
    Ok((snapshot, slice.coverage.latest_event, cutoff))
}

struct Observed {
    snapshot: Option<ApiSnapshot>,
    error: String,
    verified_at: Instant,
}

pub fn run_historical(config: HistoricalConfig, bind: SocketAddr) -> Result<()> {
    config.validate()?;
    let state = Arc::new(RwLock::new(Observed {
        snapshot: None,
        error: "Historical Envio source is initializing.".to_string(),
        verified_at: Instant::now(),
    }));
    let writer = state.clone();
    std::thread::spawn(move || {
        let mut cached: Option<(ApiSnapshot, IndexedPoint, AsOf)> = None;
        let mut quarantined = false;
        loop {
            let checked = if quarantined {
                Err(DataQualityError::msg("Historical source is quarantined after its pinned facts changed; restart only after reconciliation."))
            } else {
                match &cached {
                    None => load(&config),
                    Some((snapshot, point, cutoff)) => (|| {
                        let coverage = config.client.fetch_coverage(config.registry.chain_id)?;
                        if let Err(error) = verify_scope(&config, &coverage) {
                            quarantined = true;
                            return Err(error);
                        }
                        if coverage.latest_event != *point
                            || block_header(&config.rpc_url, config.end_block)? != *cutoff
                        {
                            quarantined = true;
                            return Err(DataQualityError::msg(
                                "historical source anchor or independent header changed",
                            ));
                        }
                        config.client.verify_point(point)?;
                        Ok((snapshot.clone(), point.clone(), cutoff.clone()))
                    })(),
                }
            };
            if let Ok(mut observed) = writer.write() {
                match checked {
                    Ok((snapshot, point, cutoff)) => {
                        observed.snapshot = Some(snapshot.clone());
                        observed.verified_at = Instant::now();
                        cached = Some((snapshot, point, cutoff));
                    }
                    Err(error) => {
                        observed.snapshot = None;
                        observed.error = error.to_string();
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
    run_service(
        move || {
            let observed = state
                .read()
                .map_err(|_| DataQualityError::msg("historical state lock failed"))?;
            if observed.verified_at.elapsed() > Duration::from_secs(150) {
                return Err(DataQualityError::msg(
                    "historical source availability verification expired",
                ));
            }
            observed
                .snapshot
                .clone()
                .ok_or_else(|| DataQualityError::msg(&observed.error))
        },
        bind,
    )
}
