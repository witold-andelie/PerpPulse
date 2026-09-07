use std::str::FromStr;

use perppulse::accounting::account_wallet;
use perppulse::events::{load_fixture, repo_root};
use perppulse::identity::AsOf;
use perppulse::ledger::replay;
use perppulse::pipeline::run_fixture;
use perppulse::quality::gate_ledger;
use rust_decimal::Decimal;

fn fixture(name: &str) -> std::path::PathBuf {
    repo_root().join("fixtures/golden").join(name)
}

fn dec(value: &str) -> Decimal {
    Decimal::from_str(value).expect("decimal")
}

#[test]
fn open_increase_reduce_close_reconstructs_realized_facts() {
    let pulse = run_fixture(fixture("open-increase-reduce-close.json"), Some(0)).expect("pulse");
    let position = pulse
        .ledger
        .positions
        .values()
        .find(|item| item.position_id.account_id == 42)
        .expect("position");
    assert_eq!(position.status, "closed");
    assert_eq!(position.lot_lns, 0);
    assert_eq!(position.realized_pnl_cns, 619_950_000);
    assert_eq!(position.realized_funding_cns, 2_000_000);
    assert_eq!(position.fees_cns, 103_500);
    assert_eq!(pulse.metrics.taker_volume, dec("212620"));
    assert_eq!(pulse.metrics.open_interest, Decimal::ZERO);
    assert_eq!(pulse.metrics.tvl, Decimal::ZERO);
    assert_eq!(pulse.metrics.liquidations, 0);
    let wallet = account_wallet(
        &pulse.ledger,
        42,
        &pulse.fixture.as_of,
        &pulse.fixture.marks,
        true,
    )
    .unwrap();
    assert_eq!(wallet.realized_pnl, dec("619.95"));
    assert_eq!(wallet.realized_funding, dec("2"));
    assert_eq!(wallet.fees, dec("0.1035"));
    assert_eq!(wallet.free_balance, dec("20000"));
}

#[test]
fn open_position_uses_as_of_mark_without_lookahead() {
    let pulse = run_fixture(fixture("open-position-as-of.json"), Some(0)).expect("pulse");
    let wallet = account_wallet(
        &pulse.ledger,
        42,
        &pulse.fixture.as_of,
        &pulse.fixture.marks,
        true,
    )
    .unwrap();
    let position = &wallet.positions[0];
    assert_eq!(position.status, "open");
    assert_eq!(position.size, dec("1"));
    assert_eq!(position.entry, dec("70000"));
    assert_eq!(position.mark, Some(dec("71000")));
    assert_eq!(position.unrealized_pnl, Some(dec("1000")));
    assert_eq!(position.fair_market_value, Some(dec("11000")));
    assert_eq!(position.notional_value, Some(dec("71000")));
    assert_eq!(position.maintenance_margin, Some(dec("2840")));
    assert_eq!(position.liquidation_buffer, Some(dec("8160")));
    assert_eq!(position.liquidation_price, Some(dec("62500")));
    assert_eq!(pulse.metrics.open_interest, dec("71000"));
    assert_eq!(pulse.metrics.tvl, dec("10000"));
}

#[test]
fn liquidation_closes_position_from_source_event() {
    let pulse = run_fixture(fixture("liquidation.json"), Some(0)).expect("pulse");
    let position = pulse
        .ledger
        .positions
        .values()
        .find(|item| item.position_id.account_id == 99)
        .expect("liquidated position");
    assert_eq!(position.status, "liquidated");
    assert_eq!(position.lot_lns, 0);
    assert_eq!(position.realized_pnl_cns, -7_500_000_000);
    assert_eq!(position.realized_funding_cns, -2_000_000);
    assert_eq!(pulse.metrics.liquidations, 1);
    assert_eq!(pulse.metrics.markets[0].liquidation_notional, dec("62500"));
}

#[test]
fn stale_as_of_fails_closed() {
    let error = match run_fixture(fixture("stale-as-of.json"), Some(0)) {
        Ok(_) => panic!("stale as-of should fail closed"),
        Err(err) => err,
    };
    assert!(
        error.to_string().contains("processed coverage is stale"),
        "{error}"
    );
}

