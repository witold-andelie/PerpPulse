use perppulse::{
    accounting::account_wallet, events::repo_root, evidence::reconcile_positions,
    pipeline::run_fixture,
};
use serde_json::{json, Value};

fn input() -> (
    perppulse::accounting::WalletSnapshot,
    perppulse::AsOf,
    Value,
) {
    let pulse = run_fixture(
        repo_root().join("fixtures/golden/open-position-as-of.json"),
        Some(0),
    )
    .unwrap();
    let wallet = account_wallet(&pulse.ledger, 42, &pulse.fixture.as_of, &[], false).unwrap();
    let cutoff = pulse.fixture.as_of;
    let reference = json!({"source":"perpl-dex-sdk", "positionSnapshotComplete":true,
        "chainId":143, "accountId":42, "asOfBlock":cutoff.block_number,
        "asOfBlockHash":cutoff.block_hash, "asOfTimestampMs":cutoff.timestamp_ms,
        "asOfLogIndex":Value::Null, "marketIds":[1,10], "positions":[
            {"perpetualId":1,"status":"open","size":"1","deposit":"10000",
                "side":"long","entryPrice":"70000"},
            {"perpetualId":10,"status":"closed","size":"0","deposit":"0",
                "side":Value::Null,"entryPrice":Value::Null}]});
    (wallet, cutoff, reference)
}

#[test]
fn same_cutoff_checks_open_state_and_absence_only_inside_explicit_scope() {
    let (wallet, cutoff, mut reference) = input();
    let score = reconcile_positions(&wallet, &cutoff, &reference).unwrap();
    assert_eq!(score["status"], "matched");
    assert_eq!(score["checks"].as_array().unwrap().len(), 8);
    reference["positions"][0]["entryPrice"] = json!("70000.0000000000000001");
    let score = reconcile_positions(&wallet, &cutoff, &reference).unwrap();
    assert_eq!(score["status"], "mismatch");
    reference["positions"][0]["entryPrice"] = json!("70000");
    reference["positions"][1] = json!({"perpetualId":10,"status":"open","size":"2",
        "deposit":"10","side":"short","entryPrice":"0.03"});
    assert_eq!(
        reconcile_positions(&wallet, &cutoff, &reference).unwrap()["status"],
        "mismatch"
    );
}

#[test]
fn different_header_partial_cutoff_and_incomplete_market_snapshots_are_rejected() {
    let (wallet, cutoff, reference) = input();
    for field in [
        "chainId",
        "accountId",
        "asOfBlock",
        "asOfBlockHash",
        "asOfTimestampMs",
        "asOfLogIndex",
        "source",
        "positionSnapshotComplete",
        "marketIds",
        "positions",
    ] {
        let mut invalid = reference.clone();
        invalid[field] = if field == "asOfLogIndex" {
            json!(0)
        } else {
            Value::Null
        };
        assert!(
            reconcile_positions(&wallet, &cutoff, &invalid).is_err(),
            "accepted {field}"
        );
    }
    let mut partial = cutoff.clone();
    partial.log_index = Some(0);
    assert!(reconcile_positions(&wallet, &partial, &reference).is_err());
    let mut invalid = reference.clone();
    invalid["positions"].as_array_mut().unwrap().pop();
    assert!(reconcile_positions(&wallet, &cutoff, &invalid).is_err());
    let mut invalid = reference.clone();
    invalid["positions"][1]["perpetualId"] = json!(1);
    assert!(reconcile_positions(&wallet, &cutoff, &invalid).is_err());
    let mut invalid = reference;
    invalid["marketIds"] = json!([]);
    invalid["positions"] = json!([]);
    assert!(reconcile_positions(&wallet, &cutoff, &invalid).is_err());
}

#[test]
fn malformed_nonfinite_overprecision_and_inconsistent_amounts_are_rejected() {
    let (wallet, cutoff, reference) = input();
    for value in ["NaN", "Infinity", "-1", "0.00000000000000000000000000001"] {
        let mut invalid = reference.clone();
        invalid["positions"][0]["size"] = json!(value);
        assert!(
            reconcile_positions(&wallet, &cutoff, &invalid).is_err(),
            "accepted {value}"
        );
    }
    let mut invalid = reference.clone();
    invalid["positions"][1]["deposit"] = json!("1");
    assert!(reconcile_positions(&wallet, &cutoff, &invalid).is_err());
    let mut invalid = reference;
    invalid["positions"][0]["entryPrice"] = json!("NaN");
    assert!(reconcile_positions(&wallet, &cutoff, &invalid).is_err());
}
