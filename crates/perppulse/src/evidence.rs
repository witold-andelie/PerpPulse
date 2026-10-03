use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::coverage::CoverageEvidence;
use crate::error::{DataQualityError, Result};
use crate::events::CanonicalEvent;
use crate::identity::AsOf;

pub const METHODOLOGY: &str = include_str!("../../../docs/methodology.json");

pub fn digest(value: &impl serde::Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| DataQualityError::msg("cannot serialize evidence for hashing"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

/// Hash ordered canonical inputs without exporting raw provider payloads.
pub fn manifest(
    events: &[CanonicalEvent],
    as_of: &AsOf,
    coverage: &CoverageEvidence,
    source: &str,
) -> Result<Value> {
    let mut ordered: Vec<_> = events
        .iter()
        .filter(|event| as_of.includes(event.block_number, event.log_index))
        .collect();
    ordered.sort_by_key(|event| (event.block_number, event.log_index, event.tx_hash.clone()));
    let ids = ordered
        .iter()
        .map(|event| event.event_id().map(|id| id.key()))
        .collect::<Result<Vec<_>>>()?;
    let versions: std::collections::BTreeSet<_> = ordered
        .iter()
        .filter_map(|event| event.provenance.as_ref())
        .map(|p| {
            (
                p.schema_version.clone(),
                p.handler_version.clone(),
                p.classifier_version.clone(),
                p.ingestion_profile.clone(),
                p.abi_fingerprint.clone(),
            )
        })
        .collect();
    let methodology: Value = serde_json::from_str(METHODOLOGY)
        .map_err(|_| DataQualityError::msg("invalid embedded methodology"))?;
    Ok(json!({
        "version": "range-manifest-v1", "source": source, "chainId": as_of.chain_id,
        "startBlock": coverage.start_block, "processedBlock": coverage.processed_block,
        "asOfBlock": as_of.block_number, "asOfBlockHash": as_of.block_hash,
        "asOfLogIndex": as_of.log_index, "asOfTimestampMs": as_of.timestamp_ms,
        "eventCount": ordered.len(), "firstEventId": ids.first(), "lastEventId": ids.last(),
        "orderedEventIdsHash": digest(&ids)?, "canonicalInputsHash": digest(&ordered)?,
        "methodologyHash": digest(&methodology)?, "provenanceTuples": versions,
        "reconciliation": {"status": "unverified", "role": "external verifier", "reason": "No same-cutoff Perpl reference was supplied."},
        "limitations": ["Coverage is processed-chain evidence, not an independent reorg or completeness certificate.", "Fixture inputs are synthetic and are never evidence of live mainnet operation."]
    }))
}

/// Compare only supported facts at the exact same cutoff; references never
/// enter the canonical ledger. Missing fields remain explicitly unverified.
pub fn reconcile(snapshot: &crate::serve::ApiSnapshot, reference: &Value) -> Result<Value> {
    if reference.get("source").and_then(Value::as_str) != Some("perpl-dex-sdk")
        || reference.get("chainId").and_then(Value::as_u64) != Some(snapshot.chain_id)
        || reference.get("asOfBlock").and_then(Value::as_u64) != Some(snapshot.as_of_block)
        || reference.get("asOfTimestampMs").and_then(Value::as_i64)
            != Some(snapshot.as_of_timestamp_ms)
        || reference.get("asOfBlockHash") != snapshot.manifest.get("asOfBlockHash")
        || reference.get("asOfLogIndex") != snapshot.manifest.get("asOfLogIndex")
    {
        return Err(DataQualityError::msg("reference must be from perpl-dex-sdk with the same chain, block hash, log cutoff, and timestamp"));
    }
    let account = reference
        .get("accountId")
        .and_then(Value::as_u64)
        .ok_or_else(|| DataQualityError::msg("reference accountId is required"))?;
    let wallet = snapshot
        .wallets
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["accountId"].as_u64() == Some(account))
        })
        .ok_or_else(|| {
            DataQualityError::msg("reference account is absent from canonical snapshot")
        })?;
    let mut checks = Vec::new();
    for field in ["realizedPnl", "realizedFunding", "fees", "freeBalance"] {
        let actual = &wallet[field];
        let expected = &reference[field];
        let status = if actual.is_null() || expected.is_null() {
            "unverified"
        } else {
            let a = actual
                .as_str()
                .and_then(|s| s.parse::<rust_decimal::Decimal>().ok());
            let b = expected
                .as_str()
                .and_then(|s| s.parse::<rust_decimal::Decimal>().ok());
            if a.is_none() || b.is_none() {
                return Err(DataQualityError::msg(
                    "reconciliation amounts must be finite decimal strings",
                ));
            }
            if a == b {
                "matched"
            } else {
                "mismatch"
            }
        };
        checks.push(
            json!({"field": field, "status": status, "canonical": actual, "reference": expected}),
        );
    }
    let status = if checks.iter().any(|c| c["status"] == "mismatch") {
        "mismatch"
    } else if checks.iter().any(|c| c["status"] == "unverified") {
        "unverified"
    } else {
        "matched"
    };
    Ok(
        json!({"version": "reconciliation-scorecard-v1", "status": status, "accountId": account,
        "asOfBlock": snapshot.as_of_block, "canonicalRole": "Envio lifecycle ledger", "referenceRole": "Perpl verifier",
        "checks": checks, "scope": "Account totals only; open-position state, unrealized funding, and executable liquidity are not verified by this scorecard."}),
    )
}

