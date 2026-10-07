# Serving and evidence contracts

All serving routes are GET-only. HTTP connections have five-second read and write
timeouts, an 8 KiB request-header bound, and a maximum of 32 concurrent handlers.
Static assets are embedded in the Rust binary. Data-source failure does not fall
back to a previous successful live snapshot.

| Route | Contract |
| --- | --- |
| `/` | Embedded English web application |
| `/health` | Current source availability, mode, chain, and cutoff |
| `/api/snapshot` | One immutable snapshot for consistent multi-panel UI reads |
| `/api/protocol` | Fixture or global-reader protocol facts; HTTP 503 for a live account-only source |
| `/api/analytics` | `protocol-analytics-v1` windows, completeness proof and point-in-time state; HTTP 503 when unavailable |
| `/api/signals` | `risk-signals-v1` items, hashed rules, top three, transitions, resolved list and stress scenarios |
| `/api/comparison` | `snapshot-cohort-v1` wallet statistics, percentiles, exposure overlap and Nansen label groups |
| `/api/wallets`, `/api/wallet/<accountId>` | Canonical account facts and explicit replay eligibility |
| `/api/events` | Bounded filters: accountId, perpetualId, fromBlock, toBlock, offset, limit (1 to 1000) |
| `/api/event/<eventId>` | Percent-encoded event identity with block, transaction, and log |
| `/api/coverage` | Processed coverage, separate last-event silence, cutoff, and limitations |
| `/api/manifest` | Ordered identity hash, canonical input hash, provenance tuples, methodology hash |
| `/api/methodology` | Versioned machine-readable definitions and safety contract |
| `/api/context` | Separate optional context availability |

Optional `asOfBlock` rejects a changed block with HTTP 409. For consistency within
one block, clients should use `/api/snapshot` rather than separate route reads.
Unknown or duplicated filters are rejected. Historical bounds cannot extend
outside coverage or past the as-of cutoff. Without `--protocol-max-events`,
live data is a watchlist inspection and eligible-account replay surface and
protocol analytics return HTTP 503. With it, a bounded global reader keyset-pages
every exact `risk-hotpath-v3` event through the shared cutoff, re-verifies its
last ingested event before each incremental read and replays deterministically.
Windows are complete only when coverage starts at deployment or its
independently observed start time precedes the window; point-in-time state
requires deployment history. A coverage start change, rewritten ingested
event, bound overflow or unknown/excluded registry market fails visibly and
commits nothing. Raw global events are not mirrored into served events or the
compact database; the manifest carries `protocolEventCount` and
`protocolEventIdsHash`, which the same-cutoff quarantine and the compact
publication guard both compare.

Signals and comparison are computed once per snapshot from served facts and
are stored with it, so a compact database snapshot serves them unchanged. A
live process carries each signal's first-observed and severity-change blocks
from its previous accepted snapshot; without one, signals are baselines.

## Normalized verifier reference

This format is an adapter contract for a Perpl dex-sdk operator export, not a
claim that the SDK natively emits this JSON. Obtain the reference independently;
never construct it from the canonical values to claim external reconciliation.

```json
{
  "source": "perpl-dex-sdk",
  "chainId": 143,
  "asOfBlock": 54773080,
  "asOfBlockHash": "COPY_THE_EXACT_REFERENCE_BLOCK_HASH",
  "asOfLogIndex": null,
  "asOfTimestampMs": 1770001800000,
  "accountId": 42,
  "realizedPnl": "619.95",
  "realizedFunding": "-0.1",
  "fees": "0.3",
  "freeBalance": null
}
```

These illustrative amounts are not live provider evidence. Null or absent fields
remain unverified. Non-decimal strings are rejected. Reconciliation does not
change canonical facts. A matched scorecard proves only its supported account
totals, not the account's complete position state or executable liquidity.

## Operational boundaries

The separate [SDK reference operator](../tools/perpl-reference/README.md)
executes `SnapshotBuilder` at an independently checked historical end-of-block
header and compares selected position state with retained Envio replay. Its
`position-reconciliation-v1` contract requires an explicit nonempty market
scope, complete open/closed rows, exact finite decimal strings and the same
chain, account, block hash and timestamp. Absence outside requested markets is
not evidence. This does not change the account-total CLI contract above or
replace the live API's freshness gates. See the
[40-check mainnet result](verification-2026-10-03-sdk.md).

- A live wallet requires the exact v2 provenance tuple and history from deployment
  or its first account-creation event before position replay is eligible.
- Live marks have no verified ledger cutoff yet, so open unrealized PnL,
  mark-derived notional, liquidation price, and liquidation buffer are unavailable.
- Protocol PnL and executable-liquidity risk remain separate; no order-book
  execution estimate is inferred from mark prices.
- Nansen labels have their own observation timestamp. Context observed after a
  cutoff cannot be treated as contemporaneous evidence or alter financial facts.
- Hashes detect changes and bind replay inputs; they are not an independent proof
  of chain finality or complete ingestion. Raw provider responses are not exported.
- Compact database rows carry no raw event bodies. Stale or invalid rows return
  an error, and their absence never becomes an empty successful financial result.

The Nansen request and response fields are implemented from the official
[Address Labels contract](https://docs.nansen.ai/api/profiler/address-labels).
Attribution is displayed near optional labels. A live key, permitted endpoint
coverage, request-cost approval, and redistribution eligibility must be confirmed
before using real context in a public deployment.
