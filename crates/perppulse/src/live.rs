use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::accounting::account_wallet;
use crate::envio::{EnvioClient, MARKET_INGESTION_PROFILE};
use crate::error::{DataQualityError, Result};
use crate::evidence::manifest;
use crate::ledger::replay;
use crate::registry::ProtocolRegistry;
use crate::serve::{run_service, unavailable_context, wallet_value, ApiSnapshot};

pub struct LiveConfig {
    pub client: EnvioClient,
    pub registry: ProtocolRegistry,
    pub accounts: Vec<u64>,
    pub rpc_url: String,
    pub refresh_seconds: u64,
    pub max_lag_blocks: u64,
    pub database_url: Option<String>,
    pub nansen: Option<crate::context::NansenClient>,
}

/// Independent chain head prevents a stopped indexer's own source watermark
/// from certifying its freshness. No wallet or signing method is used.
pub fn chain_head(endpoint: &str) -> Result<u64> {
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err(DataQualityError::msg("RPC endpoint must use HTTP or HTTPS"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let chain: Value = agent
        .post(endpoint)
        .send_json(json!({"jsonrpc":"2.0","id":2,"method":"eth_chainId","params":[]}))
        .map_err(|_| DataQualityError::msg("independent Monad chain identity request failed"))?
        .body_mut()
        .read_json()
        .map_err(|_| DataQualityError::msg("invalid Monad chain identity response"))?;
    if chain["result"]
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .and_then(|s| u64::from_str_radix(s, 16).ok())
        != Some(143)
    {
        return Err(DataQualityError::msg(
            "independent RPC is not Monad chain 143",
        ));
    }
    let response: Value = agent
        .post(endpoint)
        .send_json(json!({"jsonrpc": "2.0", "id": 1, "method": "eth_blockNumber", "params": []}))
        .map_err(|_| DataQualityError::msg("independent Monad head request failed"))?
        .body_mut()
        .read_json()
        .map_err(|_| DataQualityError::msg("invalid Monad head response"))?;
    if response.get("error").is_some() {
        return Err(DataQualityError::msg("Monad RPC returned an error"));
    }
    let value = response["result"]
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .ok_or_else(|| DataQualityError::msg("Monad RPC head is missing"))?;
    u64::from_str_radix(value, 16).map_err(|_| DataQualityError::msg("invalid Monad head number"))
}

pub fn fetch_snapshot(config: &LiveConfig) -> Result<ApiSnapshot> {
    if config.accounts.is_empty()
        || config.accounts.len() > 20
        || config
            .accounts
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != config.accounts.len()
    {
        return Err(DataQualityError::msg(
            "live watchlist requires 1..20 distinct account ids",
        ));
    }
    let coverage = config.client.fetch_coverage(config.registry.chain_id)?;
    let head = chain_head(&config.rpc_url)?;
    if !coverage.is_ready
        || coverage.evidence.processed_block > head
        || head - coverage.evidence.processed_block > config.max_lag_blocks
    {
        return Err(DataQualityError::msg(
            "Envio coverage is unready, inconsistent, or stale against the independent Monad head",
        ));
    }
    let mut events = Vec::new();
    let mut wallets = Vec::new();
    let mut as_of = None;
    let mut market_cache: BTreeMap<u32, Vec<crate::events::CanonicalEvent>> = BTreeMap::new();
    let mut market_marks = BTreeMap::new();
    for account in &config.accounts {
        let slice = config.client.fetch_account_at(*account, &coverage)?;
        let eligibility = slice.replay_eligibility(&config.registry)?;
        let mut wallet = if eligibility.eligible {
            let mut ledger = replay(&slice.events, &config.registry, &slice.as_of)?;
            let market_profile = slice.events.iter().all(|e| {
                e.provenance
                    .as_ref()
                    .is_some_and(|p| p.ingestion_profile == MARKET_INGESTION_PROFILE)
            });
            let open_markets: BTreeSet<_> = ledger
                .open_positions()
                .iter()
                .map(|p| p.position_id.perpetual_id)
                .collect();
            if market_profile {
                let uncached: Vec<_> = open_markets
                    .iter()
                    .filter(|id| !market_cache.contains_key(id))
                    .copied()
                    .collect();
                if !uncached.is_empty() {
                    let inputs = config.client.fetch_market_inputs_at(
                        &uncached,
                        &coverage,
                        &slice.as_of,
                        &config.registry,
                    )?;
                    for id in &uncached {
                        market_cache.insert(
                            *id,
                            inputs
                                .events
                                .iter()
                                .filter(|e| e.perpetual_id == Some(*id))
                                .cloned()
                                .collect(),
                        );
                    }
                }
                let mut canonical = slice.events.clone();
                for id in &open_markets {
                    canonical.extend(market_cache[id].iter().cloned());
                }
                ledger = replay(&canonical, &config.registry, &slice.as_of)?;
            }
            let marks: Vec<_> = ledger.market_marks.values().cloned().collect();
            market_marks.extend(ledger.market_marks.clone());
            let wallet = account_wallet(&ledger, *account, &slice.as_of, &marks, false)?;
            let mut value = wallet_value(&wallet, true);
            value["marketInputProfile"] = json!(if market_profile {
                "canonical-v3"
            } else {
                "unavailable-legacy-profile"
            });
            value["marketInputs"] = json!(if market_profile {
                open_markets
                    .iter()
                    .map(|id| crate::funding::timeline(&ledger.events, *id, &slice.as_of))
                    .collect::<crate::Result<Vec<_>>>()?
            } else {
                Vec::new()
            });
            value["quality"] = json!(if market_profile
                && wallet
                    .positions
                    .iter()
                    .filter(|p| p.status == "open")
                    .all(|p| p.mark.is_some())
            {
                "funding-checkpoint-unverified"
            } else {
                "marks-unavailable"
            });
            value
        } else {
            json!({"accountId": account, "owner": null, "freeBalance": null, "realizedPnl": null,
                "unrealizedPnl": null, "fees": null, "realizedFunding": null, "positions": [], "warnings": [eligibility.reason]})
        };
        wallet["replayEligible"] = json!(eligibility.eligible);
        wallet["replayBasis"] = json!(eligibility.basis);
        if !eligibility.eligible {
            wallet["quality"] = json!("incomplete-history");
        }
        wallet["balanceNote"] =
            json!("Exact free balance requires complete balance-event coverage.");
        wallet["context"] =
            if let (Some(client), Some(owner)) = (&config.nansen, wallet["owner"].as_str()) {
                client.labels(owner, slice.as_of.timestamp_ms)
            } else {
                unavailable_context()
            };
        wallets.push(wallet);
        as_of = Some(slice.as_of);
        events.extend(slice.events);
        if events.len() > 100_000 {
            return Err(DataQualityError::msg(
                "combined snapshot exceeds 100000 event bound",
            ));
        }
    }
    let as_of = as_of.ok_or_else(|| DataQualityError::msg("live snapshot has no cutoff"))?;
    for inputs in market_cache.values() {
        events.extend(inputs.iter().cloned());
    }
    let mut unique = BTreeMap::new();
    for event in events {
        if let Some(previous) = unique.insert(event.event_id()?.key(), event.clone()) {
            if previous != event {
                return Err(DataQualityError::msg(
                    "canonical market/account event identity has conflicting facts",
                ));
            }
        }
    }
    let mut events: Vec<_> = unique.into_values().collect();
    if events.len() > 100_000 {
        return Err(DataQualityError::msg(
            "combined canonical input bound exceeded",
        ));
    }
    events.sort_by_key(|e| (e.block_number, e.log_index));
    let evidence = events.iter().map(|event| Ok(json!({
        "eventId": event.event_id()?.key(), "abi": event.abi_event_name, "kind": format!("{:?}", event.kind),
        "blockNumber": event.block_number, "blockHash": event.block_hash, "txHash": event.tx_hash,
        "logIndex": event.log_index, "timestampMs": event.timestamp_ms, "accountId": event.account_id, "perpetualId": event.perpetual_id,
        "markPricePns": event.mark_price_pns.map(|v| v.to_string()), "fundingEventBlock": event.funding_event_block,
        "fundingPaymentPns": event.funding_payment_pns.map(|v| v.to_string()), "fundingSumPns": event.funding_sum_pns.map(|v| v.to_string()),
        "fundingAllowOverwrite": event.funding_allow_overwrite, "fundingScalingExponent": event.funding_scaling_exp,
        "provenance": event.provenance.as_ref().map(|p| json!({"schemaVersion": p.schema_version, "handlerVersion": p.handler_version,
            "classifierVersion": p.classifier_version, "ingestionProfile": p.ingestion_profile, "abiFingerprint": p.abi_fingerprint}))
    }))).collect::<Result<Vec<_>>>()?;
    let mut manifest = manifest(
        &events,
        &as_of,
        &coverage.evidence,
        "Envio account watchlist",
    )?;
    manifest["registryInputsHash"] = json!(crate::evidence::digest(&config.registry)?);
    manifest["marketMarksHash"] = if market_marks.is_empty() {
        Value::Null
    } else {
        json!(crate::evidence::digest(&market_marks)?)
    };
    let context = if config.nansen.is_some() {
        json!({"source":"Nansen","status":"per-account","attribution":"Powered by Nansen API","affectsCanonicalFacts":false,
            "reason":"See each wallet's labels and observation time. Context can be observed after the ledger cutoff."})
    } else {
        unavailable_context()
    };
    let snapshot = ApiSnapshot {
        fixture_name: "Envio account watchlist".to_string(), chain_id: config.registry.chain_id,
        exchange_address: config.registry.exchange_address.clone(), as_of_block: as_of.block_number,
        as_of_timestamp_ms: as_of.timestamp_ms, start_block: coverage.evidence.start_block,
        processed_block: coverage.evidence.processed_block,
        source_note: "Selected account events only. Global metrics require a complete protocol ledger. Eligible v3 marks are source-linked; position funding checkpoints remain unverified.".to_string(),
        protocol: json!({"scope": "selected-accounts", "quality": "unavailable", "takerVolume": null, "openInterest": null,
            "tvl": null, "protocolFees": null, "liquidations": null, "activeAccounts": null, "markets": [],
            "warnings": ["A watchlist cannot prove protocol totals. Global metrics are unavailable."]}),
        coverage: json!({"chainId": config.registry.chain_id, "startBlock": coverage.evidence.start_block,
            "processedBlock": coverage.evidence.processed_block, "independentHeadBlock": head,
            "coverageLagBlocks": head - coverage.evidence.processed_block, "asOfBlock": as_of.block_number,
            "asOfTimestampMs": as_of.timestamp_ms, "eventSilenceBlocks": coverage.evidence.processed_block - coverage.latest_event.block_number,
            "quality": "covered-account-slice", "eventCount": events.len(), "rangeNote": "Selected account history only; eligibility is reported for each account."}),
        wallets: json!(wallets), events: json!(evidence), mode: "live".to_string(), manifest, context, events_available: true,
    };
    Ok(snapshot)
}

struct Observed {
    snapshot: Option<ApiSnapshot>,
    error: String,
    successful_at: Instant,
    advanced_at: Instant,
    last_block: Option<u64>,
    quarantined: bool,
    last_manifest: Option<Value>,
}

fn accept_snapshot(
    observed: &mut Observed,
    snapshot: ApiSnapshot,
    database_url: Option<&str>,
    now: Instant,
) -> Result<()> {
    let inconsistent = observed.last_manifest.as_ref().is_some_and(|prior| {
        let previous = (
            prior["asOfBlock"].as_u64().unwrap_or(0),
            prior["asOfLogIndex"].as_u64().unwrap_or(u32::MAX as u64),
        );
        let current = (
            snapshot.as_of_block,
            snapshot.manifest["asOfLogIndex"]
                .as_u64()
                .unwrap_or(u32::MAX as u64),
        );
        current < previous
            || (current == previous
                && (prior["canonicalInputsHash"] != snapshot.manifest["canonicalInputsHash"]
                    || prior["asOfBlockHash"] != snapshot.manifest["asOfBlockHash"]
                    || prior["registryInputsHash"] != snapshot.manifest["registryInputsHash"]
                    || prior["marketMarksHash"] != snapshot.manifest["marketMarksHash"]))
    });
    if observed.quarantined
        || inconsistent
        || observed
            .last_block
            .is_some_and(|block| snapshot.processed_block < block)
    {
        observed.quarantined = true;
        return Err(DataQualityError::msg("Coverage or canonical facts regressed or changed at the same cutoff; restart only after reconciliation."));
    }
    if observed.last_block != Some(snapshot.processed_block) {
        observed.advanced_at = now;
    }
    if now.duration_since(observed.advanced_at) > Duration::from_secs(120) {
        return Err(DataQualityError::msg("Processed coverage has stalled."));
    }
    if let Some(url) = database_url {
        crate::publication::publish(url, "live-watchlist", &snapshot)?;
    }
    observed.last_manifest = Some(snapshot.manifest.clone());
    observed.last_block = Some(snapshot.processed_block);
    observed.successful_at = now;
    observed.snapshot = Some(snapshot);
    Ok(())
}

pub fn run_live(config: LiveConfig, bind: SocketAddr) -> Result<()> {
    if !(1..=60).contains(&config.refresh_seconds) {
        return Err(DataQualityError::msg("refresh-seconds must be 1..60"));
    }
    let state = Arc::new(RwLock::new(Observed {
        snapshot: None,
        error: "Live source is initializing.".to_string(),
        successful_at: Instant::now(),
        advanced_at: Instant::now(),
        last_block: None,
        quarantined: false,
        last_manifest: None,
    }));
    let writer = state.clone();
    std::thread::spawn(move || loop {
        let result = fetch_snapshot(&config);
        if let Ok(mut observed) = writer.write() {
            match result {
                Ok(snapshot) => {
                    if let Err(error) = accept_snapshot(
                        &mut observed,
                        snapshot,
                        config.database_url.as_deref(),
                        Instant::now(),
                    ) {
                        observed.snapshot = None;
                        observed.error = error.to_string();
                    }
                }
                Err(error) => {
                    observed.snapshot = None;
                    observed.error = error.to_string();
                }
            }
        }
        std::thread::sleep(Duration::from_secs(config.refresh_seconds));
    });
    run_service(
        move || {
            let observed = state
                .read()
                .map_err(|_| DataQualityError::msg("live state lock failed"))?;
            if observed.successful_at.elapsed() > Duration::from_secs(90)
                || observed.advanced_at.elapsed() > Duration::from_secs(120)
            {
                return Err(DataQualityError::msg(
                    "live source observation is stale or processed coverage has stalled",
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

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> ApiSnapshot {
        crate::serve::build_snapshot(
            &crate::pipeline::run_fixture(
                crate::events::repo_root().join("fixtures/golden/open-position-as-of.json"),
                Some(0),
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn observed(now: Instant) -> Observed {
        Observed {
            snapshot: None,
            error: String::new(),
            successful_at: now,
            advanced_at: now,
            last_block: None,
            quarantined: false,
            last_manifest: None,
        }
    }
    #[test]
    fn repeated_metadata_cannot_hide_a_stalled_processed_watermark() {
        let now = Instant::now();
        let mut state = observed(now);
        accept_snapshot(&mut state, snapshot(), None, now).unwrap();
        assert!(
            accept_snapshot(&mut state, snapshot(), None, now + Duration::from_secs(121)).is_err()
        );
    }
    #[test]
    fn changed_same_cutoff_facts_remain_quarantined_after_source_recovers() {
        let now = Instant::now();
        for field in [
            "canonicalInputsHash",
            "asOfBlockHash",
            "registryInputsHash",
            "marketMarksHash",
        ] {
            let mut state = observed(now);
            accept_snapshot(&mut state, snapshot(), None, now).unwrap();
            let mut changed = snapshot();
            changed.manifest[field] = json!("sha256:changed");
            assert!(
                accept_snapshot(&mut state, changed, None, now).is_err(),
                "{field}"
            );
            assert!(accept_snapshot(&mut state, snapshot(), None, now).is_err());
            assert!(state.quarantined);
        }
    }
}
