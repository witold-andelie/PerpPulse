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
use perpl_sdk::{
    Chain,
    state::{PositionType, SnapshotBuilder},
    types::AccountAddressOrID,
};
use perppulse::{
    AsOf, accounting::account_wallet, envio::EnvioClient, evidence, load_registry, replay,
};
use rust_decimal::Decimal;
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
}

fn decimal(value: impl std::fmt::Display) -> Result<String, String> {
    Decimal::from_str_exact(&value.to_string())
        .map(|n| n.normalize().to_string())
        .map_err(|_| "SDK numeric value cannot be represented exactly".into())
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
        {
            return Err("SDK market decimals differ from the canonical registry".into());
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
        let ledger = replay(&slice.events, &registry, &as_of).map_err(|e| e.to_string())?;
        let wallet = account_wallet(&ledger, u64::from(*id), &as_of, &[], false)
            .map_err(|e| e.to_string())?;
        let account = snapshot
            .accounts()
            .get(id)
            .ok_or("SDK account is missing")?;
        let mut positions = Vec::new();
        for market_id in &markets {
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
        scorecards.push(
            evidence::reconcile_positions(&wallet, &as_of, &reference)
                .map_err(|e| e.to_string())?,
        );
        let mut manifest = evidence::manifest(
            &slice.events,
            &as_of,
            &slice.coverage.evidence,
            "envio-mainnet-archive",
        )
        .map_err(|e| e.to_string())?;
        manifest["accountId"] = json!(id);
        manifest["registryHash"] = json!(evidence::digest(&registry).map_err(|e| e.to_string())?);
        manifest["observedSourceBlock"] = json!(slice.coverage.source_block);
        manifest["observedIsReady"] = json!(slice.coverage.is_ready);
        manifest["reconciliation"] = json!({"status": "partially-verified",
            "role": "external verifier", "positionsStatus": scorecards.last().unwrap()["status"],
            "accountTotalsStatus": "unverified",
            "referenceHash": evidence::digest(&reference).map_err(|e| e.to_string())?,
            "scope": "Requested position state only; lifetime totals, balances and mark-derived facts remain unverified."});
        manifests.push(manifest);
        references.push(reference);
    }
    if header(&provider, config.block).await? != as_of {
        return Err("Pinned block header changed during reference acquisition".into());
    }
    let matched = scorecards.iter().all(|v| v["status"] == "matched");
    Ok(
        json!({"version": "sdk-reference-execution-v1", "sdkCommit": SDK_COMMIT,
        "status": if matched {"matched"} else {"mismatch"}, "mode": "historical-end-of-block",
        "chainId": 143, "asOfBlock": as_of.block_number, "asOfBlockHash": as_of.block_hash,
        "asOfLogIndex": Value::Null, "asOfTimestampMs": as_of.timestamp_ms,
        "canonicalManifests": manifests, "references": references, "scorecards": scorecards,
        "referenceHash": evidence::digest(&references).map_err(|e| e.to_string())?,
        "marketObservations": metadata,
        "limitations": ["SDK calls pin the block number; its header hash is checked before and after acquisition, not EIP-1898 on every call.",
            "Completeness is restricted to the selected markets and covered account histories.",
            "Lifetime totals, balances, mark-derived accounting and live freshness are not verified."]}),
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
