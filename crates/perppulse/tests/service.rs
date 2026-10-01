use perppulse::events::repo_root;
use perppulse::evidence::{manifest, reconcile};
use perppulse::pipeline::run_fixture;
use perppulse::serve::{build_snapshot, route};
use serde_json::json;

#[test]
fn event_ranges_reject_lookahead_unsupported_filters_and_duplicate_keys() {
    let snapshot = build_snapshot(
        &run_fixture(
            repo_root().join("fixtures/golden/open-position-as-of.json"),
            Some(0),
        )
        .unwrap(),
    )
    .unwrap();
    for query in [
        "fromBlock=0",
        "toBlock=999999999",
        "limit=0",
        "limit=1001",
        "limit=2&limit=3",
        "unknown=1",
        "offset=-1",
    ] {
        assert_eq!(
            route(&snapshot, "GET", &format!("/api/events?{query}")).0,
            400,
            "{query}"
        );
    }
    assert_eq!(route(&snapshot, "GET", "/api/events?asOfBlock=1").0, 409);
    let rows = route(&snapshot, "GET", "/api/events?accountId=42&limit=1").1;
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["accountId"], 42);
    assert_eq!(
        route(&snapshot, "GET", "/api/protocol?fromBlock=54773020").0,
        400
    );
}

#[test]
fn range_manifest_is_deterministic_sensitive_to_facts_and_excludes_future_events() {
    let pulse = run_fixture(
        repo_root().join("fixtures/golden/open-position-as-of.json"),
        Some(0),
    )
    .unwrap();
    let a = manifest(
        &pulse.ledger.events,
        &pulse.fixture.as_of,
        &pulse.fixture.coverage,
        "fixture",
    )
    .unwrap();
    let mut reverse = pulse.ledger.events.clone();
    reverse.reverse();
    let b = manifest(
        &reverse,
        &pulse.fixture.as_of,
        &pulse.fixture.coverage,
        "fixture",
    )
    .unwrap();
    assert_eq!(a, b);
    reverse[0].fee_cns = Some(99);
    let c = manifest(
        &reverse,
        &pulse.fixture.as_of,
        &pulse.fixture.coverage,
        "fixture",
    )
    .unwrap();
    assert_ne!(a["canonicalInputsHash"], c["canonicalInputsHash"]);
    let mut future = pulse.ledger.events[0].clone();
    future.block_number = pulse.fixture.as_of.block_number + 1;
    reverse = pulse.ledger.events.clone();
    reverse.push(future);
    assert_eq!(
        a,
        manifest(
            &reverse,
            &pulse.fixture.as_of,
            &pulse.fixture.coverage,
            "fixture"
        )
        .unwrap()
    );
}

#[test]
fn reconciliation_requires_exact_cutoff_and_reports_missing_or_mismatched_fields() {
    let snapshot = build_snapshot(
        &run_fixture(
            repo_root().join("fixtures/golden/open-increase-reduce-close.json"),
            Some(0),
        )
        .unwrap(),
    )
    .unwrap();
    let wallet = snapshot
        .wallets
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["accountId"] == 42)
        .unwrap();
    let mut reference = json!({"source":"perpl-dex-sdk", "chainId":snapshot.chain_id,"asOfBlock":snapshot.as_of_block,
        "asOfBlockHash":snapshot.manifest["asOfBlockHash"],"asOfLogIndex":snapshot.manifest["asOfLogIndex"],
        "asOfTimestampMs":snapshot.as_of_timestamp_ms,"accountId":42,"realizedPnl":wallet["realizedPnl"],
        "realizedFunding":wallet["realizedFunding"],"fees":wallet["fees"],"freeBalance":wallet["freeBalance"]});
    assert_eq!(
        reconcile(&snapshot, &reference).unwrap()["status"],
        "matched"
    );
    reference["fees"] = json!("99");
    assert_eq!(
        reconcile(&snapshot, &reference).unwrap()["status"],
        "mismatch"
    );
    reference["fees"] = wallet["fees"].clone();
    reference.as_object_mut().unwrap().remove("freeBalance");
    assert_eq!(
        reconcile(&snapshot, &reference).unwrap()["status"],
        "unverified"
    );
    reference["asOfBlock"] = json!(0);
    assert!(reconcile(&snapshot, &reference).is_err());
}

#[test]
fn unmarked_live_positions_never_report_zero_unrealized_pnl_or_exact_balance() {
    let pulse = run_fixture(
        repo_root().join("fixtures/golden/open-position-as-of.json"),
        Some(0),
    )
    .unwrap();
    let wallet =
        perppulse::accounting::account_wallet(&pulse.ledger, 42, &pulse.fixture.as_of, &[], false)
            .unwrap();
    let value = perppulse::serve::wallet_value(&wallet, true);
    assert!(value["unrealizedPnl"].is_null());
    assert!(value["freeBalance"].is_null());
    assert_eq!(value["positions"][0]["entry"], "70000.0");
}

#[test]
fn future_timestamp_marks_and_wrong_contract_events_are_rejected() {
    let pulse = run_fixture(
        repo_root().join("fixtures/golden/open-position-as-of.json"),
        Some(0),
    )
    .unwrap();
    let mut marks = pulse.fixture.marks.clone();
    marks[0].timestamp_ms = pulse.fixture.as_of.timestamp_ms + 1;
    assert!(perppulse::accounting::account_wallet(
        &pulse.ledger,
        42,
        &pulse.fixture.as_of,
        &marks,
        true
    )
    .is_err());
    let mut events = pulse.ledger.events.clone();
    events[0].timestamp_ms = pulse.fixture.as_of.timestamp_ms + 1;
    assert!(perppulse::replay(&events, &pulse.fixture.registry, &pulse.fixture.as_of).is_err());
    events[0].timestamp_ms = 0;
    events[0].contract_address = "0xother".into();
    assert!(perppulse::replay(&events, &pulse.fixture.registry, &pulse.fixture.as_of).is_err());
}

