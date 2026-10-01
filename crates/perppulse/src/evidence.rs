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
