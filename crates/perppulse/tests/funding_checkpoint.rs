use perppulse::{
    accounting::account_wallet, events::repo_root, funding_checkpoint::FundingCoverage,
    ledger::replay_with_funding_coverage, load_registry, replay, AsOf, CanonicalEvent, Ledger,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};

const START: u64 = 54773010;
fn dec(n: &str) -> Decimal {
    Decimal::from_str_exact(n).unwrap()
}
fn event(block: u64, log: u32, kind: &str, fields: Value) -> CanonicalEvent {
    let mut value = json!({"chain_id":143,"block_hash":format!("0x{block:x}"),"tx_hash":format!("0x{block:x}"),
        "block_number":block,"log_index":log,"timestamp_ms":block as i64 * 1000,
        "contract_address":"0x34B6552d57a35a1D042CcAe1951BD1C370112a6F","abi_event_name":"SyntheticFixture",
        "kind":kind,"perpetual_id":1});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}
fn schedule(block: u64, target: u64, payment: i128, sum: i128, overwrite: bool) -> CanonicalEvent {
    event(
        block,
        0,
        "market_funding",
        json!({"funding_rate_pct100k":1,"funding_event_block":target,
        "funding_payment_pns":payment,"funding_sum_pns":sum,"funding_allow_overwrite":overwrite}),
    )
}
fn scale(block: u64, exp: u32) -> CanonicalEvent {
    event(
        block,
        1,
        "funding_scale_updated",
        json!({"funding_scaling_exp":exp}),
    )
}
fn base(side: u8, with_scale: bool) -> Vec<CanonicalEvent> {
    let mut events = vec![
        event(
            START,
            0,
            "account_created",
            json!({"account_id":42,"owner":"0x1111111111111111111111111111111111111111"}),
        ),
        schedule(START + 1, START + 2, 0, 0, false),
        event(
            START + 3,
            0,
            "position_opened",
            json!({"account_id":42,"position_type":side,
            "lot_lns":100000,"price_pns":1000,"deposit_cns":20000000}),
        ),
    ];
    if with_scale {
        events.push(scale(START, 0));
    }
    events
}
fn cutoff(block: u64) -> AsOf {
    AsOf::new(
        143,
        block,
        format!("0x{block:x}"),
        block as i64 * 1000,
        None,
    )
    .unwrap()
}
fn run(events: &[CanonicalEvent], block: u64) -> Ledger {
    run_from(events, block, START, vec![1])
}
fn run_from(events: &[CanonicalEvent], block: u64, start: u64, markets: Vec<u32>) -> Ledger {
    let registry =
        load_registry(repo_root().join("fixtures/protocol/mainnet-registry.json")).unwrap();
    replay_with_funding_coverage(
        events,
        &registry,
        &cutoff(block),
        &FundingCoverage {
            start_block: start,
            end_block: block,
            market_ids: markets,
        },
    )
    .unwrap()
}
fn amount(ledger: &Ledger) -> Option<Decimal> {
    ledger.open_positions()[0].funding_checkpoint.unsettled_pnl
}

#[test]
fn payment_waits_for_target_and_applies_in_a_quiet_block_once() {
    let mut events = base(1, true);
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    assert_eq!(amount(&run(&events, START + 5)), Some(dec("0")));
    assert_eq!(amount(&run(&events, START + 6)), Some(dec("-10")));
    assert_eq!(amount(&run(&events, START + 9)), Some(dec("-10")));
    events.reverse();
    assert_eq!(amount(&run(&events, START + 9)), Some(dec("-10")));
}

#[test]
fn long_pays_short_receives_and_signed_rebates_reverse_direction() {
    for (side, payment, expected) in [
        (1, 100, "-10"),
        (2, 100, "10"),
        (1, -100, "10"),
        (2, -100, "-10"),
    ] {
        let mut events = base(side, true);
        events.push(schedule(START + 4, START + 6, payment, payment, false));
        assert_eq!(amount(&run(&events, START + 6)), Some(dec(expected)));
    }
}

#[test]
fn effective_block_payment_uses_size_before_partial_settlement() {
    let mut events = base(1, true);
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    events.push(event(START + 6, 0, "position_decreased", json!({"account_id":42,"position_type":1,
        "start_lot_lns":100000,"end_lot_lns":50000,"start_deposit_cns":20000000,"end_deposit_cns":10000000,
        "delta_pnl_cns":0,"funding_cns":-4000000})));
    let ledger = run(&events, START + 6);
    assert_eq!(amount(&ledger), Some(dec("-6")));
    let proof = &ledger.open_positions()[0].funding_checkpoint;
    assert_eq!(proof.settlement_event_ids.len(), 1);
    assert!(proof.reset_event_id.is_some() && proof.baseline_event_id.is_some());
}

