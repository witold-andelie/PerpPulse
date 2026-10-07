# Signal-first analytics, comparison and global reader verification

Observed 2026-10-07 on a macOS workstation. Base revision
`14307094f26cc8c000d771e7a93387843567c95d`. This record covers deterministic
local implementation, fixtures, HTTP mocks and CI-equivalent checks. It is not
a new mainnet run, a hosted deployment, a live Nansen request or sponsor
acceptance. Earlier mainnet evidence keeps its original implementation and
cutoff.

Toolchain: Rust 1.95.0, Node.js 22.22.2 with pinned pnpm 10.5.2, Python
3.11.9, Graphviz 14.1.5, Docker 29.4 and a disposable `postgres:16-alpine`
container on a loopback port.

## Delivered scope

| Area | Implementation | Contract |
| --- | --- | --- |
| Protocol analytics (T05) | [`analytics.rs`](../crates/perppulse/src/analytics.rs) | `protocol-analytics-v1`: 24h/7d/30d/coverage flows, coverage-proven completeness, deployment-history open interest, collateral and skew, per-window event-ID hashes |
| Global live reader (T05) | `EnvioClient::read_protocol_events`, `live::ProtocolAggregator`, `serve-envio --protocol-max-events` | Exact v3 keyset reads through the shared cutoff; incremental resume after re-verifying the last ingested event; bound, rewrite and coverage-start failures commit nothing; RPC-observed coverage start time |
| Alerts and stress (P2) | [`signals.rs`](../crates/perppulse/src/signals.rs) | `risk-signals-v1`: nine hashed rules, ranked top three, transitions carried by the live process, resolved retention; `mark-shock-stress-v1` |
| Wallet comparison (P2) | [`cohort.rs`](../crates/perppulse/src/cohort.rs) | `snapshot-cohort-v1`: normalized statistics, midrank percentiles over eligible known values, Jaccard exposure overlap |
| Nansen participant filtering (T08) | Label index in `cohort.rs`, comparison panel filter | Label groups with per-member observation-time eligibility; mock labels only |
| Dashboard | [`web/`](../web/) | Risk Pulse cards, windows, skew, alerts with filters, stress, comparison, label filter, evidence inspection |
| Demo fixture | [`watchlist-cohort.json`](../fixtures/golden/watchlist-cohort.json), [generator](../scripts/build_cohort_fixture.py) | Synthetic seven accounts, three markets, ten days; complete BTC/ETH funding from deployment; MON funding deliberately uncovered |

`/api/analytics`, `/api/signals` and `/api/comparison` serve the same reports
that are stored in each snapshot. The new snapshot fields are omitted when
null, so compact snapshots published by earlier versions keep their exact
content hash.

## Defects found and corrected in existing code