#[test]
fn missing_mark_fails_closed_for_open_positions() {
    let mut fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    fixture.marks.clear();
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let error =
        account_wallet(&ledger, 42, &fixture.as_of, &fixture.marks, true).expect_err("mark");
    assert!(error.to_string().contains("missing as-of mark"), "{error}");
}

#[test]
fn duplicate_event_identity_is_rejected() {
    let fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    let mut events = fixture.events.clone();
    events.push(events[0].clone());
    let error = replay(&events, &fixture.registry, &fixture.as_of).expect_err("duplicate");
    assert!(
        error.to_string().contains("duplicate event identity"),
        "{error}"
    );
}

#[test]
fn unknown_market_fails_closed() {
    let mut fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    fixture.events[1].perpetual_id = Some(999);
    let error = replay(&fixture.events, &fixture.registry, &fixture.as_of).expect_err("unknown");
    assert!(
        error.to_string().contains("unknown perpetual_id"),
        "{error}"
    );
}

#[test]
fn excluded_legacy_sol_market_is_not_listed() {
    let registry =
        perppulse::load_registry(repo_root().join("fixtures/protocol/mainnet-registry.json"))
            .unwrap();
    let error = registry.market(30).expect_err("legacy");
    assert!(error.to_string().contains("excluded"), "{error}");
    assert_eq!(registry.market(31).unwrap().symbol, "SOL");
}

#[test]
fn sqlite_store_round_trips_canonical_events() {
    let fixture = load_fixture(fixture("liquidation.json")).unwrap();
    let store = perppulse::store::EventStore::memory().unwrap();
    store.insert_many(&fixture.events).unwrap();
    let loaded = store.load_all().unwrap();
    assert_eq!(loaded.len(), fixture.events.len());
    let last = fixture.events.last().unwrap();
    let fetched = store.get(&last.event_id().unwrap().key()).unwrap();
    assert_eq!(fetched.abi_event_name, "PositionLiquidated");
}

#[test]
fn isolated_positions_do_not_share_collateral() {
    let mut fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    let mut eth = fixture.events[1].clone();
    eth.log_index = 2;
    eth.tx_hash = "0xtxethccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
    eth.perpetual_id = Some(20);
    eth.price_pns = Some(240000);
    eth.lot_lns = Some(1000);
    eth.deposit_cns = Some(3_000_000_000);
    eth.prot_fee_cns = Some(20000);
    fixture.events.push(eth);
    fixture.marks.push(perppulse::events::MarketMark {
        perpetual_id: 20,
        mark_pns: 241000,
        oracle_pns: Some(240900),
        block_number: 54773030,
        timestamp_ms: 1770000900000,
    });
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let btc = ledger
        .positions
        .values()
        .find(|item| item.position_id.perpetual_id == 1)
        .unwrap();
    let eth = ledger
        .positions
        .values()
        .find(|item| item.position_id.perpetual_id == 20)
        .unwrap();
    assert_eq!(btc.deposit_cns, 10_000_000_000);
    assert_eq!(eth.deposit_cns, 3_000_000_000);
    assert!(btc.is_open() && eth.is_open());
}

#[test]
fn demo_walk_covers_protocol_wallet_and_event() {
    let pulse = run_fixture(fixture("open-increase-reduce-close.json"), Some(0)).unwrap();
    assert!(!pulse.metrics.markets.is_empty());
    let wallets = pulse.wallets(true).unwrap();
    assert!(wallets.iter().any(|wallet| wallet.account_id == 42));
    let last = pulse.ledger.events.last().unwrap().event_id().unwrap();
    let evidence = pulse.store.get(&last.key()).unwrap();
    assert_eq!(evidence.kind, perppulse::LifecycleKind::MakerFill);
    assert_eq!(
        gate_ledger(
            &pulse.ledger,
            &pulse.fixture.as_of,
            &pulse.fixture.coverage,
            Some(0)
        )
        .unwrap()
        .status,
        "eligible"
    );
}

