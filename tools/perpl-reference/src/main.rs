//! Independent, read-only SDK verifier. This executable is not a serving ledger.
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    time::Duration,
};

use alloy::{
    eips::BlockId,
    providers::{Provider, ProviderBuilder},
};
use fastnum::{UD64, UD128, decimal::RoundingMode};
use perpl_sdk::{
    Chain,
    state::{PositionType, SnapshotBuilder},
    types::AccountAddressOrID,
};
use perppulse::{
    AsOf,
    accounting::{account_wallet, isolated_liquidation_price, position_snapshot},
    envio::EnvioClient,
    events::MarketMark,
    evidence, load_registry,
    money::margin_fraction,
    registry::{SIDE_LONG, SIDE_SHORT},
    replay,
};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::Deserialize;
use serde_json::{Value, json};

const SDK_COMMIT: &str = "dbb37c59f6aef03e38d0787eb9c968f59f652617";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    rpc_url: String,
    block: u64,
    block_hash: String,
    graphql_url: String,
    registry_path: String,
    account_ids: Vec<u32>,
    market_ids: Vec<u32>,
    #[serde(default)]
    risk_diagnostics: bool,
    #[serde(default)]
    market_diagnostics: bool,
    #[serde(default)]
    funding_checkpoints: bool,
}

fn decimal(value: impl std::fmt::Display) -> Result<String, String> {
    Decimal::from_str_exact(&value.to_string())
        .map(|n| n.normalize().to_string())
        .map_err(|_| "SDK numeric value cannot be represented exactly".into())
}

fn number(value: impl std::fmt::Display) -> Result<Decimal, String> {
    Decimal::from_str_exact(&value.to_string())
        .map_err(|_| "SDK decimal exceeds exact range".into())
}

fn risk_check(name: &str, actual: Decimal, expected: Decimal, decimals: Option<u32>) -> Value {
    let comparable = |value: Decimal| decimals.map_or(value, |scale| value.trunc_with_scale(scale));
    json!({"field": name, "formulaValue": actual.to_string(), "sdkValue": expected.to_string(),
        "comparisonDecimals": decimals, "rounding": if decimals.is_some() {"truncate toward zero"} else {"exact"},
        "status": if comparable(actual) == comparable(expected) {"matched"} else {"mismatch"}})
}

// Snapshot premiumPnlCNS and deltaPnlCNS are separate native collateral
// components. Project each before addition, not the final exact equity.
fn funded_projection(
    price: Decimal,
    premium: Decimal,
    deposit: Decimal,
    maintenance: Decimal,
    decimals: u32,
) -> Result<[Decimal; 4], String> {
    let funding = premium.trunc_with_scale(decimals);
    let pnl = price
        .trunc_with_scale(decimals)
        .checked_add(funding)
        .ok_or("Projected PnL overflow")?;
    let equity = deposit
        .checked_add(pnl)
        .ok_or("Projected equity overflow")?;
    let buffer = equity
        .checked_sub(maintenance)
        .ok_or("Projected buffer overflow")?;
    Ok([funding, pnl, equity, buffer])
}

// Verifier-only projection based on the pinned MIT SDK's
// state/position.rs::effective_entry_price; see LICENSES/Perpl-dex-sdk.txt.
fn sdk_entry_value(stored: i128, residue: u32, side: u8, decimals: u32) -> Result<UD64, String> {
    if stored <= 0 || residue >= 65536 || decimals > 18 || !matches!(side, SIDE_LONG | SIDE_SHORT) {
        return Err("Unsupported SDK entry projection inputs".into());
    }
    let stored = u64::try_from(stored).map_err(|_| "SDK native entry exceeds u64")?;
    let converter = perpl_sdk::num::Converter::new(decimals as u8);
    if residue == 0 {
        return Ok(converter.from_u64(stored));
    }
    let mut base = UD64::from_u64(stored).with_rounding_mode(RoundingMode::Floor);
    if side == SIDE_LONG {
        base -= UD64::ONE;
    }
    Ok((base
        + UD64::from_u32(residue).with_rounding_mode(RoundingMode::Floor)
            / UD64::from_u64(65536).with_rounding_mode(RoundingMode::Floor))
        / converter.scale())
}

