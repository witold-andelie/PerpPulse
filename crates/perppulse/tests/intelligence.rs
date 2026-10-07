use perppulse::analytics::{analyze, CoverageBasis};
use perppulse::events::repo_root;
use perppulse::pipeline::run_fixture;
use perppulse::serve::{build_snapshot, route};
use perppulse::signals::{carry_forward, SignalReport};
use rust_decimal::Decimal;
use serde_json::{json, Value};

const COHORT: &str = "fixtures/golden/watchlist-cohort.json";
const DAY: i64 = 86_400_000;

fn dec(text: &str) -> Decimal {
    Decimal::from_str_exact(text).unwrap()
}

fn cohort() -> perppulse::serve::ApiSnapshot {
    build_snapshot(&run_fixture(repo_root().join(COHORT), Some(0)).unwrap()).unwrap()
}

fn signal<'a>(snapshot: &'a perppulse::serve::ApiSnapshot, id: &str) -> &'a Value {
    snapshot.signals["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["signalId"] == id)
        .unwrap_or_else(|| panic!("missing signal {id}"))
}

#[test]
fn cohort_windows_are_complete_from_deployment_and_bound_to_the_cutoff() {
    let snapshot = cohort();
    let windows = snapshot.analytics["windows"].as_array().unwrap();
    let ids: Vec<_> = windows.iter().map(|w| w["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["24h", "7d", "30d", "coverage"]);
    for window in windows {
        assert_eq!(window["status"], "complete");
        assert_eq!(window["endBlock"], snapshot.as_of_block);
        assert!(window["eventIdsHash"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
    }
    let day = &windows[0]["totals"];
    assert_eq!(dec(day["takerVolume"].as_str().unwrap()), dec("185200"));
    assert_eq!(day["trades"], 3);
    assert_eq!(day["liquidations"], 1);
    assert_eq!(
        dec(day["liquidationNotional"].as_str().unwrap()),
        dec("26520")
    );
    let week = &windows[1]["totals"];
    assert_eq!(
        dec(week["netCollateralFlow"].as_str().unwrap()),
        dec("-12000")
    );
    let all = &windows[3]["totals"];
    assert_eq!(
        dec(all["collateralDeposits"].as_str().unwrap()),
        dec("116000")
    );
    assert_eq!(dec(all["takerVolume"].as_str().unwrap()), dec("415550"));
    // The coverage window equals the existing protocol metric definitions.
    assert_eq!(
        dec(all["takerVolume"].as_str().unwrap()),
        dec(snapshot.protocol["takerVolume"].as_str().unwrap())
    );
    assert_eq!(
        dec(all["protocolFees"].as_str().unwrap()),
        dec(snapshot.protocol["protocolFees"].as_str().unwrap())
    );
    let state = &snapshot.analytics["state"];
    assert_eq!(state["status"], "complete");
    assert_eq!(
        dec(state["openInterest"].as_str().unwrap()),
        dec(snapshot.protocol["openInterest"].as_str().unwrap())
    );
    assert_eq!(dec(state["skew"].as_str().unwrap()), dec("0.285823"));
}

#[test]
fn bounded_coverage_reports_incomplete_windows_and_unavailable_state() {
    let pulse = run_fixture(repo_root().join(COHORT), Some(0)).unwrap();
    let as_of = &pulse.fixture.as_of;
    let start_block = pulse.fixture.registry.deployed_at_block + 1_080_000;
    let events: Vec<_> = pulse
        .ledger
        .events
        .iter()
        .filter(|e| e.block_number >= start_block)
        .cloned()
        .collect();
    let deployed = pulse.fixture.registry.deployed_at_block;
    let report = |start_timestamp_ms| {
        analyze(
            &events,
            &pulse.fixture.registry,
            as_of,
            &CoverageBasis {
                start_block,
                deployed_at_block: deployed,
                start_timestamp_ms,
            },
            Some((&pulse.ledger, &pulse.fixture.marks)),
            "test",
        )
        .unwrap()
    };
    let unobserved = report(None);
    assert_eq!(unobserved.history_basis, "bounded-coverage");
    assert_eq!(unobserved.state.status, "unavailable");
    assert!(unobserved.state.open_interest.is_none());
    let day = unobserved.window("24h").unwrap();
    assert_eq!(day.status, "incomplete");
    assert_eq!(day.basis, "coverage-start-unobserved");
    assert!(day.totals.is_none());
    assert_eq!(unobserved.window("coverage").unwrap().status, "complete");

    // Coverage began five days before the cutoff: 24h is provable, 7d/30d are not.
    let observed = report(Some(as_of.timestamp_ms - 5 * DAY));
    assert_eq!(observed.window("24h").unwrap().status, "complete");
    assert_eq!(
        observed.window("24h").unwrap().basis,
        "observed-coverage-start"
    );
    assert_eq!(observed.window("7d").unwrap().status, "incomplete");
    assert_eq!(
        observed.window("7d").unwrap().basis,
        "coverage-after-window-start"
    );
    assert_eq!(observed.window("30d").unwrap().status, "incomplete");
}

#[test]
fn analytics_reject_inputs_outside_coverage_and_duplicate_positions() {
    let pulse = run_fixture(repo_root().join(COHORT), Some(0)).unwrap();
    let registry = &pulse.fixture.registry;
    let basis = CoverageBasis {
        start_block: pulse.ledger.events[0].block_number + 1,
        deployed_at_block: registry.deployed_at_block,
        start_timestamp_ms: None,
    };
    assert!(analyze(
        &pulse.ledger.events,
        registry,
        &pulse.fixture.as_of,
        &basis,
        None,
        "t"
    )
    .is_err());
    let mut duplicated = pulse.ledger.events.clone();
    let mut copy = duplicated[3].clone();
    copy.tx_hash = "0xdifferent".into();
    duplicated.push(copy);
    let deployment = CoverageBasis {
        start_block: registry.deployed_at_block,
        ..basis.clone()
    };
    assert!(analyze(
        &duplicated,
        registry,
        &pulse.fixture.as_of,
        &deployment,
        None,
        "t"
    )
    .is_err());
    let future = CoverageBasis {
        start_timestamp_ms: Some(pulse.fixture.as_of.timestamp_ms + 1),
        ..deployment
    };
    assert!(analyze(
        &pulse.ledger.events,
        registry,
        &pulse.fixture.as_of,
        &future,
        None,
        "t"
    )
    .is_err());
}

#[test]
fn stale_marks_make_market_state_partial_instead_of_failing() {
    let pulse = run_fixture(repo_root().join(COHORT), Some(0)).unwrap();
    let mut marks = pulse.fixture.marks.clone();
    let btc = marks.iter_mut().find(|m| m.perpetual_id == 1).unwrap();
    btc.timestamp_ms -= 120_000;
    btc.block_number -= 300;
    btc.block_hash = None;
    let report = analyze(
        &pulse.ledger.events,
        &pulse.fixture.registry,
        &pulse.fixture.as_of,
        &CoverageBasis {
            start_block: pulse.fixture.registry.deployed_at_block,
            deployed_at_block: pulse.fixture.registry.deployed_at_block,
            start_timestamp_ms: None,
        },
        Some((&pulse.ledger, &marks)),
        "test",
    )
    .unwrap();
    assert_eq!(report.state.status, "partial");
    assert!(report.state.open_interest.is_none());
    assert!(report.state.position_collateral.is_some());
    let btc = report
        .state
        .markets
        .iter()
        .find(|m| m.perpetual_id == 1)
        .unwrap();
    assert_eq!(btc.status, "mark-unavailable");
    assert!(btc.skew.is_none());
    let eth = report
        .state
        .markets
        .iter()
        .find(|m| m.perpetual_id == 20)
        .unwrap();
    assert_eq!(eth.status, "complete");
}

#[test]
fn signals_rank_funded_liquidation_first_and_trace_rule_and_evidence() {
    let snapshot = cohort();
    let signals = &snapshot.signals;
    assert_eq!(signals["version"], "risk-signals-v1");
    assert_eq!(
        signals["topSignalIds"],
        json!([
            "liquidation-distance:102:1",
            "liquidation-activity:-:10",
            "leverage-utilization:105:20"
        ])
    );
    let critical = signal(&snapshot, "liquidation-distance:102:1");
    assert_eq!(critical["severity"], "critical");
    assert_eq!(critical["metric"], "0.007345");
    assert_eq!(critical["basis"], "canonical");
    assert_eq!(
        dec(critical["inputs"]["liquidationPrice"].as_str().unwrap()),
        dec("66011.5")
    );
    assert_eq!(
        dec(critical["inputs"]["unrealizedFunding"].as_str().unwrap()),
        dec("-1.8")
    );
    assert!(critical["evidence"]["markEventId"].is_string());
    assert!(critical["evidence"]["fundingResetEventId"].is_string());
    let rule = signals["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|rule| rule["id"] == "liquidation-distance")
        .unwrap();
    assert_eq!(critical["ruleHash"], rule["ruleHash"]);
    let mut definition = rule.clone();
    definition.as_object_mut().unwrap().remove("ruleHash");
    assert_eq!(
        rule["ruleHash"],
        json!(perppulse::evidence::digest(&definition).unwrap())
    );
    assert_eq!(signals["counts"]["critical"], 2);

    let liquidation = signal(&snapshot, "liquidation-activity:-:10");
    assert_eq!(liquidation["metric"], "0.654814");
    assert_eq!(
        signal(&snapshot, "position-liquidated:104:10")["severity"],
        "warning"
    );
    // Uncovered MON funding is a visible data-quality signal, never a risk fact.
    let unverified = signal(&snapshot, "risk-input-unavailable:105:10");
    assert_eq!(unverified["basis"], "zero-funding-conditional");
    assert!(unverified["metric"].is_null());
    // Complete protocol state supersedes snapshot crowding for the same market.
    assert!(signals["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["ruleId"] != "watchlist-crowding"));
    assert_eq!(signal(&snapshot, "market-skew:-:20")["side"], "long");
    assert_eq!(signal(&snapshot, "market-skew:-:10")["side"], "short");
}

#[test]
fn signal_evaluation_is_deterministic_and_bounded_by_threshold_edges() {
    let first = cohort();
    let second = cohort();
    assert_eq!(first.signals, second.signals);
    // Exactly 25% drawdown crosses the inclusive watch threshold.
    let drawdown = signal(&first, "collateral-drawdown:101:1");
    assert_eq!(drawdown["metric"], "0.25");
    assert_eq!(drawdown["severity"], "watch");
    // Healthy positions do not emit noise.
    assert!(first.signals["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["signalId"] != "collateral-drawdown:103:1"));
}

#[test]
fn stress_scenarios_apply_shocks_to_eligible_marks_with_funding_basis() {
    let snapshot = cohort();
    let scenarios = snapshot.signals["stress"]["scenarios"].as_array().unwrap();
    let breached: Vec<_> = scenarios
        .iter()
        .map(|s| s["breached"].as_u64().unwrap())
        .collect();
    assert_eq!(breached, [4, 3, 1, 0, 0, 1]);
    let down_five = &scenarios[2];
    assert_eq!(down_five["shock"], "-0.05");
    assert_eq!(down_five["breaches"][0]["accountId"], 102);
    assert_eq!(down_five["breaches"][0]["basis"], "funded");
    assert_eq!(scenarios[5]["breaches"][0]["side"], "short");
    assert!(snapshot.signals["stress"]["excluded"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn carry_forward_marks_new_escalated_unchanged_and_resolved_signals() {
    let snapshot = cohort();
    let base: SignalReport = serde_json::from_value(snapshot.signals.clone()).unwrap();
    assert!(base.items.iter().all(|item| item.change == "baseline"));

    let mut previous = base.clone();
    previous.as_of_block -= 10;
    for item in &mut previous.items {
        item.as_of_block = previous.as_of_block;
        item.first_observed_as_of_block = previous.as_of_block - 90;
        item.severity_since_as_of_block = previous.as_of_block - 50;
    }
    // Previously warning, now critical; previously present, now gone; absent before.
    previous.items[0].severity = "warning".into();
    let gone = previous.items.pop().unwrap();
    let mut extra = previous.items[1].clone();
    extra.signal_id = "collateral-drawdown:999:1".into();
    previous.items.push(extra);
    let mut current = base.clone();
    carry_forward(Some(&previous), &mut current);
    assert_eq!(current.baseline_as_of_block, Some(previous.as_of_block));
    let top = &current.items[0];
    assert_eq!(top.change, "escalated");
    assert_eq!(top.first_observed_as_of_block, previous.as_of_block - 90);
    assert_eq!(top.severity_since_as_of_block, current.as_of_block);
    assert_eq!(current.items[1].change, "unchanged");
    assert_eq!(
        current.items[1].severity_since_as_of_block,
        previous.as_of_block - 50
    );
    let reappeared = current
        .items
        .iter()
        .find(|i| i.signal_id == gone.signal_id)
        .unwrap();
    assert_eq!(reappeared.change, "new");
    assert_eq!(current.resolved.len(), 1);
    assert_eq!(current.resolved[0].signal_id, "collateral-drawdown:999:1");
    assert_eq!(
        current.resolved[0].resolved_as_of_block,
        current.as_of_block
    );

    // Resolved entries expire after the retention window and never cross a regression.
    let mut later = base.clone();
    later.as_of_block = current.as_of_block + perppulse::signals::RESOLVED_RETENTION_BLOCKS + 1;
    carry_forward(Some(&current), &mut later);
    assert!(later.resolved.is_empty());
    let mut regressed = base.clone();
    regressed.as_of_block = previous.as_of_block - 1;
    carry_forward(Some(&current), &mut regressed);
    assert!(regressed.baseline_as_of_block.is_none());
}

#[test]
fn comparison_ranks_only_known_values_and_measures_exposure_overlap() {
    let snapshot = cohort();
    let cohort = &snapshot.cohort;
    assert_eq!(cohort["version"], "snapshot-cohort-v1");
    assert_eq!(cohort["members"], 7);
    let wallet = |id: u64| {
        cohort["wallets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["accountId"] == id)
            .unwrap()
            .clone()
    };
    let w102 = wallet(102);
    assert_eq!(
        dec(w102["collateralLeverage"].as_str().unwrap()),
        dec("11.565217")
    );
    assert_eq!(
        dec(w102["priceReturnOnCollateral"].as_str().unwrap()),
        dec("-0.434782")
    );
    // Four wallets have open collateral; 102 is the most leveraged of them.
    assert_eq!(w102["percentiles"]["collateralLeverage"], "87.5");
    // Accounts without open positions are not ranked on return or leverage.
    assert!(wallet(106)["percentiles"]["collateralLeverage"].is_null());
    let pair = cohort["overlap"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["accountA"] == 103 && p["accountB"] == 105)
        .unwrap();
    assert_eq!(pair["jaccard"], "0.333333");
    assert_eq!(pair["sameSideMarkets"], 1);
    assert_eq!(pair["sharedMarkets"][0]["symbol"], "ETH");
    assert_eq!(cohort["labels"]["status"], "unavailable");
}

#[test]
fn ineligible_histories_are_excluded_and_labels_group_participants() {
    let snapshot = cohort();
    let mut wallets = snapshot.wallets.clone();
    let rows = wallets.as_array_mut().unwrap();
    rows[1]["replayEligible"] = json!(false);
    rows[1]["replayBasis"] = json!("incomplete-history");
    rows[2]["context"] = json!({"source":"Nansen","status":"available","attribution":"Powered by Nansen API",
        "pointInTimeEligible":false,"labels":[{"label":"Synthetic fund","category":"fund"},{"label":"Synthetic trader","category":null}]});
    rows[3]["context"] = json!({"source":"Nansen","status":"partial","pointInTimeEligible":true,
        "labels":[{"label":"Synthetic fund","category":"fund"}]});
    rows[4]["context"] = json!({"source":"Nansen","status":"available","labels":[]});
    let excluded = rows[1]["accountId"].clone();
    let unlabeled = rows[4]["accountId"].as_u64().unwrap();
    let report = perppulse::cohort::evaluate("test", snapshot.as_of_block, &wallets).unwrap();
    assert_eq!(report.excluded[0]["accountId"], excluded);
    assert!(report
        .wallets
        .iter()
        .all(|w| json!(w.account_id) != excluded));
    assert_eq!(report.labels.status, "partial");
    assert_eq!(
        report.labels.attribution.as_deref(),
        Some("Powered by Nansen API")
    );
    let fund = report
        .labels
        .groups
        .iter()
        .find(|g| g.label == "Synthetic fund")
        .unwrap();
    assert_eq!(fund.members.len(), 2);
    assert!(!fund.members[0].point_in_time_eligible);
    assert!(fund.members[1].point_in_time_eligible);
    let trader = report
        .labels
        .groups
        .iter()
        .find(|g| g.label == "Synthetic trader")
        .unwrap();
    assert!(trader.category.is_none());
    assert_eq!(report.labels.unlabeled_accounts, vec![unlabeled]);
    let signals = perppulse::signals::evaluate(&perppulse::signals::SignalInputs {
        scope: "test",
        as_of_block: snapshot.as_of_block,
        as_of_timestamp_ms: snapshot.as_of_timestamp_ms,
        wallets: &wallets,
        events: &snapshot.events,
        analytics: None,
        registry: &run_fixture(repo_root().join(COHORT), Some(0))
            .unwrap()
            .fixture
            .registry,
    })
    .unwrap();
    let incomplete = format!("incomplete-history:{excluded}:-");
    assert!(signals
        .items
        .iter()
        .any(|item| item.signal_id == incomplete));
    // Without protocol state the snapshot crowding rule becomes visible.
    assert!(signals
        .items
        .iter()
        .any(|item| item.rule_id == "watchlist-crowding"));
    assert_eq!(signals.stress.excluded[0]["reason"], "incomplete-history");
}

#[test]
fn methodology_documents_the_served_versions_and_rule_ids() {
    let methodology: Value = serde_json::from_str(perppulse::evidence::METHODOLOGY).unwrap();
    let signals = &methodology["riskSignals"];
    assert_eq!(signals["version"], perppulse::signals::SIGNAL_VERSION);
    assert_eq!(signals["stressVersion"], perppulse::signals::STRESS_VERSION);
    let documented: Vec<_> = signals["rules"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut served: Vec<_> = perppulse::signals::rule_definitions()
        .unwrap()
        .iter()
        .map(|rule| rule["id"].as_str().unwrap().to_string())
        .collect();
    served.sort();
    assert_eq!(documented, served);
    assert_eq!(
        methodology["protocolAnalytics"]["version"],
        perppulse::analytics::ANALYTICS_VERSION
    );
    assert_eq!(
        methodology["comparison"]["version"],
        perppulse::cohort::COHORT_VERSION
    );
}

#[test]
fn new_routes_serve_reports_and_fail_visibly_without_them() {
    let snapshot = cohort();
    for path in ["/api/analytics", "/api/signals", "/api/comparison"] {
        assert_eq!(route(&snapshot, "GET", path).0, 200, "{path}");
        assert_eq!(route(&snapshot, "POST", path).0, 405, "{path}");
        assert_eq!(
            route(&snapshot, "GET", &format!("{path}?limit=1")).0,
            400,
            "{path}"
        );
    }
    let mut missing = snapshot.clone();
    missing.analytics = perppulse::serve::unavailable_analytics("selected-accounts", "test");
    missing.signals = Value::Null;
    missing.cohort = Value::Null;
    assert_eq!(route(&missing, "GET", "/api/analytics").0, 503);
    assert_eq!(route(&missing, "GET", "/api/signals").0, 503);
    assert_eq!(route(&missing, "GET", "/api/comparison").0, 503);
    // Older compact snapshots without the new fields keep their exact hash.
    let mut legacy = snapshot.clone();
    legacy.analytics = Value::Null;
    legacy.signals = Value::Null;
    legacy.cohort = Value::Null;
    let text = serde_json::to_string(&legacy).unwrap();
    assert!(!text.contains("\"signals\""));
    let parsed: perppulse::serve::ApiSnapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(
        perppulse::evidence::digest(&parsed).unwrap(),
        perppulse::evidence::digest(&legacy).unwrap()
    );
}
