//! Public metadata observation, separate from eligible accounting inputs.
//! Context timestamps describe composite state, not a verified mark-update
//! cutoff. This adapter deliberately does not produce `MarketMark` values.

use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::error::{DataQualityError, Result};
use crate::evidence::digest;
use crate::registry::{MarketSpec, ProtocolRegistry};

pub const CONTEXT_ENDPOINT: &str = "https://app.perpl.xyz/api/v1/pub/context";

pub fn fetch_public_context(registry: &ProtocolRegistry) -> Result<Value> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let response: Value = agent
        .get(CONTEXT_ENDPOINT)
        .call()
        .map_err(|_| DataQualityError::msg("Perpl public context request failed"))?
        .body_mut()
        .with_config()
        .limit(1_048_576)
        .read_json()
        .map_err(|_| DataQualityError::msg("invalid or oversized Perpl public context"))?;
    let observed = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DataQualityError::msg("market observation clock failed"))?
            .as_millis(),
    )
    .map_err(|_| DataQualityError::msg("market observation time overflow"))?;
    inspect_public_context(&response, registry, observed)
}

fn uint(row: &Value, key: &str) -> Result<u64> {
    row[key]
        .as_u64()
        .ok_or_else(|| DataQualityError::msg(format!("context {key} must be an unsigned integer")))
}

fn u32_field(row: &Value, key: &str) -> Result<u32> {
    u32::try_from(uint(row, key)?)
        .map_err(|_| DataQualityError::msg(format!("context {key} exceeds u32")))
}

fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str> {
    row[key]
        .as_str()
        .filter(|s| {
            !s.is_empty() && s.len() <= 256 && s.is_ascii() && !s.chars().any(char::is_control)
        })
        .ok_or_else(|| DataQualityError::msg(format!("context {key} is missing or invalid")))
}

fn rows<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value[key]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 100)
        .map(Vec::as_slice)
        .ok_or_else(|| DataQualityError::msg(format!("context {key} requires 1..100 rows")))
}

fn timestamp(row: &Value, key: &str, observed_ms: i64) -> Result<(u64, i64)> {
    let block = uint(&row[key], "b")?;
    let time = i64::try_from(uint(&row[key], "t")?)
        .map_err(|_| DataQualityError::msg("context timestamp exceeds i64"))?;
    if block == 0 || time <= 0 || time > observed_ms {
        return Err(DataQualityError::msg(
            "context timestamp is missing, nonpositive, or in the future",
        ));
    }
    Ok((block, time))
}