fn sdk_entry_projection(
    stored: i128,
    residue: u32,
    side: u8,
    decimals: u32,
) -> Result<Decimal, String> {
    number(sdk_entry_value(stored, residue, side, decimals)?)
}

fn sdk_maintenance_projection(
    stored: i128,
    residue: u32,
    side: u8,
    decimals: u32,
    size: Decimal,
    inverse: Decimal,
) -> Result<Decimal, String> {
    if size <= Decimal::ZERO || inverse <= Decimal::ZERO {
        return Err("Invalid SDK margin projection inputs".into());
    }
    let entry: UD128 = sdk_entry_value(stored, residue, side, decimals)?.resize();
    let sdk_size = size
        .to_string()
        .parse::<UD64>()
        .map_err(|_| "SDK size projection failed")?
        .with_rounding_mode(RoundingMode::Floor);
    let sdk_inverse = inverse
        .to_string()
        .parse::<UD64>()
        .map_err(|_| "SDK margin projection failed")?
        .with_rounding_mode(RoundingMode::Floor);
    if number(sdk_size)? != size || number(sdk_inverse)? != inverse {
        return Err("SDK size or inverse cannot be projected exactly".into());
    }
    let size: UD128 = sdk_size.resize();
    let inverse: UD128 = sdk_inverse.resize();
    number(entry * size / inverse)
}

async fn header<P: Provider>(provider: &P, block: u64) -> Result<AsOf, String> {
    let value = provider
        .get_block(BlockId::number(block))
        .await
        .map_err(|_| "Pinned RPC block header request failed")?
        .ok_or("Pinned RPC block header is missing")?;
    if value.header.number != block {
        return Err("RPC returned a different block number".into());
    }
    let timestamp_ms = value
        .header
        .timestamp
        .checked_mul(1000)
        .and_then(|n| i64::try_from(n).ok())
        .ok_or("RPC timestamp overflow")?;
    AsOf::new(
        143,
        block,
        value.header.hash.to_string(),
        timestamp_ms,
        None,
    )
    .map_err(|_| "RPC block header is invalid".into())
}

