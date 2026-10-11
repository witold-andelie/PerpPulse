# Nansen withdrawal and repository audit

Observed 2026-10-11 on a macOS workstation. Base revision
`376abc91efe6eca6409f818b8b03f5a8c45e63bf`. This record covers an owner
decision, a whole-repository review and local checks. It is not a new mainnet
run, a hosted deployment or sponsor acceptance.

## Owner decision: no Nansen

The owner withdrew Nansen from PerpPulse. No live Nansen request was ever made
by this project; the removed code had been verified only against local mocks.

| Removed | Replacement or effect |
| --- | --- |
| `context.rs` common-label adapter and its two unit tests | None; no third-party enrichment or labeling provider is used |
| `serve-envio --nansen-max-requests` and `NANSEN_API_KEY` | None; `.env.example` no longer lists the key |
| Snapshot `context` field and `/api/context` | New snapshots omit the field and the route returns 404. The field is still accepted on read so compact snapshots published earlier keep their content hash; a test covers both cases |
| Comparison label index and dashboard label filter | `snapshot-cohort-v2`: the same statistics, percentiles and overlap, without labels |
| Methodology `perppulse-methodology-v8` | `perppulse-methodology-v9`: `contextSource` states that no provider is used and the comparison section records the v2 change |

Current documentation, the OPM model and the dashboard were updated. Dated
verification records keep their original content; the 2026-10-07 record links
here. [`nansen-bounty.md`](nansen-bounty.md) now records the withdrawal so
earlier links stay valid.

## Defects found and corrected

| Finding | Correction and regression evidence |
| --- | --- |
| Ledger replay accepted two different events at the same block and log position (for example events from two forks), and events of one block carrying different block hashes. The order check rejected only strictly earlier positions | Replay now rejects a repeated block/log position and a block hash that differs within one block. `repeated_block_log_positions_and_mixed_block_hashes_fail_closed` failed against the previous ledger and passes now. Three existing lifecycle tests had placed synthetic events on a log index already used by the fixture's maker fill; they now use a free index |
| Fixture protocol metrics added decimals and native fee integers without overflow checks, so an extreme input would panic instead of failing visibly | Checked addition with explicit overflow errors, matching `protocol-analytics-v1` |
| A ledger error path called `unwrap()` while formatting its message | The event identity is resolved before the lookup |
| Resolved-signal retention subtracted block numbers without a guard | Saturating subtraction; inputs are ordered, so behavior is unchanged |
| The exposure overlap table showed raw ratios such as `0.50` next to `1` | Display formatting only |

## Reviewed without change

These are design limits or risks for the owner to decide, not silent errors.
The first item was decided afterwards; see the follow-up below.

- Live wallet accounting rejected the whole refresh when an open position's
  latest canonical mark was 60 seconds or more older than the cutoff, while
  protocol analytics only marked that market's open interest unavailable. A
  quiet market could therefore take the live dashboard down until a fresh mark
  arrived.
- Three synthetic golden fixtures declare an as-of block hash that differs from
  the hash on their own events in that block. The live Envio readers enforce
  the match; replay does not, so fixture manifests and evidence stay stable.
- The live loop holds its state write lock while publishing to PostgreSQL, so
  dashboard reads can wait for the 10-second connect and 5-second statement
  timeouts when the database is slow.
- CI validates the DOT source but cannot byte-compare the SVG because its
  Graphviz version differs from the local one; synchronization relies on the
  local render recorded below.
- The global reader replays retained deployment history on every refresh; the
  1,000,000-event bound stays the documented limit.

Also reviewed: event validation, funding schedules and checkpoints, accounting
and liquidation prices, quality gates, analytics windows and state, signal
rules and stress, comparison, the Envio coverage, account and protocol readers,
publication SQL, the HTTP server, the dashboard script, Envio handlers, the
container build and both workflows.

## Reproducible results

| Check | Result |
| --- | --- |
| `cargo fmt --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings` | Passed |
| `cargo test --locked` | 103 passed, one disposable PostgreSQL test ignored (102 after removing the adapter tests, plus one new regression test) |
| Disposable PostgreSQL round trip | Not run locally because Docker was paused on this workstation; the CI Rust job runs it |
| Envio `pnpm typecheck`, `pnpm test` | Passed; schema and handlers unchanged |
| `python3 scripts/test_sdk_reference.py` | Passed |
| Publication policy `--working-tree`, `git diff --check` | Passed |
| Cohort fixture reproducibility | Generator output equals the tracked fixture |
| `dot -Tsvg perppulse_opm.dot` | Passed; the tracked SVG is byte-identical to a fresh render with Graphviz 14.1.5 |
| Relative documentation links | No broken file or anchor links |
| Fixture service smoke | `/api/context` 404; `/api/comparison` is `snapshot-cohort-v2` without labels; `/api/methodology` is v9; snapshots carry no `context` |
| Browser, cohort fixture (installed Chrome, Playwright 1.63.0) | Signals, windows, stress, comparison and evidence navigation render; no JavaScript or console errors |
| Browser, `verify_risk_browser.py` | The original seven checks pass |

## Follow-up owner decision: stale live marks degrade

After reviewing the first item above, the owner chose degradation. Under
`perppulse-methodology-v10`:

- When marks are not required (live wallet accounting), a mark at or beyond the
  60-second limit makes only that position's mark, price PnL, notional and
  mark-derived risk unavailable. The position carries a warning with the mark's
  age, the wallet quality is `marks-unavailable`, and the
  `risk-input-unavailable` warning signal lists that reason. Other positions
  and wallets are served.
- Integrity violations are unchanged and still fail visibly: future block or
  timestamp, same-block hash or log mismatch, non-positive or out-of-range
  prices and duplicate marks, including when the mark is also stale.
- Fixture protocol metrics still require eligible marks, and protocol analytics
  keep their existing per-market `mark-unavailable` state.
- The live manifest keeps hashing the stale mark as a canonical input.

| Check | Result |
| --- | --- |
| `stale_v3_mark_degrades_its_position_instead_of_failing_the_snapshot` | Failed against the previous code (the snapshot refresh errored) and passes now; it replaces the former live "stale" rejection case |
| Extended `marks_reject_staleness_nonpositive_values_duplicates_and_log_lookahead` | Required marks still reject staleness; non-required marks degrade staleness but reject a stale non-positive mark and a future mark. Failed against the previous code |
| `cargo test --locked` | 104 passed, one disposable PostgreSQL test ignored locally |
| Format, Clippy, policy, `git diff --check`, documentation links, browser checks | Passed; see the publication commit's CI for the PostgreSQL round trip |
