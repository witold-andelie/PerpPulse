"""Build the synthetic multi-wallet cohort fixture deterministically.

The fixture is synthetic evidence for the read-only demo and tests. It is never
mainnet evidence. Run from the repository root:

    python3 scripts/build_cohort_fixture.py > fixtures/golden/watchlist-cohort.json
"""

from __future__ import annotations

import hashlib
import json
from decimal import Decimal

EXCHANGE = "0x34B6552d57a35a1D042CcAe1951BD1C370112a6F"
DEPLOYMENT_BLOCK = 54773010
T0 = 1770000000000
DAY = 86_400_000
BLOCK_MS = 400
AS_OF_MS = T0 + 10 * DAY

# perpetual id -> (price decimals, size decimals)
MARKETS = {1: (1, 5), 10: (6, 0), 20: (2, 3)}
COLLATERAL_DECIMALS = 6

events: list[dict] = []
log_counter: dict[int, int] = {}


def block_at(ms: int) -> int:
    return DEPLOYMENT_BLOCK + (ms - T0) // BLOCK_MS


def digest(text: str) -> str:
    return "0x" + hashlib.sha256(text.encode()).hexdigest()


def native(value: str | Decimal, decimals: int) -> int:
    scaled = Decimal(value) * (Decimal(10) ** decimals)
    if scaled != scaled.to_integral_value():
        raise ValueError(f"{value} is not representable at {decimals} decimals")
    return int(scaled)


def cns(value: str | Decimal) -> int:
    return native(value, COLLATERAL_DECIMALS)


def price(market: int, value: str | Decimal) -> int:
    return native(value, MARKETS[market][0])


def lots(market: int, value: str | Decimal) -> int:
    return native(value, MARKETS[market][1])


def emit(ms: int, name: str, kind: str, tx: str, **fields) -> None:
    block = block_at(ms)
    log = log_counter.get(block, 0)
    log_counter[block] = log + 1
    event = {
        "chain_id": 143,
        "block_hash": digest(f"cohort-block-{block}"),
        "tx_hash": digest(f"cohort-tx-{tx}"),
        "log_index": log,
        "block_number": block,
        "timestamp_ms": T0 + (block - DEPLOYMENT_BLOCK) * BLOCK_MS,
        "contract_address": EXCHANGE,
        "abi_event_name": name,
        "kind": kind,
    }
    event.update(fields)
    events.append(event)


def at(days: str) -> int:
    return T0 + int(Decimal(days) * DAY)


def fill(ms: int, tx: str, market: int, px: str, size: str) -> None:
    notional = Decimal(px) * Decimal(size)
    emit(ms, "MakerOrderFilled", "maker_fill", tx, account_id=7, perpetual_id=market,
         price_pns=price(market, px), lot_lns=lots(market, size),
         fee_cns=cns((notional * Decimal("0.0001")).quantize(Decimal("0.000001"))))


def open_position(ms: int, tx: str, account: int, market: int, side: int, px: str,
                  size: str, deposit: str) -> None:
    notional = Decimal(px) * Decimal(size)
    leverage = int((notional / Decimal(deposit) * 100).to_integral_value())
    emit(ms, "PositionOpened", "position_opened", tx, account_id=account, perpetual_id=market,
         position_type=side, leverage_hdths=leverage, deposit_cns=cns(deposit),
         price_pns=price(market, px), lot_lns=lots(market, size),
         ins_fee_cns=cns((notional * Decimal("0.0001")).quantize(Decimal("0.000001"))),
         prot_fee_cns=cns((notional * Decimal("0.00035")).quantize(Decimal("0.000001"))))
    fill(ms, tx, market, px, size)


def schedule(ms_pub: int, ms_eff: int, market: int, payment: int, total: int) -> None:
    emit(ms_pub, "FundingEventCompleted", "market_funding", f"funding-{market}-{ms_pub}",
         perpetual_id=market, funding_rate_pct100k=1, funding_event_block=block_at(ms_eff),
         funding_payment_pns=payment, funding_sum_pns=total, funding_allow_overwrite=False)


def mark(ms: int, market: int, px: str) -> None:
    emit(ms, "MarkUpdated", "mark_updated", f"mark-{market}-{ms}", perpetual_id=market,
         mark_price_pns=price(market, px))