async fn run(config: Config) -> Result<Value, String> {
    // The public wrapper supplies the method/block/budget-limited loopback gate.
    let url = config
        .rpc_url
        .parse::<alloy::transports::http::reqwest::Url>()
        .map_err(|_| "Invalid local RPC gate URL")?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("SDK RPC must use the bounded loopback gate".into());
    }
    let accounts: BTreeSet<_> = config.account_ids.iter().copied().collect();
    let markets: BTreeSet<_> = config.market_ids.iter().copied().collect();
    if accounts.is_empty()
        || accounts.len() > 20
        || accounts.len() != config.account_ids.len()
        || markets.is_empty()
        || markets.len() > 5
        || markets.len() != config.market_ids.len()
        || accounts.contains(&0)
        || markets.contains(&0)
    {
        return Err("Require 1-20 unique accounts and 1-5 unique markets".into());
    }
    if config.funding_checkpoints && !(config.market_diagnostics && config.risk_diagnostics) {
        return Err("Funding checkpoints require both market and risk diagnostics".into());
    }
    let registry = load_registry(&config.registry_path).map_err(|e| e.to_string())?;
    registry.require_chain(143).map_err(|e| e.to_string())?;
    for id in &markets {
        registry.market(*id).map_err(|e| e.to_string())?;
    }
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_http(url);
    if provider
        .get_chain_id()
        .await
        .map_err(|_| "RPC chain identity request failed")?
        != 143
    {
        return Err("RPC chain is not Monad mainnet".into());
    }
    let as_of = header(&provider, config.block).await?;
    if as_of.block_hash != config.block_hash {
        return Err("RPC block hash differs from the required hash".into());
    }
    let chain = Chain::mainnet();
    if !chain
        .exchange()
        .to_string()
        .eq_ignore_ascii_case(&registry.exchange_address)
    {
        return Err("SDK Exchange differs from the canonical registry".into());
    }
    let snapshot = tokio::time::timeout(
        Duration::from_secs(120),
        SnapshotBuilder::new(&chain, provider.clone())
            .at_block(BlockId::number(config.block))
            .with_perpetuals(config.market_ids.clone())
            .with_accounts(
                config
                    .account_ids
                    .iter()
                    .map(|n| AccountAddressOrID::ID(*n))
                    .collect(),
            )
            .with_orders_per_batch(250)
            .with_positions_per_batch(100)
            .build(),
    )
    .await
    .map_err(|_| "SDK snapshot reached the 120-second limit")?
    .map_err(|_| "SDK snapshot failed; no reference was accepted")?;
    if snapshot.instant().block_number() != as_of.block_number
        || snapshot.instant().block_timestamp() != (as_of.timestamp_ms / 1000) as u64
        || snapshot.chain().chain_id() != as_of.chain_id
        || snapshot.accounts().len() != accounts.len()
        || snapshot.perpetuals().len() != markets.len()
    {
        return Err("SDK snapshot coverage/instant differs from requested scope".into());
    }
    if u32::from(snapshot.collateral_converter().decimals()) != registry.collateral.decimals {
        return Err("SDK collateral decimals differ from canonical registry".into());
    }
    let mut metadata = Vec::new();
    for id in &markets {
        let market = snapshot
            .perpetuals()
            .get(id)
            .ok_or("SDK requested market is missing")?;
        let spec = registry.market(*id).map_err(|e| e.to_string())?;
        if u32::from(market.price_converter().decimals()) != spec.price_decimals
            || u32::from(market.size_converter().decimals()) != spec.size_decimals
            || number(market.initial_margin())?
                != margin_fraction(spec.init_margin_frac_hdths, "initial margin")
                    .map_err(|e| e.to_string())?
            || number(market.maintenance_margin())?
                != margin_fraction(spec.maint_margin_frac_hdths, "maintenance margin")
                    .map_err(|e| e.to_string())?
        {
            return Err(
                "SDK market scales or margin parameters differ from the canonical registry".into(),
            );
        }
        metadata.push(json!({"perpetualId": id, "symbol": market.symbol(),
            "priceDecimals": market.price_converter().decimals(),
            "sizeDecimals": market.size_converter().decimals(),
            "initialMarginInverse": decimal(market.initial_margin())?,
            "maintenanceMarginInverse": decimal(market.maintenance_margin())?,
            "markPrice": decimal(market.mark_price())?,
            "markTimestampSeconds": market.mark_price_timestamp(),
            "priceMaxAgeSeconds": market.price_max_age_sec(),
            "sdkMarkObsolete": market.is_mark_price_obsolete(),
            "role": "reference observation; not adopted into canonical risk accounting"}));
    }
    let client = EnvioClient::new(
        config.graphql_url,
        std::env::var("HASURA_ADMIN_SECRET").ok(),
        500,
        10_000,
    )
    .map_err(|e| e.to_string())?;
    let mut references = Vec::new();
    let mut scorecards = Vec::new();
    let mut manifests = Vec::new();
    let mut risk_references = Vec::new();
    let mut risk_scorecards = Vec::new();
    let mut funding_scorecards = Vec::new();
    let mut market_scorecards = Vec::new();
    let mut market_inputs = Vec::new();
    let mut funding_timelines = Vec::new();
    if config.market_diagnostics {
        market_inputs = client
            .fetch_archived_market_inputs_at(&config.market_ids, &as_of, &registry)
            .map_err(|e| e.to_string())?
            .events;
        let ledger = replay(&market_inputs, &registry, &as_of).map_err(|e| e.to_string())?;
        for id in &markets {
            let canonical = ledger
                .market_marks
                .get(id)
                .ok_or("Canonical mark is missing")?;
            let market = snapshot.perpetuals().get(id).ok_or("SDK market missing")?;
            let spec = registry.market(*id).map_err(|e| e.to_string())?;
            let native = number(market.mark_price())?
                .checked_mul(Decimal::from(10u64.pow(spec.price_decimals)))
                .ok_or("SDK mark scaling overflow")?;
            if !native.fract().is_zero() {
                return Err("SDK mark is not an exact native integer".into());
            }
            let sdk_time = market
                .mark_price_timestamp()
                .checked_mul(1000)
                .and_then(|v| i64::try_from(v).ok())
                .ok_or("SDK mark time overflow")?;
            let checks = vec![
                risk_check("markPricePns", number(canonical.mark_pns)?, native, None),
                risk_check(
                    "markTimestampMs",
                    number(canonical.timestamp_ms)?,
                    number(sdk_time)?,
                    None,
                ),
            ];
            market_scorecards.push(json!({"perpetualId":id,"sourceEventId":ledger.mark_event_ids[id],
                "sourceBlock":canonical.block_number,"sourceBlockHash":canonical.block_hash,"sourceLogIndex":canonical.log_index,
                "markAgeMs":as_of.timestamp_ms-canonical.timestamp_ms,
                "status":if checks.iter().all(|c| c["status"]=="matched") {"matched"} else {"mismatch"},"checks":checks}));
            funding_timelines.push(
                perppulse::funding::timeline(&market_inputs, *id, &as_of)
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    let mut total_events = 0usize;
    for id in &accounts {
        let slice = client
            .fetch_archived_account_at(u64::from(*id), &as_of)
            .map_err(|e| e.to_string())?;
        total_events = total_events
            .checked_add(slice.events.len())
            .ok_or("Event count overflow")?;
        if total_events > 100_000 {
            return Err(
                "Combined canonical event count exceeded 100000; no partial reference accepted"
                    .into(),
            );
        }
        if !slice
            .replay_eligibility(&registry)
            .map_err(|e| e.to_string())?
            .eligible
        {
            return Err("Canonical account history is not eligible for replay".into());
        }
        let mut combined = slice.events.clone();
        combined.extend(market_inputs.iter().cloned());
        let ledger = if config.funding_checkpoints {
            perppulse::ledger::replay_with_funding_coverage(
                &combined,
                &registry,
                &as_of,
                &perppulse::funding_checkpoint::FundingCoverage {
                    start_block: slice.coverage.evidence.start_block,
                    end_block: as_of.block_number,
                    market_ids: config.market_ids.clone(),
                },
            )
        } else {
            replay(&combined, &registry, &as_of)
        }
        .map_err(|e| e.to_string())?;
        let marks: Vec<_> = ledger.market_marks.values().cloned().collect();
        let wallet = account_wallet(&ledger, u64::from(*id), &as_of, &marks, false)
            .map_err(|e| e.to_string())?;
        let account = snapshot
            .accounts()
            .get(id)
            .ok_or("SDK account is missing")?;
        let mut positions = Vec::new();
        for market_id in &markets {
            if config.risk_diagnostics
                && let Some(p) = account.positions().get(market_id)
            {
                let market = snapshot
                    .perpetuals()
                    .get(market_id)
                    .ok_or("SDK market missing")?;
                risk_references.push(json!({"accountId": id, "perpetualId": market_id,
                        "deltaPnl": decimal(p.delta_pnl())?, "premiumPnl": decimal(p.premium_pnl())?,
                        "totalUnrealizedPnl": decimal(p.pnl())?,
                        "maintenanceMargin": decimal(p.maintenance_margin_requirement())?,
                        "liquidationPrice": decimal(p.liquidation_price())?,
                        "markPrice": decimal(market.mark_price())?,
                        "markTimestampSeconds": market.mark_price_timestamp(),
                        "fundingSumDecimals": market.funding_sum_converter().decimals(),
                        "role": "independent risk diagnostic only; no SDK risk inputs enter the canonical ledger"}));
                let canonical = ledger
                    .positions
                    .values()
                    .find(|p| {
                        p.position_id.account_id == u64::from(*id)
                            && p.position_id.perpetual_id == *market_id
                            && p.is_open()
                    })
                    .ok_or("SDK open risk diagnostic has no canonical open position")?;
                let spec = registry.market(*market_id).map_err(|e| e.to_string())?;
                if config.funding_checkpoints {
                    let canonical_position = wallet
                        .positions
                        .iter()
                        .find(|p| p.perpetual_id == *market_id && p.status == "open")
                        .ok_or("Canonical funding position missing")?;
                    let mut checks = Vec::new();
                    if let Some(funding) = canonical_position.unrealized_funding {
                        let reference_pnl = number(p.pnl())?;
                        let reference_equity = number(p.deposit())?
                            .checked_add(reference_pnl)
                            .ok_or("Reference equity overflow")?;
                        let reference_buffer = reference_equity
                            .checked_sub(number(p.maintenance_margin_requirement())?)
                            .ok_or("Reference buffer overflow")?;
                        let projected = funded_projection(
                            canonical_position
                                .unrealized_price_pnl
                                .ok_or("Canonical price PnL missing")?,
                            funding,
                            canonical_position.deposit,
                            sdk_maintenance_projection(
                                canonical.entry_pns,
                                canonical.entry_residue_pnsq16,
                                canonical.side,
                                spec.price_decimals,
                                canonical_position.size,
                                margin_fraction(spec.maint_margin_frac_hdths, "maint_margin_frac")
                                    .map_err(|e| e.to_string())?,
                            )?,
                            registry.collateral.decimals,
                        )?;
                        for (index, (field, canonical, reference, decimals)) in [
                            (
                                "canonicalUnsettledFundingAtCollateralUnits",
                                funding,
                                number(p.premium_pnl())?,
                                registry.collateral.decimals,
                            ),
                            (
                                "canonicalTotalPnlAtCollateralUnits",
                                canonical_position
                                    .unrealized_pnl
                                    .ok_or("Canonical funded PnL missing")?,
                                reference_pnl,
                                registry.collateral.decimals,
                            ),
                            (
                                "canonicalEquityAtCollateralUnits",
                                canonical_position
                                    .fair_market_value
                                    .ok_or("Canonical funded equity missing")?,
                                reference_equity,
                                registry.collateral.decimals,
                            ),
                            (
                                "canonicalBufferAtCollateralUnits",
                                canonical_position
                                    .liquidation_buffer
                                    .ok_or("Canonical funded buffer missing")?,
                                reference_buffer,
                                registry.collateral.decimals,
                            ),
                            (
                                "canonicalFundedLiquidationAtPriceTicks",
                                canonical_position
                                    .liquidation_price
                                    .ok_or("Canonical funded liquidation missing")?,
                                number(p.liquidation_price())?,
                                spec.price_decimals,
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let comparison = projected.get(index).copied().unwrap_or(canonical);
                            let mut check =
                                risk_check(field, comparison, reference, Some(decimals));
                            check["canonicalExactValue"] = json!(canonical.to_string());
                            check["comparisonRepresentation"] = json!(if index < 4 {
                                "Separate collateral-native price/funding components before addition; SDK-width canonical maintenance for buffer. Exact ledger values are retained"
                            } else {
                                "Exact canonical funded liquidation compared at market price ticks"
                            });
                            checks.push(check);
                        }
                    }
                    funding_scorecards.push(json!({"accountId": id, "perpetualId": market_id,
                        "status": if checks.is_empty() {"unverified"} else if checks.iter().all(|c| c["status"] == "matched") {"matched"} else {"mismatch"},
                        "checks": checks, "checkpoint": canonical_position.funding_checkpoint,
                        "role": "Canonical covered funding compared to independent SDK state; unknown checkpoints are not scored as matches."}));
                }
                let native = number(market.mark_price())?
                    .checked_mul(Decimal::from(10u64.pow(spec.price_decimals)))
                    .ok_or("SDK mark native scale overflow")?;
                if !native.fract().is_zero() {
                    return Err("SDK mark is not an exact native price".into());
                }
                let mark = MarketMark {
                    perpetual_id: *market_id,
                    mark_pns: native.to_i128().ok_or("SDK mark native range overflow")?,
                    oracle_pns: None,
                    block_number: as_of.block_number,
                    block_hash: Some(as_of.block_hash.clone()),
                    log_index: None,
                    timestamp_ms: market
                        .mark_price_timestamp()
                        .checked_mul(1000)
                        .and_then(|n| i64::try_from(n).ok())
                        .ok_or("SDK mark timestamp overflow")?,
                };
                // This is a reference-input scenario. The mark block is the SDK
                // observation cutoff, not independently indexed MarkUpdated provenance.
                let scenario = position_snapshot(canonical, &registry, &as_of, &[mark], true)
                    .map_err(|e| e.to_string())?;
                let premium = number(p.premium_pnl())?;
                let conditional_liq = isolated_liquidation_price(
                    if scenario.side == "long" {
                        SIDE_LONG
                    } else {
                        SIDE_SHORT
                    },
                    scenario.entry,
                    scenario.size,
                    scenario.deposit,
                    number(market.maintenance_margin())?,
                    premium,
                )
                .map_err(|e| e.to_string())?;
                let mut checks = vec![
                    risk_check(
                        "entryMaintenanceMarginAtSdkArithmetic",
                        sdk_maintenance_projection(
                            canonical.entry_pns,
                            canonical.entry_residue_pnsq16,
                            canonical.side,
                            spec.price_decimals,
                            scenario.size,
                            number(market.maintenance_margin())?,
                        )?,
                        number(p.maintenance_margin_requirement())?,
                        None,
                    ),
                    risk_check(
                        "pricePnlAtCollateralUnits",
                        scenario
                            .unrealized_price_pnl
                            .ok_or("Price scenario missing")?,
                        number(p.delta_pnl())?,
                        Some(registry.collateral.decimals),
                    ),
                    risk_check(
                        "liquidationWithReferenceFundingAtPriceTicks",
                        conditional_liq,
                        number(p.liquidation_price())?,
                        Some(spec.price_decimals),
                    ),
                ];
                checks[0]["canonicalExactValue"] =
                    json!(scenario.maintenance_margin.map(|v| v.to_string()));
                checks[0]["comparisonRepresentation"] = json!(
                    "Pinned SDK UD64 entry flooring followed by UD128 margin arithmetic; canonical exact arithmetic is unchanged"
                );
                if config.market_diagnostics {
                    let canonical_position = wallet
                        .positions
                        .iter()
                        .find(|p| p.perpetual_id == *market_id && p.status == "open")
                        .ok_or("Canonical price PnL position missing")?;
                    checks.push(risk_check(
                        "canonicalPricePnlAtCollateralUnits",
                        canonical_position
                            .unrealized_price_pnl
                            .ok_or("Canonical price PnL missing")?,
                        number(p.delta_pnl())?,
                        Some(registry.collateral.decimals),
                    ));
                    checks.last_mut().unwrap()["markSourceEventId"] =
                        json!(canonical_position.mark_event_id);
                }
                risk_scorecards.push(json!({"accountId": id, "perpetualId": market_id,
                        "status": if checks.iter().all(|v| v["status"] == "matched") {"matched"} else {"mismatch"}, "checks": checks,
                        "zeroFundingLiquidationPrice": scenario.zero_funding_liquidation_price.map(|v| v.to_string()),
                        "referencePremiumPnl": premium.to_string(), "conditionalFundedLiquidationPrice": conditional_liq.to_string(),
                        "canonicalActualLiquidationPrice": scenario.liquidation_price.map(|v| v.to_string()),
                        "role": "SDK mark/funding scenarios are verifier inputs only; canonical price checks use indexed marks. Canonical checkpoint acceptance is reported separately in fundingScorecards; unsupported checkpoints remain unverified."}));
            }
            positions.push(if let Some(p) = account.positions().get(market_id) {
                json!({"perpetualId": market_id, "status": "open",
                    "size": decimal(p.size())?, "deposit": decimal(p.deposit())?,
                    "side": match p.r#type() {PositionType::Long => "long", PositionType::Short => "short"},
                    "entryPrice": decimal(p.entry_price())?})
            } else {
                json!({"perpetualId": market_id, "status": "closed", "size": "0",
                    "deposit": "0", "side": Value::Null, "entryPrice": Value::Null})
            });
        }
        let reference = json!({"source": "perpl-dex-sdk", "sdkCommit": SDK_COMMIT,
            "execution": "SnapshotBuilder::build", "chainId": 143, "accountId": id,
            "asOfBlock": as_of.block_number, "asOfBlockHash": as_of.block_hash,
            "asOfTimestampMs": as_of.timestamp_ms, "asOfLogIndex": Value::Null,
            "positionSnapshotComplete": true, "marketIds": markets, "positions": positions,
            "realizedPnl": Value::Null, "realizedFunding": Value::Null, "fees": Value::Null,
            "freeBalance": Value::Null});
        let mut sdk_wallet = wallet.clone();
        for p in sdk_wallet
            .positions
            .iter_mut()
            .filter(|p| p.status == "open" && markets.contains(&p.perpetual_id))
        {
            let spec = registry.market(p.perpetual_id).map_err(|e| e.to_string())?;
            p.entry = sdk_entry_projection(
                p.stored_entry_pns,
                p.entry_residue_pnsq16,
                if p.side == "long" {
                    SIDE_LONG
                } else {
                    SIDE_SHORT
                },
                spec.price_decimals,
            )?;
        }
        let mut scorecard = evidence::reconcile_positions(&sdk_wallet, &as_of, &reference)
            .map_err(|e| e.to_string())?;
        scorecard["version"] = json!("position-reconciliation-v2-sdk-width");
        for check in scorecard["checks"]
            .as_array_mut()
            .ok_or("Invalid position scorecard")?
        {
            if check["field"] == "entryPrice"
                && let Some(p) = wallet.positions.iter().find(|p| {
                    Some(u64::from(p.perpetual_id)) == check["perpetualId"].as_u64()
                        && p.status == "open"
                })
            {
                check["canonicalSdkProjection"] = check["canonical"].clone();
                check["canonical"] = json!(p.entry.to_string());
                check["canonicalStoredEntryPns"] = json!(p.stored_entry_pns.to_string());
                check["canonicalEntryResiduePnsQ16"] = json!(p.entry_residue_pnsq16);
                check["comparisonRepresentation"] = json!(
                    "Pinned SDK UD64 Floor projection from canonical native entry/residue; exact ledger entry is retained"
                );
            }
        }
        scorecards.push(scorecard);
        let mut manifest = evidence::manifest(
            &combined,
            &as_of,
            &slice.coverage.evidence,
            "envio-mainnet-archive",
        )
        .map_err(|e| e.to_string())?;
        manifest["accountId"] = json!(id);
        manifest["registryHash"] = json!(evidence::digest(&registry).map_err(|e| e.to_string())?);
        manifest["marketMarksHash"] =
            json!(evidence::digest(&ledger.market_marks).map_err(|e| e.to_string())?);
        if config.funding_checkpoints {
            manifest["fundingCheckpointsHash"] = json!(
                evidence::digest(
                    &wallet
                        .positions
                        .iter()
                        .map(|p| (p.perpetual_id, &p.funding_checkpoint))
                        .collect::<Vec<_>>()
                )
                .map_err(|e| e.to_string())?
            );
        }
        manifest["observedSourceBlock"] = json!(slice.coverage.source_block);
        manifest["observedIsReady"] = json!(slice.coverage.is_ready);
        manifest["reconciliation"] = json!({"status": "partially-verified",
            "role": "external verifier", "positionsStatus": scorecards.last().unwrap()["status"],
            "accountTotalsStatus": "unverified",
            "referenceHash": evidence::digest(&reference).map_err(|e| e.to_string())?,
            "scope": "Requested position state; enabled diagnostics separately compare marks, price PnL and eligible funding checkpoints. Unknown funding is explicitly unverified; lifetime totals and balances remain unverified."});
        manifests.push(manifest);
        references.push(reference);
    }
    if header(&provider, config.block).await? != as_of {
        return Err("Pinned block header changed during reference acquisition".into());
    }
    let matched = scorecards
        .iter()
        .chain(&risk_scorecards)
        .chain(&market_scorecards)
        .all(|v| v["status"] == "matched");
    let matched = matched && funding_scorecards.iter().all(|v| v["status"] != "mismatch");
    Ok(
        json!({"version": "sdk-reference-execution-v1", "sdkCommit": SDK_COMMIT,
        "status": if matched {"matched"} else {"mismatch"}, "mode": "historical-end-of-block",
        "chainId": 143, "asOfBlock": as_of.block_number, "asOfBlockHash": as_of.block_hash,
        "asOfLogIndex": Value::Null, "asOfTimestampMs": as_of.timestamp_ms,
        "canonicalManifests": manifests, "references": references, "scorecards": scorecards,
        "referenceHash": evidence::digest(&references).map_err(|e| e.to_string())?,
        "marketObservations": metadata,
        "riskReferences": risk_references,
        "riskScorecards": risk_scorecards,
        "fundingScorecards": funding_scorecards,
        "marketScorecards": market_scorecards, "fundingTimelines": funding_timelines,
        "marketInputsHash": if market_inputs.is_empty() {Value::Null} else {json!(evidence::digest(&market_inputs).map_err(|e|e.to_string())?)},
        "marketInputEventCount": market_inputs.len(),
        "limitations": ["SDK calls pin the block number; its header hash is checked before and after acquisition, not EIP-1898 on every call.",
            "Completeness is restricted to the selected markets and covered account histories.",
            "Lifetime totals, balances and live freshness are not verified. Funding and funded risk are verified only for nonempty matched funding scorecards; unverified checkpoints remain unknown."]}),
    )
}

#[tokio::main]
async fn main() {
    let result: Result<Value, String> = async {
        let mut input = Vec::new();
        std::io::stdin()
            .take(65_537)
            .read_to_end(&mut input)
            .map_err(|_| "Cannot read reference configuration")?;
        if input.len() > 65_536 {
            return Err("Reference configuration exceeds size limit".into());
        }
        let config =
            serde_json::from_slice(&input).map_err(|_| "Invalid reference configuration")?;
        run(config).await
    }
    .await;
    match result {
        Ok(value) => {
            let matched = value["status"] == "matched";
            if serde_json::to_writer(std::io::stdout().lock(), &value).is_err()
                || std::io::stdout().write_all(b"\n").is_err()
            {
                std::process::exit(1);
            }
            if !matched {
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("Reference verification failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equity_comparison_preserves_native_pnl_component_rounding_before_addition() {
        let deposit = number("40.002876").unwrap();
        let price_pnl = number("-0.08951400421142578125").unwrap();
        let projected = funded_projection(
            price_pnl,
            Decimal::ZERO,
            deposit,
            number("16.001150159831542967808").unwrap(),
            6,
        )
        .unwrap();
        let check = risk_check(
            "equity",
            projected[2],
            number("39.913362").unwrap(),
            Some(6),
        );
        assert_eq!(check["status"], "matched");
        assert_eq!(
            deposit + price_pnl,
            number("39.91336199578857421875").unwrap()
        );
        assert_eq!(projected[3], number("23.912211840168457032192").unwrap());
        assert_eq!(
            funded_projection(
                number("-0.0000006").unwrap(),
                number("-0.0000006").unwrap(),
                number("1").unwrap(),
                Decimal::ZERO,
                6
            )
            .unwrap()[1],
            Decimal::ZERO
        );
    }

    #[test]
    fn sdk_width_entry_projection_matches_native_q16_flooring() {
        assert_eq!(
            sdk_entry_projection(279308, 9405, SIDE_LONG, 4).unwrap(),
            number("27.93071435089111328").unwrap()
        );
        assert_eq!(
            sdk_entry_projection(849317, 62196, SIDE_SHORT, 1).unwrap(),
            number("84931.79490356445312").unwrap()
        );
        assert_ne!(
            sdk_entry_projection(279308, 9405, SIDE_LONG, 4).unwrap(),
            sdk_entry_projection(279308, 9406, SIDE_LONG, 4).unwrap()
        );
        assert!(sdk_entry_projection(279308, 65536, SIDE_LONG, 4).is_err());
        assert!(sdk_entry_projection(279308, 9405, 0, 4).is_err());
    }

    #[test]
    fn sdk_width_maintenance_preserves_the_upstream_entry_rounding() {
        assert_eq!(
            sdk_maintenance_projection(
                279308,
                9405,
                SIDE_LONG,
                4,
                number("6.47").unwrap(),
                number(10).unwrap()
            )
            .unwrap(),
            number("18.07117218502655029216").unwrap()
        );
    }

    #[test]
    fn comparison_contract_preserves_raw_values_and_rejects_native_unit_differences() {
        let a = number("17.156464214599609375").unwrap();
        let b = number("17.156464").unwrap();
        let check = risk_check("price", a, b, Some(6));
        assert_eq!(check["status"], "matched");
        assert_eq!(check["formulaValue"], "17.156464214599609375");
        assert_eq!(risk_check("price", a, b, None)["status"], "mismatch");
        assert_eq!(
            risk_check("price", a, number("17.156465").unwrap(), Some(6))["status"],
            "mismatch"
        );
        assert_eq!(risk_check("price", -a, -b, Some(6))["status"], "matched");
    }

    #[test]
    fn liquidation_compares_market_ticks_without_hiding_a_tick_difference() {
        let formula = number("82913.83199757252093700691832").unwrap();
        assert_eq!(
            risk_check(
                "liq",
                formula,
                number("82913.83199757252093").unwrap(),
                Some(1)
            )["status"],
            "matched"
        );
        assert_eq!(
            risk_check(
                "liq",
                formula,
                number("82913.93199757252093").unwrap(),
                Some(1)
            )["status"],
            "mismatch"
        );
    }
}
