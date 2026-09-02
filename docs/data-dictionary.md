# Data Dictionary

Canonical facts are lifecycle events. Derived wallet and protocol numbers are
computed by the Rust ledger from those events plus an as-of market mark.

## Event identity

`(chain_id, block_hash, tx_hash, log_index)`

Replay order is `(block_number, log_index)`. Duplicate identities fail closed.

## Lifecycle kinds

| Kind | On-chain source | Ledger effect |
| --- | --- | --- |
| `account_created` | `AccountCreated` | Opens an account record |
| `collateral_deposit` / `collateral_withdrawal` | matching logs | Sets free account balance from `balanceCNS` |
| `position_opened` | `PositionOpened` / `V2` | Creates isolated position; `pricePNS` is entry |
| `position_increased` | `PositionIncreased` / `V2` | Size/deposit/entry from event end fields; `pricePNS` is the new entry |
| `position_decreased` | `PositionDecreased` | Partial reduce; realized PnL and funding from event integers |
| `position_closed` | `PositionClosed` | Size and deposit to zero; realized PnL and funding from event |
| `position_liquidated` | `PositionLiquidated` | Uses `liqLotLNS` and remaining `end_lot_lns` |
| `position_deleveraged` | `PositionDeleveraged` / `V2` | Forced size cut |
| `position_inverted` | `PositionInverted` | Side flip |
| `position_unwound` | `PositionUnwound` / `V2` | Market unwind |
| `collateral_increased` / `decreased` | collateral logs | Isolated deposit only; entry size unchanged on increase |
| `maker_fill` | `MakerOrderFilled` / `V2` | Protocol volume and fill fees; does not mutate positions |
| `taker_fill` | `TakerOrderFilled` / `V2` | Evidence only until joined to `OrderRequest` in the same transaction |
| `market_funding` | `FundingEventCompleted` | Market-level funding evidence |

Realized PnL, funding, and fees are **event integers**, not recomputed by
PerpPulse. Unrealized PnL for open positions is derived:

- long: `(mark - entry) * size`
- short: `(entry - mark) * size`

using the as-of mark. Missing marks fail closed.

## Isolated margin

Each `(chain_id, account_id, perpetual_id)` position has its own `depositCNS`.
Free account balance does not back a position.

## Liquidation math

Official criterion: liquidation when `0 < FMV <= MMR`, with `MMR = N / MMF`
([Margin](https://docs.perpl.xyz/exchange/margin.md)). PerpPulse sets
`FMV = deposit + unrealized price PnL` and solves for mark. Unrealized funding
is not included; that limitation is attached to the snapshot.

## Protocol metrics

| Metric | Definition |
| --- | --- |
| Taker volume | Sum of maker-fill notional (`price * size`) so maker and taker are not both counted |
| Open interest | Sum of open position mark notionals at as-of |
| TVL | Sum of isolated position deposits at as-of |
| Protocol fees | `insFeeCNS + protFeeCNS` on open/increase/invert |
| Liquidations | Count and notional from `PositionLiquidated` |

Windowed metrics fail if the window contains no events.