#[test]
fn overwrite_replaces_pending_amount_and_scale_is_frozen_at_publication() {
    let mut events = base(1, true);
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    events.push(schedule(START + 5, START + 6, 200, 200, true));
    events.push(scale(START + 5, 2));
    assert_eq!(amount(&run(&events, START + 4)), Some(dec("0")));
    assert_eq!(amount(&run(&events, START + 6)), Some(dec("-20")));
    // The subsequent schedule uses the new unit scale, including its raw sum.
    events.push(schedule(START + 7, START + 8, 100, 20100, false));
    assert_eq!(amount(&run(&events, START + 8)), Some(dec("-20.1")));
}

#[test]
fn missing_scale_does_not_fake_nonzero_payment_but_proven_reset_recovers_zero() {
    let mut events = base(1, false);
    assert_eq!(amount(&run(&events, START + 3)), Some(dec("0")));
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    assert_eq!(amount(&run(&events, START + 6)), None);
    events.push(event(START + 7, 0, "position_increased", json!({"account_id":42,"position_type":1,
        "start_lot_lns":100000,"end_lot_lns":200000,"start_deposit_cns":20000000,"end_deposit_cns":40000000,"price_pns":1000})));
    let ledger = run(&events, START + 7);
    assert_eq!(amount(&ledger), Some(dec("0")));
    assert!(ledger.open_positions()[0]
        .funding_checkpoint
        .payment_event_ids
        .is_empty());
}

#[test]
fn no_complete_coverage_or_prebaseline_reset_remains_unknown() {
    let mut events = base(1, true);
    let registry =
        load_registry(repo_root().join("fixtures/protocol/mainnet-registry.json")).unwrap();
    assert_eq!(
        amount(&replay(&events, &registry, &cutoff(START + 5)).unwrap()),
        None
    );
    // Coverage that starts after deployment cannot see a pre-window schedule.
    let mut window = base(1, false);
    window[1].funding_event_block = Some(START + 4);
    assert_eq!(
        amount(&run_from(&window, START + 5, START + 1, vec![1])),
        None
    );
    window.retain(|e| e.kind != perppulse::LifecycleKind::MarketFunding);
    assert_eq!(
        amount(&run_from(&window, START + 5, START + 1, vec![1])),
        None
    );
    // A market outside declared funding coverage stays unknown even from deployment.
    events[1].funding_event_block = Some(START + 4);
    assert_eq!(amount(&run_from(&events, START + 5, START, vec![10])), None);
    events.retain(|e| e.kind != perppulse::LifecycleKind::MarketFunding);
    assert!(replay_with_funding_coverage(
        &events,
        &registry,
        &cutoff(START + 5),
        &FundingCoverage {
            start_block: START,
            end_block: START + 6,
            market_ids: vec![1]
        }
    )
    .is_err());
}

#[test]
fn deployment_coverage_proves_resets_before_the_first_schedule() {
    let mut events = base(1, true);
    events[1].funding_event_block = Some(START + 4);
    let ledger = run(&events, START + 3);
    let proof = &ledger.open_positions()[0].funding_checkpoint;
    assert_eq!(proof.unsettled_pnl, Some(dec("0")));
    assert!(proof.reason.is_none());
    assert_eq!(proof.baseline_effective_block, Some(START + 4));
    events.push(schedule(START + 5, START + 6, 100, 100, false));
    assert_eq!(amount(&run(&events, START + 6)), Some(dec("-10")));
    events.retain(|e| e.kind != perppulse::LifecycleKind::MarketFunding);
    assert_eq!(amount(&run(&events, START + 6)), Some(dec("0")));
}

#[test]
fn funded_pnl_equity_liquidation_and_api_proof_use_canonical_inputs() {
    let mut events = base(1, true);
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    events.push(event(
        START + 6,
        0,
        "mark_updated",
        json!({"mark_price_pns":1100}),
    ));
    let ledger = run(&events, START + 6);
    let marks = ledger.market_marks.values().cloned().collect::<Vec<_>>();
    let wallet = account_wallet(&ledger, 42, &cutoff(START + 6), &marks, true).unwrap();
    let p = &wallet.positions[0];
    assert_eq!(p.unrealized_funding, Some(dec("-10")));
    assert_eq!(p.unrealized_price_pnl, Some(dec("10")));
    assert_eq!(p.unrealized_pnl, Some(dec("0")));
    assert_eq!(p.fair_market_value, Some(dec("20")));
    assert_eq!(p.liquidation_price, Some(dec("94")));
    assert_eq!(p.liquidation_buffer, Some(dec("16")));
    assert_eq!(wallet.unrealized_funding, Some(dec("-10")));
    let value = perppulse::serve::wallet_value(&wallet, true);
    assert_eq!(
        value["positions"][0]["riskStatus"],
        "canonical-funding-covered"
    );
    assert_eq!(
        value["positions"][0]["fundingCheckpoint"]["throughBlock"],
        START + 6
    );
    let missing_mark = account_wallet(&ledger, 42, &cutoff(START + 6), &[], false).unwrap();
    assert!(missing_mark.unrealized_pnl.is_none());
    assert_eq!(missing_mark.unrealized_funding, Some(dec("-10")));
}