/// Export only selected public protocol fields. No user, location, rewards,
/// authentication, or raw provider payload is returned or persisted.
pub fn inspect_public_context(
    value: &Value,
    registry: &ProtocolRegistry,
    observed_ms: i64,
) -> Result<Value> {
    registry.validate()?;
    if observed_ms <= 0 || uint(&value["chain"], "chain_id")? != registry.chain_id {
        return Err(DataQualityError::msg(
            "context observation time or chain identity is invalid",
        ));
    }
    let instances = rows(value, "instances")?;
    let mut instance_ids = BTreeSet::new();
    for instance in instances {
        if !instance_ids.insert(uint(instance, "id")?) {
            return Err(DataQualityError::msg("duplicate context instance id"));
        }
    }
    let matches: Vec<_> = instances
        .iter()
        .filter(|row| {
            row["address"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&registry.exchange_address))
        })
        .collect();
    if matches.len() != 1 {
        return Err(DataQualityError::msg(
            "context must contain exactly one expected Exchange instance",
        ));
    }
    let instance = matches[0];
    let instance_id = uint(instance, "id")?;
    let token_id = uint(instance, "collateral_token_id")?;
    let tokens = rows(value, "tokens")?;
    let mut token_ids = BTreeSet::new();
    for token in tokens {
        if !token_ids.insert(uint(token, "id")?) {
            return Err(DataQualityError::msg("duplicate context token id"));
        }
    }
    let token = tokens
        .iter()
        .find(|row| row["id"].as_u64() == Some(token_id))
        .ok_or_else(|| DataQualityError::msg("context collateral token is missing"))?;
    if !text(token, "address")?.eq_ignore_ascii_case(&registry.collateral.address)
        || text(token, "symbol")? != registry.collateral.symbol
        || uint(token, "decimals")? != u64::from(registry.collateral.decimals)
    {
        return Err(DataQualityError::msg(
            "context collateral identity or scale differs from registry",
        ));
    }
    let mut markets = Vec::new();
    let mut changes = Vec::new();
    let mut state_quality = Vec::new();
    let mut ids = BTreeSet::new();
    for row in rows(value, "markets")? {
        let row_instance = uint(row, "instance_id")?;
        if !instance_ids.contains(&row_instance) {
            return Err(DataQualityError::msg(
                "context market references an unknown instance",
            ));
        }
        if row_instance != instance_id {
            continue;
        }
        let id = u32_field(row, "perpetual_id")?;
        if !ids.insert(id) {
            return Err(DataQualityError::msg("duplicate context perpetual id"));
        }
        let config = &row["config"];
        let (config_block, config_ms) = timestamp(config, "at", observed_ms)?;
        let is_open = config["is_open"]
            .as_bool()
            .ok_or_else(|| DataQualityError::msg("context market open status is missing"))?;
        let mut symbol = row["symbol"]
            .as_str()
            .ok_or_else(|| DataQualityError::msg("context symbol is missing"))?;
        // The official context currently returns empty symbols for BTC and MON
        // with their ticker in name. Require an unambiguous ticker-only fallback.
        if symbol.is_empty() {
            symbol = text(row, "name")?;
            if !symbol
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                return Err(DataQualityError::msg(
                    "context empty symbol has no unambiguous ticker fallback",
                ));
            }
        }
        let market = MarketSpec {
            perpetual_id: id,
            symbol: symbol.into(),
            price_decimals: u32_field(config, "price_decimals")?,
            size_decimals: u32_field(config, "size_decimals")?,
            init_margin_frac_hdths: u32_field(config, "initial_margin")?,
            maint_margin_frac_hdths: u32_field(config, "maintenance_margin")?,
            listed: !registry.excluded_perpetuals.contains(&id),
            notes: String::new(),
        };
        market.validate()?;
        let version = config["contract_version"]
            .as_array()
            .filter(|v| v.len() == 3)
            .ok_or_else(|| {
                DataQualityError::msg("context contract version must have three components")
            })?;
        for component in version {
            if component.as_u64().is_none_or(|v| v > u64::from(u32::MAX)) {
                return Err(DataQualityError::msg(
                    "context contract version component is invalid",
                ));
            }
        }
        let (state_block, state_ms) = timestamp(&row["state"], "at", observed_ms)?;
        if config_block > state_block || config_ms > state_ms {
            return Err(DataQualityError::msg(
                "context market state predates its configuration",
            ));
        }
        let mark = uint(&row["state"], "mrk")?;
        if mark == 0 {
            return Err(DataQualityError::msg("context mark price must be positive"));
        }
        match registry.markets.get(&id) {
            None => changes.push(json!({"perpetualId":id,"kind":"new-market"})),
            Some(old)
                if old.price_decimals != market.price_decimals
                    || old.size_decimals != market.size_decimals
                    || old.init_margin_frac_hdths != market.init_margin_frac_hdths
                    || old.maint_margin_frac_hdths != market.maint_margin_frac_hdths
                    || old.symbol != market.symbol =>
            {
                changes.push(json!({"perpetualId":id,"kind":"metadata-changed"}))
            }
            _ => {}
        }
        state_quality.push(json!({"perpetualId":id,"stateAgeMs":observed_ms-state_ms,
            "status":if observed_ms-state_ms >= crate::accounting::MAX_MARK_AGE_MS {"stale-state"} else {"metadata-observation"}}));
        markets.push(json!({"market":market,"configBlock":config_block,"configTimestampMs":config_ms,
            "contractVersion":version,"tradingOpen":is_open,"stateBlock":state_block,"stateTimestampMs":state_ms,
            "observedMarkPns":mark.to_string(),"markAccountingEligible":false,
            "reason":"Composite REST state has no verified mark-update time, block hash, or log cutoff."}));
    }
    if markets.is_empty() {
        return Err(DataQualityError::msg(
            "context contains no markets for the expected Exchange",
        ));
    }
    for market in registry.markets.values().filter(|m| m.listed) {
        if !ids.contains(&market.perpetual_id) {
            changes.push(json!({"perpetualId":market.perpetual_id,"kind":"absent-from-context"}));
        }
    }
    markets.sort_by_key(|row| row["market"]["perpetual_id"].as_u64());
    changes.sort_by_key(|row| row["perpetualId"].as_u64());
    state_quality.sort_by_key(|row| row["perpetualId"].as_u64());
    let fields = json!({"chainId":registry.chain_id,"exchangeAddress":registry.exchange_address,
        "collateral":registry.collateral,"markets":markets});
    Ok(
        json!({"version":"perpl-context-observation-v1","source":CONTEXT_ENDPOINT,
        "observedAtMs":observed_ms,"fieldsHash":digest(&fields)?,"fields":fields,"registryDifferences":changes,
        "stateQuality":state_quality,
        "accountingEligible":false,"canonicalRegistryUpdated":false,
        "limitations":["Metadata requires independent onchain verification at the ledger cutoff before adoption.",
            "REST state observation time does not prove the freshness or cutoff of the underlying mark.",
            "REST protocol totals and fee schedules are not imported into canonical accounting."]}),
    )
}