def build() -> dict:
    owners = {7: "7777", 101: "a101", 102: "a102", 103: "a103", 104: "a104", 105: "a105", 106: "a106"}
    for index, (account, tag) in enumerate(owners.items()):
        ms = at("0.01") + index * 1000
        emit(ms, "AccountCreated", "account_created", f"create-{account}", account_id=account,
             owner="0x" + (tag * 10)[:40])
    for index, (account, amount) in enumerate(
        [(101, "60000"), (102, "8000"), (103, "20000"), (104, "4000"), (105, "9000"), (106, "15000")]
    ):
        ms = at("0.02") + index * 1000
        emit(ms, "CollateralDeposit", "collateral_deposit", f"deposit-{account}",
             account_id=account, amount_cns=cns(amount), balance_cns=cns(amount))

    # Funding scale anchors precede every covered payment.
    emit(at("0.05"), "FundingSumScalingExpUpdated", "funding_scale_updated", "scale-1",
         perpetual_id=1, funding_scaling_exp=0)
    emit(at("0.05") + 1000, "FundingSumScalingExpUpdated", "funding_scale_updated", "scale-20",
         perpetual_id=20, funding_scaling_exp=0)

    # BTC schedules: payment in price native units per unit size; longs pay.
    btc = [("0.1", "1.0", 20), ("1.0", "3.0", 30), ("3.0", "5.0", -10), ("5.0", "7.0", 40),
           ("7.0", "9.0", 25), ("9.0", "9.9", 15), ("9.9", "10.5", 10)]
    total = 0
    for published, effective, payment in btc:
        total += payment
        schedule(at(published) + 2000, at(effective), 1, payment, total)
    eth = [("0.1", "2.0", 50), ("2.0", "6.0", 80), ("6.0", "8.5", 120), ("8.5", "9.95", 60),
           ("9.95", "10.6", 40)]
    total = 0
    for published, effective, payment in eth:
        total += payment
        schedule(at(published) + 3000, at(effective), 20, payment, total)

    # Account 106: long BTC lifecycle, fully closed at a loss.
    open_position(at("0.5"), "106-open", 106, 1, 1, "71000", "0.3", "4000")
    emit(at("2.0"), "PositionIncreased", "position_increased", "106-inc", account_id=106,
         perpetual_id=1, position_type=1, leverage_hdths=852, price_pns=price(1, "70500"),
         start_lot_lns=lots(1, "0.3"), end_lot_lns=lots(1, "0.6"),
         start_deposit_cns=cns("4000"), end_deposit_cns=cns("5000"),
         ins_fee_cns=cns("2.07"), prot_fee_cns=cns("7.245"), funding_cns=cns("-0.6"))
    fill(at("2.0"), "106-inc", 1, "70000", "0.3")
    emit(at("4.0"), "PositionDecreased", "position_decreased", "106-dec", account_id=106,
         perpetual_id=1, position_type=1, start_lot_lns=lots(1, "0.6"), end_lot_lns=lots(1, "0.2"),
         start_deposit_cns=cns("5000"), end_deposit_cns=cns("1666.666666"),
         delta_pnl_cns=cns("-600"), funding_cns=cns("-1.2"))
    fill(at("4.0"), "106-dec", 1, "69000", "0.4")
    emit(at("6.0") + 4000, "PositionClosed", "position_closed", "106-close", account_id=106,
         perpetual_id=1, position_type=1, price_pns=price(1, "68000"),
         delta_pnl_cns=cns("-500"), funding_cns=cns("-0.4"))
    fill(at("6.0") + 4000, "106-close", 1, "68000", "0.2")
    emit(at("6.5"), "CollateralWithdrawal", "collateral_withdrawal", "withdraw-106",
         account_id=106, amount_cns=cns("12000"), balance_cns=cns("3000"))

    # Account 101: profitable ETH short round trip and a moderate BTC long.
    open_position(at("1.2"), "101-btc", 101, 1, 1, "70000", "0.5", "7000")
    open_position(at("3.2"), "101-eth", 101, 20, 2, "3500", "4", "2000")
    emit(at("5.2"), "PositionClosed", "position_closed", "101-eth-close", account_id=101,
         perpetual_id=20, position_type=2, price_pns=price(20, "3400"),
         delta_pnl_cns=cns("400"), funding_cns=0)
    fill(at("5.2"), "101-eth-close", 20, "3400", "4")

    # Recent positions.
    open_position(at("7.0") + 5000, "105-eth", 105, 20, 1, "3650", "5", "1600")
    open_position(at("6.0") + 6000, "104-mon", 104, 10, 1, "0.05", "600000", "3500")
    open_position(at("8.0"), "103-eth", 103, 20, 1, "3600", "10", "4000")
    open_position(at("9.05"), "102-btc", 102, 1, 1, "69000", "1.2", "6900")
    open_position(at("9.5"), "103-btc", 103, 1, 2, "68000", "0.8", "10000")
    open_position(at("9.8"), "105-mon", 105, 10, 2, "0.048", "1000000", "5000")
    emit(at("9.75"), "PositionLiquidated", "position_liquidated", "104-liq", account_id=104,
         perpetual_id=10, position_type=1, liq_price_pns=price(10, "0.0442"),
         liq_lot_lns=lots(10, "600000"), end_lot_lns=0, deposit_cns=0,
         delta_pnl_cns=cns("-3480"), funding_cns=0)

    # Canonical marks: earlier observations plus the as-of block.
    for days, btc_px, eth_px, mon_px in [("5.0", "69500", "3450", "0.049"), ("9.0", "68800", "3600", "0.047")]:
        mark(at(days) + 7000, 1, btc_px)
        mark(at(days) + 7000, 20, eth_px)
        mark(at(days) + 7000, 10, mon_px)
    for market, px in [(1, "66500"), (20, "3710"), (10, "0.0405")]:
        mark(AS_OF_MS, market, px)

    events.sort(key=lambda e: (e["block_number"], e["log_index"]))
    as_of_block = block_at(AS_OF_MS)
    return {
        "name": "watchlist-cohort",
        "description": "Synthetic seven-account, three-market, ten-day cohort with canonical marks, complete BTC/ETH funding publications from deployment and deliberately uncovered MON funding. Never mainnet evidence.",
        "registry_file": "fixtures/protocol/mainnet-registry.json",
        "coverage": {"chain_id": 143, "start_block": DEPLOYMENT_BLOCK, "processed_block": as_of_block},
        "as_of": {"chain_id": 143, "block_number": as_of_block,
                  "block_hash": digest(f"cohort-block-{as_of_block}"), "timestamp_ms": AS_OF_MS},
        "funding_coverage_markets": [1, 20],
        "events": events,
    }


if __name__ == "__main__":
    print(json.dumps(build(), indent=2))