/// Position-only verification against a complete SDK snapshot of explicitly
/// selected markets. Account lifetime totals are outside this contract.
pub fn reconcile_positions(
    wallet: &crate::accounting::WalletSnapshot,
    as_of: &AsOf,
    reference: &Value,
) -> Result<Value> {
    use rust_decimal::Decimal;
    use std::collections::{BTreeMap, BTreeSet};

    if as_of.log_index.is_some()
        || reference["source"] != "perpl-dex-sdk"
        || reference["positionSnapshotComplete"] != true
        || reference["chainId"].as_u64() != Some(as_of.chain_id)
        || reference["accountId"].as_u64() != Some(wallet.account_id)
        || reference["asOfBlock"].as_u64() != Some(as_of.block_number)
        || reference["asOfBlockHash"].as_str() != Some(&as_of.block_hash)
        || reference.get("asOfLogIndex") != Some(&Value::Null)
        || reference["asOfTimestampMs"].as_i64() != Some(as_of.timestamp_ms)
    {
        return Err(DataQualityError::msg(
            "position reference requires a complete SDK snapshot at the identical end-of-block header",
        ));
    }
    let ids = reference["marketIds"]
        .as_array()
        .ok_or_else(|| DataQualityError::msg("position reference requires explicit marketIds"))?;
    let mut scope = BTreeSet::new();
    for id in ids {
        let id = id
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0)
            .ok_or_else(|| DataQualityError::msg("invalid reference market ID"))?;
        if !scope.insert(id) {
            return Err(DataQualityError::msg("duplicate reference market ID"));
        }
    }
    if scope.is_empty() || scope.len() > 20 {
        return Err(DataQualityError::msg(
            "reference market scope must contain 1 to 20 IDs",
        ));
    }
    let rows = reference["positions"]
        .as_array()
        .ok_or_else(|| DataQualityError::msg("reference positions are missing"))?;
    let mut positions = BTreeMap::new();
    for row in rows {
        let id = row["perpetualId"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| DataQualityError::msg("invalid reference position market"))?;
        if !scope.contains(&id) || positions.insert(id, row).is_some() {
            return Err(DataQualityError::msg(
                "duplicate or out-of-scope reference position",
            ));
        }
    }
    if positions.len() != scope.len() {
        return Err(DataQualityError::msg(
            "every requested market requires explicit open or closed reference state",
        ));
    }
    let canonical: BTreeMap<_, _> = wallet
        .positions
        .iter()
        .map(|p| (p.perpetual_id, p))
        .collect();
    if canonical.len() != wallet.positions.len() {
        return Err(DataQualityError::msg("duplicate canonical position"));
    }
    let mut checks = Vec::new();
    for id in &scope {
        let row = positions[id];
        let status = row["status"]
            .as_str()
            .filter(|s| matches!(*s, "open" | "closed"))
            .ok_or_else(|| DataQualityError::msg("invalid reference position status"))?;
        let mut amounts = BTreeMap::new();
        for field in ["size", "deposit"] {
            let amount = row[field]
                .as_str()
                .and_then(|s| Decimal::from_str_exact(s).ok())
                .filter(|n| *n >= Decimal::ZERO)
                .ok_or_else(|| {
                    DataQualityError::msg(
                        "reference size/deposit must be exact nonnegative decimals",
                    )
                })?;
            amounts.insert(field, amount);
        }
        let side = row["side"].as_str();
        let entry = row["entryPrice"]
            .as_str()
            .and_then(|s| Decimal::from_str_exact(s).ok());
        if (status == "closed"
            && (amounts["size"] != Decimal::ZERO
                || amounts["deposit"] != Decimal::ZERO
                || row.get("side") != Some(&Value::Null)
                || row.get("entryPrice") != Some(&Value::Null)))
            || (status == "open"
                && (amounts["size"] <= Decimal::ZERO
                    || !matches!(side, Some("long" | "short"))
                    || entry.is_none_or(|n| n <= Decimal::ZERO)))
        {
            return Err(DataQualityError::msg(
                "inconsistent reference position state",
            ));
        }
        let position = canonical.get(id);
        let canonical_status = position.map_or("closed", |p| p.status.as_str());
        let mut check = |field: &str, actual: Value, expected: Value, matched: bool| {
            checks.push(json!({"perpetualId": id, "field": field,
                "canonical": actual, "reference": expected,
                "status": if matched {"matched"} else {"mismatch"}}));
        };
        check(
            "status",
            json!(canonical_status),
            json!(status),
            canonical_status == status,
        );
        for field in ["size", "deposit"] {
            let actual = position.map_or(Decimal::ZERO, |p| {
                if field == "size" {
                    p.size
                } else {
                    p.deposit
                }
            });
            check(
                field,
                json!(actual.normalize().to_string()),
                row[field].clone(),
                actual == amounts[field],
            );
        }
        if status == "open" || canonical_status == "open" {
            let actual_side = position
                .filter(|p| p.status == "open")
                .map(|p| p.side.as_str());
            let actual_entry = position.filter(|p| p.status == "open").map(|p| p.entry);
            check(
                "side",
                json!(actual_side),
                row["side"].clone(),
                actual_side == side,
            );
            check(
                "entryPrice",
                json!(actual_entry.map(|n| n.normalize().to_string())),
                row["entryPrice"].clone(),
                actual_entry == entry,
            );
        }
    }
    let excluded: Vec<_> = canonical.keys().filter(|id| !scope.contains(id)).collect();
    let matched = checks.iter().all(|c| c["status"] == "matched");
    Ok(json!({"version": "position-reconciliation-v1",
        "status": if matched {"matched"} else {"mismatch"},
        "accountId": wallet.account_id, "chainId": as_of.chain_id,
        "asOfBlock": as_of.block_number, "asOfBlockHash": as_of.block_hash,
        "asOfLogIndex": Value::Null, "asOfTimestampMs": as_of.timestamp_ms,
        "marketIds": scope, "excludedCanonicalMarketIds": excluded, "checks": checks,
        "scope": "Selected position status, size, deposit, side and effective entry only. Lifetime PnL, funding, fees, balances, marks and risk are unverified."}))
}