#[test]
fn processed_coverage_accepts_a_quiet_interval_without_faking_an_event() {
    let fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    let as_of = AsOf::new(
        fixture.as_of.chain_id,
        fixture.as_of.block_number + 100,
        "0xquietcoveragecccccccccccccccccccccccccccccccccccccccccccccccc",
        fixture.as_of.timestamp_ms + 60_000,
        None,
    )
    .unwrap();
    let mut coverage = fixture.coverage.clone();
    coverage.processed_block = as_of.block_number;
    let ledger = replay(&fixture.events, &fixture.registry, &as_of).unwrap();
    let report = gate_ledger(&ledger, &as_of, &coverage, Some(0)).unwrap();
    assert_eq!(report.coverage_lag_blocks, 0);
    assert_eq!(report.event_silence_blocks, 100);
    assert!(report.notes[0].contains("no matching canonical event"));
}

#[test]
fn unspecified_transition_side_preserves_the_existing_position_side() {
    let mut fixture = load_fixture(fixture("open-increase-reduce-close.json")).unwrap();
    let increased = fixture
        .events
        .iter_mut()
        .find(|event| event.kind == perppulse::LifecycleKind::PositionIncreased)
        .expect("increase event");
    increased.position_type = Some(0);
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let position = ledger
        .positions
        .values()
        .find(|position| position.position_id.account_id == 42)
        .expect("position");
    assert_eq!(position.side, 1);
}

#[test]
fn exact_post_event_balance_updates_account_state() {
    let mut fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    let opened = fixture
        .events
        .iter()
        .find(|event| event.kind == perppulse::LifecycleKind::PositionOpened)
        .expect("open event")
        .clone();
    let mut fill = opened;
    fill.kind = perppulse::LifecycleKind::MakerFill;
    fill.abi_event_name = "MakerOrderFilled".to_string();
    fill.log_index += 1;
    fill.tx_hash = "0xbalancefillcccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
    fill.price_pns = Some(700_000);
    fill.lot_lns = Some(100_000);
    fill.fee_cns = Some(100);
    fill.balance_cns = Some(12_345_678);
    fixture.events.push(fill);
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    assert_eq!(ledger.accounts.get(&42).unwrap().balance_cns, 12_345_678);
}

#[test]
fn collateral_decrease_updates_deposit_and_entry_from_end_state() {
    let mut fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    let opened = fixture
        .events
        .iter()
        .find(|event| event.kind == perppulse::LifecycleKind::PositionOpened)
        .expect("open event")
        .clone();
    let mut decreased = opened;
    decreased.kind = perppulse::LifecycleKind::CollateralDecreased;
    decreased.abi_event_name = "PositionCollateralDecreased".to_string();
    decreased.log_index += 1;
    decreased.tx_hash =
        "0xcollateralcccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
    decreased.position_type = Some(0);
    decreased.start_deposit_cns = Some(10_000_000_000);
    decreased.end_deposit_cns = Some(8_000_000_000);
    decreased.price_pns = Some(695_000);
    fixture.events.push(decreased);

    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let position = ledger.positions.values().next().unwrap();
    assert_eq!(position.deposit_cns, 8_000_000_000);
    assert_eq!(position.entry_pns, 695_000);
}

#[test]
fn liquidation_credit_requires_and_applies_exact_start_deposit() {
    let mut fixture = load_fixture(fixture("open-position-as-of.json")).unwrap();
    let opened = fixture
        .events
        .iter()
        .find(|event| event.kind == perppulse::LifecycleKind::PositionOpened)
        .expect("open event")
        .clone();
    let mut credit = opened;
    credit.kind = perppulse::LifecycleKind::PositionLiquidationCredit;
    credit.abi_event_name = "PositionLiquidationCredit".to_string();
    credit.log_index += 1;
    credit.tx_hash = "0xliqcreditccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
    credit.position_type = None;
    credit.start_deposit_cns = Some(10_000_000_000);
    credit.end_deposit_cns = Some(10_500_000_000);
    fixture.events.push(credit);

    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    assert_eq!(
        ledger.positions.values().next().unwrap().deposit_cns,
        10_500_000_000
    );

    let mut inconsistent = fixture;
    inconsistent.events.last_mut().unwrap().start_deposit_cns = Some(9_999_999_999);
    let error = replay(
        &inconsistent.events,
        &inconsistent.registry,
        &inconsistent.as_of,
    )
    .expect_err("inconsistent transition");
    assert!(error.to_string().contains("start deposit"), "{error}");
}
