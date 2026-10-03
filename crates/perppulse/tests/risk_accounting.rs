use perppulse::{accounting::account_wallet, events::load_fixture, events::repo_root, replay};
use perppulse::{
    accounting::isolated_liquidation_price,
    pipeline::run_fixture,
    registry::{SIDE_LONG, SIDE_SHORT},
    serve::build_snapshot,
};
use rust_decimal::Decimal;

fn dec(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap()
}

#[test]
fn maintenance_uses_effective_entry_notional_and_does_not_move_with_mark() {
    let mut fixture =
        load_fixture(repo_root().join("fixtures/golden/open-position-as-of.json")).unwrap();
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    for mark in [710000, 680000] {
        fixture.marks[0].mark_pns = mark;
        let wallet = account_wallet(&ledger, 42, &fixture.as_of, &fixture.marks, true).unwrap();
        assert_eq!(wallet.positions[0].maintenance_margin, Some(dec("2800")));
    }
}

#[test]
fn mark_alone_cannot_establish_funded_liquidation_price_or_equity() {
    let fixture =
        load_fixture(repo_root().join("fixtures/golden/open-position-as-of.json")).unwrap();
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let wallet = account_wallet(&ledger, 42, &fixture.as_of, &fixture.marks, true).unwrap();
    let position = &wallet.positions[0];
    assert_eq!(position.liquidation_price, None);
    assert_eq!(position.liquidation_buffer, None);
    assert_eq!(position.fair_market_value, None);
    assert_eq!(wallet.unrealized_pnl, None);
    assert_eq!(wallet.unrealized_funding, None);
    assert_eq!(wallet.unrealized_price_pnl, Some(dec("1000")));
}

#[test]
fn signed_funding_changes_long_and_short_liquidation_scenarios() {
    for (side, premium, expected) in [
        (SIDE_LONG, "0", "95"),
        (SIDE_LONG, "-50", "100"),
        (SIDE_LONG, "50", "90"),
        (SIDE_SHORT, "0", "105"),
        (SIDE_SHORT, "-50", "100"),
        (SIDE_SHORT, "50", "110"),
    ] {
        assert_eq!(
            isolated_liquidation_price(
                side,
                dec("100"),
                dec("10"),
                dec("100"),
                dec("20"),
                dec(premium)
            )
            .unwrap(),
            dec(expected)
        );
    }
    assert_eq!(
        isolated_liquidation_price(
            SIDE_LONG,
            dec("100"),
            dec("1"),
            dec("1000"),
            dec("20"),
            Decimal::ZERO
        )
        .unwrap(),
        Decimal::ZERO
    );
}

#[test]
fn liquidation_rejects_invalid_inputs_and_overflow_without_panicking() {
    for (side, entry, size, deposit, inverse) in [
        (0, "100", "1", "10", "20"),
        (SIDE_LONG, "0", "1", "10", "20"),
        (SIDE_LONG, "100", "0", "10", "20"),
        (SIDE_LONG, "100", "1", "-1", "20"),
        (SIDE_LONG, "100", "1", "10", "1"),
    ] {
        assert!(isolated_liquidation_price(
            side,
            dec(entry),
            dec(size),
            dec(deposit),
            dec(inverse),
            Decimal::ZERO
        )
        .is_err());
    }
    assert!(isolated_liquidation_price(
        SIDE_LONG,
        Decimal::MAX,
        dec("2"),
        Decimal::ZERO,
        dec("20"),
        Decimal::ZERO
    )
    .is_err());
}

#[test]
fn missing_one_open_mark_invalidates_wallet_price_aggregate() {
    let fixture =
        load_fixture(repo_root().join("fixtures/golden/open-position-as-of.json")).unwrap();
    let mut ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let mut position = ledger.positions.values().next().unwrap().clone();
    let mut market = ledger
        .registry
        .market(position.position_id.perpetual_id)
        .unwrap()
        .clone();
    market.perpetual_id = 777;
    position.position_id.perpetual_id = 777;
    ledger.registry.markets.insert(777, market);
    ledger
        .positions
        .insert(position.position_id.key(), position);
    let wallet = account_wallet(&ledger, 42, &fixture.as_of, &fixture.marks, false).unwrap();
    assert_eq!(wallet.positions[0].unrealized_price_pnl, Some(dec("1000")));
    assert_eq!(wallet.positions[1].unrealized_price_pnl, None);
    assert_eq!(wallet.unrealized_price_pnl, None);
    assert_eq!(wallet.unrealized_pnl, None);
    assert!(account_wallet(&ledger, 42, &fixture.as_of, &fixture.marks, true).is_err());
}

#[test]
fn api_distinguishes_unknown_actual_risk_from_zero_funding_scenarios() {
    let snapshot = build_snapshot(
        &run_fixture(
            repo_root().join("fixtures/golden/open-position-as-of.json"),
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
    let position = &wallet["positions"][0];
    assert!(wallet["unrealizedPnl"].is_null());
    assert!(wallet["unrealizedFunding"].is_null());
    assert_eq!(
        dec(wallet["unrealizedPricePnl"].as_str().unwrap()),
        dec("1000")
    );
    assert_eq!(position["riskStatus"], "funding-unverified");
    for field in ["liquidationPrice", "liquidationBuffer", "fairMarketValue"] {
        assert!(position[field].is_null());
    }
    assert_eq!(
        dec(position["maintenanceMargin"].as_str().unwrap()),
        dec("2800")
    );
    assert_eq!(
        dec(position["zeroFundingEquity"].as_str().unwrap()),
        dec("11000")
    );
    assert_eq!(
        dec(position["zeroFundingLiquidationPrice"].as_str().unwrap()),
        dec("62800")
    );
    assert_eq!(
        dec(position["zeroFundingLiquidationBuffer"].as_str().unwrap()),
        dec("8200")
    );
}

#[test]
fn closed_positions_have_no_unsettled_position_funding() {
    let fixture =
        load_fixture(repo_root().join("fixtures/golden/open-increase-reduce-close.json")).unwrap();
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let wallet = account_wallet(&ledger, 42, &fixture.as_of, &[], false).unwrap();
    assert_eq!(wallet.unrealized_pnl, Some(Decimal::ZERO));
    assert_eq!(wallet.unrealized_funding, Some(Decimal::ZERO));
    assert_eq!(wallet.unrealized_price_pnl, Some(Decimal::ZERO));
}
