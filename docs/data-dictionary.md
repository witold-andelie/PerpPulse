# Data Dictionary

Canonical facts are lifecycle events. Derived wallet and protocol numbers are
computed by the Rust ledger from those events plus an as-of market mark.

## Event identity

`(chain_id, block_hash, tx_hash, log_index)`

Replay order is `(block_number, log_index)`. Duplicate identities fail closed.
The parent hash is retained for continuity checks, and HyperIndex rollback on
reorg is enabled.

## Canonical event provenance

| Field | Meaning |
| --- | --- |
| `blockNumber`, `blockHash`, `parentHash` | Point-in-time chain location and continuity evidence |
| `txHash`, `logIndex`, `srcAddress` | Exact source-log locator |
| `timestampMs` | Chain block timestamp converted from seconds to milliseconds; never ingestion wall time |
| `abiEventName` | Exact decoded Exchange ABI event name |
| `kind` | Fail-closed PerpPulse lifecycle classification |
| `accountId`, `perpetualId`, `positionType` | Subjects present in that ABI event; absent subjects remain null |
| `payloadJson` | Stable, sorted-key, bigint-safe serialization of decoded event parameters |
| `schemaVersion`, `handlerVersion`, `classifierVersion` | Version provenance for deterministic replay and migration |
| `ingestionProfile` | Exact indexed event-set contract; mixed profiles fail closed |
| `abiFingerprint` | SHA-256 fingerprint of the indexed ABI subset |

The latest canonical event is only the last matched Exchange log. It must never
be interpreted as the indexer's processed-chain watermark.

## Coverage observations

HyperIndex `_meta.progressBlock` is the transactional processed-coverage
watermark, including scanned blocks with no matching logs. PerpPulse compares it
with the current HyperSync height and records the latest matched event separately.
This yields five explicit states: `caught_up_active`, `caught_up_quiet`,
`lagging`, `quarantined`, and `unknown`. Missing, uninitialized, inconsistent, or
suspect observations fail visibly rather than becoming an empty successful
dataset.

Every API time-range response must be bounded by the `_meta.startBlock`
coverage observation. A quick-start database may answer only ranges that it
fully covers; longer range selectors must be disabled or marked incomplete.

## Lifecycle kinds

| Kind | On-chain source | Ledger effect |
| --- | --- | --- |
| `account_created` | `AccountCreated` | Opens an account record |
| `account_liquidation_credit` | `AccountLiquidationCredit` | Retains start/end balance evidence and applies the exact resulting account balance |
| `account_to_protocol_transfer` / `protocol_to_account_transfer` | matching transfer logs | Records amount and exact resulting account balance |
| `collateral_deposit` / `collateral_withdrawal` | matching logs | Sets free account balance from `balanceCNS` |
| `position_opened` | `PositionOpened` / `V2` | Creates isolated position; `pricePNS` is entry |
| `position_increased` | `PositionIncreased` / `V2` | Size/deposit/entry from event end fields; `pricePNS` is the new entry |
| `position_decreased` | `PositionDecreased` | Partial reduce; realized PnL and funding from event integers |
| `position_closed` | `PositionClosed` | Size and deposit to zero; realized PnL and funding from event |
| `position_liquidated` | `PositionLiquidated` | Uses `liqLotLNS` and remaining `end_lot_lns` |
| `position_liquidation_credit` | `PositionLiquidationCredit` | Verifies start deposit and applies the exact resulting position deposit |
| `position_deleveraged` | `PositionDeleveraged` / `V2` | Forced size cut |
| `position_inverted` | `PositionInverted` | Side flip |
| `position_unwound` | payment and no-payment unwind variants | Closes the position while retaining FMV, payment, or amount-owed evidence |
| `collateral_increased` / `decreased` | collateral logs | Updates isolated deposit; decrease also applies the resulting entry price |
| `maker_fill` | `MakerOrderFilled` / `V2` | Protocol volume and fill fees; does not mutate positions |
| `market_funding` | `FundingEventCompleted` | Market-level funding evidence |

Realized PnL, funding, and fees are **event integers**, not recomputed by
PerpPulse. Unrealized PnL for open positions is derived:

`positionType = 0` means unspecified on some transition events observed on
mainnet. It preserves the existing side; only open and invert events must carry
an explicit long or short value.

For `PositionLiquidated`, `liqLotLNS` is the liquidated size and `posLotLNS` is
the remaining post-event size. The canonical adapter never aliases the latter
to the liquidated size.

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

## Risk hot-path scope

The default Envio ingestion profile is `risk-hotpath-v2`. It scans from the
Exchange deployment block but persists only state-changing lifecycle facts,
maker fills, funding completions, and market additions. `OrderRequest`/`V2` is
not persisted because an order intent does not prove execution or mutate the
position ledger. `TakerOrderFilled`/`V2` is not persisted because it lacks both
account and perpetual identifiers and duplicates the execution represented by
the corresponding maker fill.

The v2 event set includes liquidation credits, account/protocol transfers, and
no-payment unwind variants. This scope preserves every implemented protocol
metric, position, PnL fact, liquidation fact, and replay transition. It cannot
prove exact arbitrary-wallet free-balance history because the subjectless taker
balance cannot be attributed without the excluded request stream; that field
must be visibly degraded or reconciled from a verified baseline. A future
raw-intent archive must remain outside the hot serving database and may enrich
evidence only; it cannot overwrite canonical lifecycle facts. Live validation
on 2026-09-06 found that the four excluded event variants accounted for
approximately 99% of matched rows in the observed historical prefix.
