# Canonical marks and effective-dated funding observations

The v3 path adds `MarkUpdated` and `FundingSumScalingExpUpdated` to Envio and
preserves each funding event's effective block and overwrite flag. Rust reads
eligible marks and funding observations at the same immutable cutoff as account
history. Marks now drive the price component of position PnL and carry a source
event ID through the API and frontend. Funding observations show pending versus
effective schedules, while position funding amounts and actual liquidation
remain unavailable until checkpoint reconstruction is proven.

This is local implementation acceptance, not a new v3 mainnet operating proof.
The previously accepted v2 archive/runtime remains intact. No cloud resource,
paid call, wallet, order or custody logic was added.

## Source contract

Base publication: `50f56fbf8d5c3ebd86f3df4672cf51e238b6202f`.
The new exact provenance tuple is:

| Field | Value |
| --- | --- |
| Schema | canonical-event-v5 |
| Handler | envio-handlers-v5 |
| Classifier | exchange-classifier-v4 |
| Profile | risk-hotpath-v3 |
| LF-normalized ABI SHA-256 | 8858f1c8a42836c58459ec37a89359deb015b23e0840c7f21efddf1c3b7315e6 |

The two ABI events are extracted from the same MIT-licensed SDK revision
`dbb37c59f6aef03e38d0787eb9c968f59f652617`; source attribution and fingerprints
are in [ABI provenance](../envio/abis/SOURCE.md). Existing v2 tuples remain
eligible for lifecycle replay, but cannot establish mark coverage. New event
names cannot borrow an older provenance tuple. Initialize v3 in a separate
database/network, preserving account birth or deployment coverage; never
relabel the retained v2 index or run destructive migrations against it.

## Deterministic data flow

For each covered open market, the adapter reads one latest MarkUpdated at or
before the shared block/log cutoff and pages funding/scale observations inside
retained coverage. It validates market and account subjects, ABI kind, Exchange,
version tuple, block/hash/time, native ranges, page limits, cursor advancement,
and final coverage nonregression. Missing marks produce explicit unavailable
price PnL; stale or inconsistent marks fail. Oracle prices, REST observations
and SDK reference marks never substitute for canonical events.

Market inputs are cached within one snapshot, deduplicated with account events,
and replayed through the same canonical ledger. Conflicting facts with the same
identity fail. The manifest covers the combined canonical inputs and the mark
map. No second persistent position ledger is introduced. Fixture input also
derives marks from canonical events and rejects conflicting manual market state.

Funding schedules preserve signed int48 payment/sum values in their original
native units, plus effective block, overwrite flag and scaling events. A future
publication remains pending until its effective block. Replacement must be
authorized and preserve its prior cumulative base; target regression, overlap
and unscaled cumulative discontinuity fail. A scale change remains visible
instead of treating differently scaled sums as comparable collateral values.

The pinned [SDK state engine](https://github.com/PerplFoundation/dex-sdk/blob/dbb37c59f6aef03e38d0787eb9c968f59f652617/crates/sdk/src/state/perpetual.rs)
describes effective-dated funding and pre-event position sizes. Official
[funding documentation](https://docs.perpl.xyz/exchange/funding) describes
virtual payments and position funding checkpoints. This implementation records
the schedule; it does not yet reconstruct those position checkpoints, establish
coverage before the first observed schedule or calculate position premium PnL.
Its API retains `positionFundingAmount: null`, and actual equity/liquidation
stay null. Raw sums are never assumed to be collateral units.

## Reproducible verification

```powershell
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --manifest-path tools/perpl-reference/Cargo.toml --locked
cargo clippy --manifest-path tools/perpl-reference/Cargo.toml --locked --all-targets -- -D warnings
py -3 scripts/test_sdk_reference.py
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
```

Local results: 71 root Rust tests passed, one PostgreSQL test ignored locally;
two SDK unit tests and two Python gate tests passed. A separate Linux container
using Node 22.23.3, pnpm 10.5.2 and Envio 2.32.6 passed code generation, frozen
installation, type checking and all 19 Envio tests. The root and nested
lockfiles are unchanged. CI independently runs fresh PostgreSQL and DOT checks.

Implementation `9e0a3673c8bbfc8c5ccc4a34bdacab4c5ff60269` was pushed and its
remote main SHA independently matched. [Analytics CI](https://github.com/witold-andelie/PerpPulse/actions/runs/37117208681)
passed Rust/fresh PostgreSQL/DOT, Envio and SDK jobs;
[repository policy](https://github.com/witold-andelie/PerpPulse/actions/runs/37117208715)
also passed. Exact run/job identities are recorded in
[CI acceptance](evidence/ci-9e0a367-2026-10-03.json).

New regressions prove mark-driven price PnL and source IDs, pending funding,
overwrite rules, native ranges, historical causality, missing marks, stale
prices, wrong subjects, old provenance, malformed booleans, lookahead,
regressing coverage, duplicate scope and bounded acquisition failure.

The [browser record](evidence/market-browser-v3-2026-10-03.json) and
[inspected screenshot](evidence/market-browser-v3-2026-10-03.png) are explicitly
synthetic. They verify the source-linked Mark evidence button, pending funding
block 54773040, separated price PnL/unknown funding/liquidation, rejected writes
and cleared rows/disabled export after a source failure. Reproduce with:

```powershell
cargo run --locked -- serve fixtures/golden/canonical-market-inputs.json --bind 127.0.0.1:18085
```

In a separate terminal, choose unused output paths:

```powershell
py -3 scripts/verify_risk_browser.py --expect-mark-event --expect-pending-funding-block 54773040 --fixture fixtures/golden/canonical-market-inputs.json --output market-browser.json --screenshot market-browser.png
```

## Pending mainnet acceptance

The local isolated runtime is now prepared from the exact implementation SHA.
It uses a separate v3 network, volume and PostgreSQL/Hasura ports, and preserves
start block 109944714 covering the selected account births. Source verification
checked all 18 Git-exported Envio files; quick-config reuse, code generation,
fresh database migration, local health and synthetic hidden-input transport
passed. The actual empty Hasura schema accepted all three queries: latest mark,
bounded ascending funding/scale and the new typed entity fields. No provider
token, provider ingestion or simulated canonical row was used during this
preparation. [Preparation evidence](evidence/market-v3-preparation-2026-10-03.json)
records its limits.

The local owner entry point is `.scratch/start-live-indexer-v3.ps1` (ignored
operational state). Its `-CheckOnly` and `-SelfTest` modes need no token. The
normal launch accepts an Envio HyperSync token through masked terminal input,
passes it through stdin, redacts output and retains it only in process memory.
Its authenticated ingestion window is capped at thirty minutes. It resumes
the source/config-bound initialized database without a restart/reset migration.
After owner input, record fresh
coverage/head, source hashes, one shared cutoff, MarkUpdated provenance and
independent SDK mark/time/price-PnL comparisons. Published future schedules must
remain pending and failures must stay visible. The retained v2 archive does not
contain MarkUpdated and cannot close this acceptance. Full position funding
replay, global analytics, continuous hosting and live Nansen remain open.