| Finding | Correction and regression evidence |
| --- | --- |
| A covered position reset before the first observed funding schedule stayed unknown even when coverage starts at Exchange deployment, where no pre-window publication can exist | Deployment coverage now proves covered resets; a market outside declared funding coverage stays unknown. `deployment_coverage_proves_resets_before_the_first_schedule` failed before the fix. The mid-history behavior of the accepted mainnet runs (coverage from block 109944714) is unchanged and still tested with a bounded-start mock |
| The HTTP connection limit checked and incremented separately, so concurrent accepts could exceed 32 handlers | One atomic reservation with release on rejection |
| `/?query` served health JSON instead of the dashboard | Static assets match the path without the query string |
| Percent-decoding pushed single bytes as characters, corrupting multi-byte UTF-8 | Bytes decoded first; unit test covers UTF-8 and a trailing partial escape |
| Browsers logged a 404 for `/favicon.ico` | Answered with 204 |
| The CLI demo printed the quality status twice | Prints processed block and quality |
| `CanonicalEvent` had no read indexes, so coverage probes and keyset pages sort the whole table | Three composite storage indexes; see [the Envio note](../envio/README.md#read-indexes-2026-10-07) |
| The positions table showed raw decimal strings | Display formatting only; the browser still calculates no financial fact |

## Reproducible results

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo test --locked` | 104 passed, one disposable PostgreSQL test ignored (85 passed before this work) |
| Disposable PostgreSQL round trip (`--ignored`) | Passed; signals, analytics and comparison survive publication; a changed `protocolEventIdsHash` at the same cutoff is rejected, and that assertion failed before the SQL guard was extended |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Passed |
| New intelligence tests | 12 passed: window totals and completeness, bounded coverage, input rejection, stale-mark partial state, signal ranking/rule hashes/evidence, threshold edges, stress, transitions, comparison, label groups, methodology alignment, routes and legacy hash |
| HTTP adapter tests | 18 passed, including three global-reader tests: deployment totals and incremental cursor, bound/legacy-profile/rewrite failure without partial commit, bounded coverage windows |
| Funding checkpoint tests | 13 passed |
| Envio `pnpm codegen`, `pnpm typecheck`, `pnpm test` | Passed; 19 handler tests after the schema index change |
| `python3 scripts/test_sdk_reference.py` | Passed |
| Publication policy `--working-tree`, `git diff --check` | Passed |
| `dot -Tsvg perppulse_opm.dot` | Passed; the tracked SVG is byte-identical to a fresh render |
| Fixture reproducibility | `python3 -I scripts/build_cohort_fixture.py` output equals the tracked fixture (now a CI step) |
| Production image | Built; runs as UID 10001; dashboard, health, analytics, signals and comparison 200; favicon 204; POST 405 |
| Browser, cohort fixture | [Record](evidence/cohort-browser-2026-10-07.json) and [screenshot](evidence/cohort-dashboard-2026-10-07.png): eight checks, no JavaScript or console errors |
| Browser, existing risk contract | [`verify_risk_browser.py` record](evidence/risk-browser-2026-10-07.json): the original seven checks pass against the new UI |

Both browser runs used installed Google Chrome through Playwright 1.63.0
because the cached bundled browser did not match that Playwright version.

## Publication and CI

Implementation `0e553ff46814db42b9b608fdbd287c1647f647b9` was pushed over SSH
and its remote SHA independently matched. Its policy run succeeded; in its
analytics run the Envio and SDK-reference jobs succeeded and the Rust job
passed every substantive step before the runner's graphviz package
installation stalled for more than 50 minutes. Commit
`39295be5f82dfcf99fe2d9e1702ea2b9ce438134` changes only the workflow, bounding
job and installation time. Its
[analytics run](https://github.com/witold-andelie/PerpPulse/actions/runs/37672517538)
succeeded in all three jobs and its
[policy run](https://github.com/witold-andelie/PerpPulse/actions/runs/37672517463)
succeeded. The [acceptance record](evidence/ci-0e553ff-2026-10-07.json) lists
job IDs and the credential-free public download hashes.

## Hand-checked cohort facts

At cutoff block 56933010 the cohort fixture yields:

- Account 102 BTC long, 1.2 at 69,000 with 6,900 collateral and proven funding
  -1.8: maintenance 3,312, funded liquidation 66,011.5, distance from mark
  66,500 equal to 0.007345 (critical).
- MON liquidated 26,520 of notional in 24 hours against 40,500 of open
  interest: 0.654814 (critical).
- Account 105 ETH long: 18,550 notional on 1,600 collateral is 11.59x, or
  0.966145 of the 12x initial limit (warning).
- Account 101 BTC total unrealized PnL -1,755 equals price PnL -1,750 plus
  funding -5.0 from five covered schedules.
- Windows: 24h volume 185,200 from three fills; 7d net collateral flow -12,000;
  coverage volume 415,550 equals the existing protocol metric.
- Stress: -5% breaches only account 102; +20% breaches account 103's BTC short.

## Limits and remaining work

- No HyperSync token, retained Envio volume, GCP credential or Nansen key was
  available or requested. The global reader, carry-forward transitions and
  Nansen filtering are verified only against mocks and fixtures.
- The global reader needs every market in coverage to be in the registry;
  unknown or excluded markets fail visibly. A deployment-history index is
  required for open interest, collateral and skew, and for complete windows
  unless the coverage start time is observed through RPC.
- Retained live events cost about 2 KB each plus a transient replay copy; the
  hard bound is 1,000,000 events.
- Signal thresholds are published product heuristics, not Perpl protocol
  rules. Snapshot crowding and comparison describe only the served accounts.
- The Envio schema index change must run in a fresh isolated runtime.
- Remaining owner-dependent items: a deployment-history mainnet index and
  browser acceptance, continuous hosting (T07), live Nansen verification
  (T08), nonzero mainnet funding anchors, final sponsor forms and submission.