#[test]
fn compact_event_and_incomplete_wallet_routes_fail_visibly() {
    let mut snapshot = build_snapshot(
        &run_fixture(
            repo_root().join("fixtures/golden/open-position-as-of.json"),
            Some(0),
        )
        .unwrap(),
    )
    .unwrap();
    snapshot.events_available = false;
    assert_eq!(route(&snapshot, "GET", "/api/events").0, 503);
    snapshot.wallets[0]["replayEligible"] = json!(false);
    let account = snapshot.wallets[0]["accountId"].as_u64().unwrap();
    assert_eq!(
        route(&snapshot, "GET", &format!("/api/wallet/{account}")).0,
        503
    );
}

#[test]
fn reopening_preserves_realized_pnl_funding_and_fees_across_lifecycles() {
    let pulse = run_fixture(
        repo_root().join("fixtures/golden/open-increase-reduce-close.json"),
        Some(0),
    )
    .unwrap();
    let before =
        perppulse::accounting::account_wallet(&pulse.ledger, 42, &pulse.fixture.as_of, &[], false)
            .unwrap();
    let mut reopened = pulse
        .ledger
        .events
        .iter()
        .find(|e| e.kind == perppulse::LifecycleKind::PositionOpened)
        .unwrap()
        .clone();
    reopened.block_number = pulse.fixture.as_of.block_number + 1;
    reopened.timestamp_ms = pulse.fixture.as_of.timestamp_ms + 1;
    reopened.tx_hash = "0xsecond-lifecycle".into();
    let mut cutoff = pulse.fixture.as_of.clone();
    cutoff.block_number += 1;
    cutoff.timestamp_ms += 1;
    let mut events = pulse.ledger.events.clone();
    events.push(reopened);
    let ledger = perppulse::replay(&events, &pulse.fixture.registry, &cutoff).unwrap();
    let after = perppulse::accounting::account_wallet(&ledger, 42, &cutoff, &[], false).unwrap();
    assert_eq!(after.realized_pnl, before.realized_pnl);
    assert_eq!(after.realized_funding, before.realized_funding);
    assert!(after.fees > before.fees);
}

#[test]
fn native_and_decimal_overflow_return_errors_instead_of_wrapping_facts() {
    let pulse = run_fixture(
        repo_root().join("fixtures/golden/open-increase-reduce-close.json"),
        Some(0),
    )
    .unwrap();
    let mut events = pulse.ledger.events.clone();
    events
        .iter_mut()
        .find(|e| e.kind == perppulse::LifecycleKind::PositionDecreased)
        .unwrap()
        .delta_pnl_cns = Some(i128::MAX);
    events
        .iter_mut()
        .find(|e| e.kind == perppulse::LifecycleKind::PositionClosed)
        .unwrap()
        .delta_pnl_cns = Some(1);
    assert!(
        perppulse::replay(&events, &pulse.fixture.registry, &pulse.fixture.as_of)
            .unwrap_err()
            .to_string()
            .contains("overflow")
    );
    assert!(perppulse::money::notional(
        rust_decimal::Decimal::MAX,
        rust_decimal::Decimal::from(2),
        "test"
    )
    .is_err());
}

/// Run explicitly against a disposable local DB; never skipped silently.
#[test]
#[ignore = "requires PERPPULSE_TEST_DATABASE_URL pointing at a disposable loopback database"]
fn postgres_round_trip_staleness_missing_and_regression_are_fail_closed() {
    let url =
        std::env::var("PERPPULSE_TEST_DATABASE_URL").expect("test DB environment is required");
    let snapshot = build_snapshot(
        &run_fixture(
            repo_root().join("fixtures/golden/open-position-as-of.json"),
            Some(0),
        )
        .unwrap(),
    )
    .unwrap();
    perppulse::publication::publish(&url, "integration-test", &snapshot).unwrap();
    perppulse::publication::publish(&url, "integration-test", &snapshot).unwrap();
    let loaded = perppulse::publication::load(&url, "integration-test", 90).unwrap();
    assert_eq!(loaded.wallets, snapshot.wallets);
    assert!(!loaded.events_available);
    assert!(perppulse::publication::load(&url, "missing-source", 90).is_err());
    let mut earlier = snapshot.clone();
    earlier.as_of_block -= 1;
    assert!(perppulse::publication::publish(&url, "integration-test", &earlier).is_err());
    for field in [
        "canonicalInputsHash",
        "asOfBlockHash",
        "registryInputsHash",
        "marketMarksHash",
    ] {
        let mut changed = snapshot.clone();
        changed.manifest[field] = json!("sha256:changed");
        assert!(
            perppulse::publication::publish(&url, "integration-test", &changed).is_err(),
            "{field}"
        );
    }
    let mut regressed = snapshot.clone();
    regressed.processed_block -= 1;
    assert!(perppulse::publication::publish(&url, "integration-test", &regressed).is_err());
    let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    db.execute("UPDATE perppulse_serving_snapshot SET observed_at=now()-interval '1000 seconds' WHERE source=$1", &[&"integration-test"]).unwrap();
    assert!(perppulse::publication::load(&url, "integration-test", 90).is_err());
    db.execute("UPDATE perppulse_serving_snapshot SET observed_at=now(),content_hash='sha256:invalid' WHERE source=$1", &[&"integration-test"]).unwrap();
    assert!(perppulse::publication::load(&url, "integration-test", 90).is_err());
}