#[test]
fn collateral_change_preserves_funding_and_inversion_resets_it() {
    let mut events = base(1, true);
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    events.push(event(
        START + 6,
        0,
        "collateral_decreased",
        json!({"account_id":42,
        "start_deposit_cns":20000000,"end_deposit_cns":19000000,"price_pns":1000}),
    ));
    assert_eq!(amount(&run(&events, START + 6)), Some(dec("-10")));
    events.push(event(START + 7, 0, "position_inverted", json!({"account_id":42,"position_type":2,
        "start_lot_lns":100000,"end_lot_lns":100000,"start_deposit_cns":19000000,"end_deposit_cns":20000000,
        "price_pns":1000,"delta_pnl_cns":0,"funding_cns":-10000000})));
    assert_eq!(amount(&run(&events, START + 7)), Some(dec("0")));
}

#[test]
fn liquidation_and_deleveraging_subtract_settlement_before_closing() {
    for kind in ["position_liquidated", "position_deleveraged"] {
        let mut events = base(1, true);
        events.push(schedule(START + 4, START + 6, 100, 100, false));
        events.push(event(START + 6, 0, kind, json!({"account_id":42,"position_type":1,
            "start_lot_lns":100000,"end_lot_lns":50000,"start_deposit_cns":20000000,
            "end_deposit_cns":10000000,"deposit_cns":10000000,"liq_lot_lns":50000,"liq_price_pns":900,
            "delta_pnl_cns":0,"funding_cns":-4000000})));
        assert_eq!(amount(&run(&events, START + 6)), Some(dec("-6")));
        events.push(event(
            START + 7,
            0,
            "position_closed",
            json!({"account_id":42,"position_type":1,
            "price_pns":1000,"delta_pnl_cns":0,"funding_cns":-6000000}),
        ));
        let ledger = run(&events, START + 7);
        assert!(ledger.open_positions().is_empty());
        let wallet = account_wallet(&ledger, 42, &cutoff(START + 7), &[], false).unwrap();
        assert_eq!(wallet.unrealized_funding, Some(dec("0")));
        assert_eq!(
            wallet.positions[0].funding_checkpoint.unsettled_pnl,
            Some(dec("0"))
        );
    }
}

#[test]
fn one_unknown_position_invalidates_wallet_funding_and_total_pnl() {
    let mut events = base(1, true);
    events.push(event(
        START + 3,
        1,
        "mark_updated",
        json!({"mark_price_pns":1100}),
    ));
    let mut ledger = run(&events, START + 3);
    let mut other = ledger.open_positions()[0].clone();
    other.position_id.perpetual_id = 10;
    other.funding_checkpoint = Default::default();
    ledger.positions.insert(other.position_id.key(), other);
    let marks = ledger.market_marks.values().cloned().collect::<Vec<_>>();
    let wallet = account_wallet(&ledger, 42, &cutoff(START + 3), &marks, false).unwrap();
    assert!(wallet.unrealized_funding.is_none());
    assert!(wallet.unrealized_pnl.is_none());
    assert_eq!(wallet.positions[0].unrealized_funding, Some(dec("0")));
}

#[test]
fn a_scale_update_after_cutoff_cannot_fill_an_earlier_anchor_gap() {
    let mut events = base(1, false);
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    events.push(scale(START + 7, 0));
    assert_eq!(amount(&run(&events, START + 6)), None);
    assert_eq!(amount(&run(&events, START + 7)), None);
}

#[test]
fn exact_native_funding_overflow_is_an_error() {
    let mut events = base(1, true);
    events[2].lot_lns = Some(i128::MAX);
    events.push(schedule(START + 4, START + 6, 100, 100, false));
    let registry =
        load_registry(repo_root().join("fixtures/protocol/mainnet-registry.json")).unwrap();
    assert!(replay_with_funding_coverage(
        &events,
        &registry,
        &cutoff(START + 6),
        &FundingCoverage {
            start_block: START,
            end_block: START + 6,
            market_ids: vec![1]
        }
    )
    .unwrap_err()
    .to_string()
    .contains("Funding native product overflow"));
}
