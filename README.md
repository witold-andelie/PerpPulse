# PerpPulse

Real-time protocol-to-wallet risk intelligence for perpetual markets on Monad.

> Public-release configuration (2026-10-12): [Render](render.yaml) serves a
> clearly labeled historical Envio window, blocks 89,200,000–89,210,000,
> containing 49,204 real Monad events. It is not a current market feed.
> Six markets and four covered accounts drive protocol flows, risk signals,
> wallet comparisons, stress scenarios and source-event inspection.
> [Deployment and verification](docs/render-deployment.md) describe the
> pinned historical registry, SDK checks and the free hosting limits.

PerpPulse turns Perpl market state and Envio-indexed onchain events into a small set of traceable risk signals. A judge or trader can move from a protocol-level anomaly to the affected market, wallet, and source event without losing the selected time or as-of context.

> 2026-10-11 update: the owner withdrew Nansen from the project. The optional
> label adapter, `--nansen-max-requests`, `/api/context` and the comparison
> label filter are removed; the methodology is now v9 and wallet comparison is
> `snapshot-cohort-v2` (same statistics, percentiles and overlap, no labels).
> The Nansen bounty is not pursued. Methodology v10 then degrades a stale live
> mark to an unavailable position fact instead of failing the whole refresh.
> Best Use of Envio is now the only sponsor bounty entered; the Perpl bounty is
> not entered.
> See the [audit record](docs/verification-2026-10-11-audit.md).

> 2026-10-07 update: the dashboard is now signal-first. Rule-hashed risk
> signals (funded liquidation distance, collateral drawdown, leverage
> utilization, skew/crowding, liquidation activity, missing-input signals)
> rank the top three at one cutoff, with mark-shock stress scenarios.
> `protocol-analytics-v1` adds 24-hour, 7-day, 30-day and covered-range flows
> whose completeness is proven from coverage, plus point-in-time open
> interest, collateral and long/short skew from deployment history. A snapshot
> wallet comparison adds percentiles and exposure overlap.
> `serve-envio --protocol-max-events` adds an incremental, bounded
> global Envio reader. Coverage from Exchange deployment now proves funding
> resets before the first observed schedule. These additions are verified with
> fixtures, mocks and local CI-equivalent checks only; see the
> [verification record](docs/verification-2026-10-07-analytics.md). No new
> mainnet run or cloud deployment is claimed.

> Status: canonical ledger, coverage-bounded Envio-to-Rust adapter,
> golden-fixture web demo, read-only fixture and live-account APIs, compact PostgreSQL serving,
> hashed evidence and reconciliation scorecards implemented.
> The isolated `risk-hotpath-v2` index resumed on 2026-10-02 with 69,317 real
> Monad events and a one-block independently observed lag. Four covered accounts
> passed actual browser checks; an [80-second operating recording](docs/demo/README.md)
> shows the data flow. [Verification and limits](docs/verification-2026-10-03-envio.md)
> separate these results and the earlier 23-field ABI diagnostic from pending work.
> The [official SDK execution](docs/verification-2026-10-03-sdk.md) now matches 40
> position fields at the identical end-of-block cutoff for four covered accounts
> and three selected markets. Lifetime totals and public live
> deployment remain unverified.
> [Risk formula verification](docs/verification-2026-10-03-risk.md) matches six
> selected historical SDK comparisons. Maintenance uses effective entry notional;
> price PnL is separate from total PnL. Unknown funding keeps actual risk null.
> The [v3 mainnet market proof](docs/verification-2026-10-03-market-mainnet.md)
> verifies canonical marks and price PnL in the actual four-account dashboard.
> Same-cutoff SDK execution matches 80 position checks, 12 native mark/time
> checks and 16 risk diagnostics across six selected markets. Real published
> funding remains pending until its effective block.
> [Covered funding verification](docs/verification-2026-10-03-funding.md) adds
> 15 matching SDK funding/risk checks for three positions with proven zero
> unsettled funding. The older BTC position remains unknown. Nonzero mainnet
> checkpoint reconstruction, global analytics and continuous public hosting
> remain pending; the retained database does not certify current freshness.
> Licensed Apache-2.0. GCP foundation was provisioned
> in `europe-west3` on project `project-5e761e8c-65aa-4033-8cb`, ceiling EUR 350;
> its current state has not been rechecked.
> No Cloud Run worker is deployed yet. Public repository:
> `https://github.com/witold-andelie/PerpPulse`.

## Competition fit

- Primary track: Onchain Finance & Trading
- Sponsor bounty: Best Use of Envio (the only one entered)

[Envio](docs/envio-bounty.md) requirements are mapped to remaining live
evidence. On 2026-10-11 the owner decided not to enter the
[Perpl bounty](docs/perpl-bounty.md), whose supplied task requires trading
activity outside the read-only scope, and withdrew the
[Nansen bounty](docs/nansen-bounty.md); no Nansen data or API is used. Perpl
stays the analyzed protocol and its SDK stays the reconciliation reference.

The product is read-only. It does not place orders, request private keys, or present AI-generated numbers as financial facts.

## Target judge-facing product path

The following is the intended full product scope. The web application now
implements the signal-first Risk Pulse, window flows and state, alerts and
stress scenarios, account drill-down, snapshot wallet comparison, and event
evidence. Live serving implements bounded account
watchlists and an optional bounded global reader. Mainnet global history,
continuously hosted operation and push delivery of alerts remain pending.

The latest Q16 reader correction and operating artifacts in `4b9135b` passed
[Rust, PostgreSQL, Envio and DOT verification](https://github.com/witold-andelie/PerpPulse/actions/runs/37076854913)
and [publication policy](https://github.com/witold-andelie/PerpPulse/actions/runs/37076854941).
CI and a bounded local mainnet run do not establish a continuously available
public demo. Missing or stale inputs, global totals and unverified lifetime
accounting remain visible limits. Selected mainnet mark/position verification is
recorded in the newer v3 proof above.

1. Start on a signal-first protocol Risk Pulse with the top three changes and visible freshness.
2. Inspect volume, open interest, TVL, fees and revenue, active users, flows, skew, liquidations, and funding over 24-hour, 7-day, 30-day, and historical windows.
3. Drill into the contributing market and wallet while preserving the as-of block, time range, and filters.
4. Inspect current positions, lifecycle history, realized and unrealized PnL, liquidation distance, and reproducible performance statistics.
5. Open the block, transaction, log, source, metric version, and rule hash behind any signal.
6. Compare saved wallets with normalized statistics, cohort percentiles, and exposure overlap.

## Data contracts

PerpPulse has explicit source-of-truth boundaries:

- Envio and Monad events are canonical for global account and position lifecycle facts.
- Perpl public REST and market WebSocket data provide low-latency market state.
- Perpl `dex-sdk` snapshots and traces are reconciliation and test instruments, not a second ledger.
- No third-party labeling or enrichment provider is used. Wallet comparison ranks only facts already served from the canonical ledger.
- AI may explain a deterministic signal, but it may not calculate or replace prices, PnL, liquidation levels, or risk facts.

The event identity is `(chain_id, block_hash, tx_hash, log_index)`. Derived records retain source, schema, metric, model, and rule versions plus an as-of cutoff.
HyperIndex `_meta.progressBlock` is the processed-chain watermark; the latest
canonical event is only the latest matched Exchange log. Keeping those facts
separate distinguishes a healthy quiet market from a stalled indexer.

## Implementation

The first vertical slice is a fixture-driven protocol-to-wallet-to-event path:

- **Rust** (`crates/perppulse`) is the canonical ledger, point-in-time accounting, protocol metrics, quality gates, and demo CLI. It matches the official Perpl dex-sdk language, keeps financial math in explicit decimal scales, and is fast enough for deterministic replay.
- **TypeScript** (`envio/`) is the Envio HyperIndex indexer. HyperIndex handlers must be TypeScript; they write canonical events only and never compute PnL.
- **SQLite** stands in for PostgreSQL locally. The event table shape is the same logical contract Envio will materialize.

Envio's default `risk-hotpath-v3` profile scans from Exchange deployment and
includes the low-frequency credit, transfer, and no-payment unwind transitions
required for position reconstruction, plus canonical marks and funding scaling
observations. It omits intent-only order requests and
subjectless duplicate taker-fill logs. This keeps judge startup and the serving
database small without dropping inputs used by position, PnL, or protocol
metrics. Because a taker-fill balance cannot be attributed to an account from
that log alone, arbitrary-wallet free-balance history remains explicitly
degraded rather than being presented as complete.

Verified Perpl mainnet facts live in [`docs/protocol-registry.md`](docs/protocol-registry.md). The [September registry](fixtures/protocol/mainnet-registry.json) remains frozen for fixtures; the [dated live registry](fixtures/protocol/mainnet-registry-2026-10-03.json) additionally covers independently verified VVV (70). Exchange proxy: `0x34B6552d57a35a1D042CcAe1951BD1C370112a6F` on Monad chain 143, start block `54773010`.

### Setup

Rust 1.95 and Python 3.12+ are used for the ledger tests and publication policy check. Node.js 22+ and pinned pnpm 10.5.2 are required only to generate and run the Envio indexer.

```powershell
cargo test
cargo run -p perppulse -- demo fixtures/golden/open-increase-reduce-close.json
cargo run -p perppulse -- serve fixtures/golden/open-increase-reduce-close.json --bind 127.0.0.1:8081
cargo run -p perppulse -- envio-account 5238 --inspect-only
python scripts/check_repository_policy.py --tracked
```

### Google Cloud foundation

Owner-authorized project `project-5e761e8c-65aa-4033-8cb` in `europe-west3`
(Frankfurt). Cloud SQL `perppulse-pg` was previously observed `RUNNABLE` there;
current resource state has not been rechecked. The script creates Artifact Registry,
evidence storage, a runtime service account, Secret Manager, a EUR 350 budget,
and Cloud SQL `db-g1-small`. It does not deploy Cloud Run.

```powershell
.\deploy\foundation.ps1
.\deploy\stop.ps1
```

The database password is stored in Secret Manager as `perppulse-db-password` and is not printed.

Envio local indexer (optional, needs Docker Desktop, WSL integration, and an
Envio API token that must not be committed):

These commands assume a new local index. Preserve any existing v1 database:
prepare v2 with a separate generated directory, Compose project, explicitly
unique Docker network and loopback ports before starting it. The generated
Compose template names its network explicitly, so a project-name override alone
does not isolate database discovery. See [the Envio run contract](envio/README.md).

```powershell
wsl -d Ubuntu-20.04
cd /mnt/d/AI_Models/hackson/monad/envio
pnpm install --frozen-lockfile
pnpm codegen
read -rsp "Envio API token: " ENVIO_API_TOKEN; echo
export ENVIO_API_TOKEN
pnpm dev
```

`pnpm dev` generates and then reuses a judge-safe 50,000-block smoke window.
Use `pnpm dev:full` for deployment-block history. Refreshing the smoke start
block is explicit (`pnpm quick:refresh`) because changing it rebuilds Envio's
local persisted state. The public judging deployment must be pre-indexed rather
than performing historical backfill on page startup.

From a second WSL shell, `pnpm coverage:check` compares transactional processed
coverage with the current HyperSync head and reports active, quiet, lagging,
quarantined, or unknown state. See [`envio/README.md`](envio/README.md) for the
full run contract.

### Demo path

`cargo run -p perppulse -- demo <fixture>` prints:

1. Protocol Risk Pulse (volume, open interest, TVL, fees, liquidations, freshness)
2. Wallet drill-down (isolated positions, realized facts, as-of unrealized PnL)
3. Event evidence for the last canonical log (block, transaction, log index)

Golden fixtures cover open → increase → partial reduce → close, an open position with mark, liquidation, and a stale as-of failure.
The synthetic `watchlist-cohort` fixture adds seven accounts, three markets and
ten days of history with canonical marks, complete BTC/ETH funding publications
from deployment and deliberately uncovered MON funding. It is generated
deterministically by [`scripts/build_cohort_fixture.py`](scripts/build_cohort_fixture.py)
and is the container's default demo:

```powershell
cargo run -p perppulse -- serve fixtures/golden/watchlist-cohort.json --bind 127.0.0.1:8081
```

`envio-account` keyset-pages one account through Envio GraphQL, binds the result
to `_meta` coverage, verifies every payload subject and provenance tuple, and
rechecks the stable as-of event after paging. A v1 database is inspection-only;
financial position replay is enabled only for v2 coverage that starts at
deployment or includes the account-creation event.

`serve` exposes the same fixture pulse as a read-only web and JSON surface. The
HTTP transport uses the standard library. It serves `GET /health`, `/api/protocol`,
`/api/analytics`, `/api/signals`, `/api/comparison`,
`/api/wallets`, `/api/wallet/<accountId>`, `/api/events`,
`/api/event/<eventId>`, and `/api/coverage`. Unknown paths return 404,
non-GET methods return 405, and every range remains bounded by the fixture
`startBlock`. The [serving contract](docs/serving-and-evidence.md) and
[data dictionary](docs/data-dictionary.md) define every window, rule and
statistic.

Open `http://127.0.0.1:8081` for the embedded web application. It shows the
fixture badge, the top three rule-hashed signals, processed coverage, protocol
metrics, window flows with completeness, market skew, alerts and stress
scenarios, account positions, wallet comparison, event evidence, and a
downloadable range manifest. Financial values are decimal
strings; the browser does not calculate PnL. Wallet, market, and block filters
operate on one immutable snapshot obtained through `/api/snapshot`.

### Live serving and compact PostgreSQL

```powershell
cargo run -p perppulse -- serve-envio --accounts 5238
cargo run --locked -p perppulse -- serve-envio --accounts 5382,5383,5384,5385 --registry fixtures/protocol/mainnet-registry-2026-10-03.json
cargo run -p perppulse -- evidence fixtures/golden/open-position-as-of.json
```

Live serving reads the same Envio adapter on a bounded watchlist (1 to 20
accounts, at most 100,000 combined events). It checks an independent Monad RPC
chain identity and head, shares one event cutoff across accounts, and replaces
the entire snapshot atomically. Source failures return 503; an observation older
than 90 seconds or processed coverage stalled for 120 seconds is unavailable.
Coverage regression or changed facts at the same cutoff quarantine the process.
Restart only after the source has been reconciled.

A watchlist cannot prove global protocol totals, so without the global reader
the live protocol and analytics endpoints return 503. Add
`--protocol-max-events 250000` to read every canonical v3 event in coverage
incrementally (at most 1,000,000; budget about 2 KB per retained event plus a
transient replay copy, so 250,000 events need roughly 1 GB at peak). Windows are complete only when coverage
starts at deployment or its RPC-observed start time precedes the window; open
interest, collateral and skew require deployment history (`pnpm dev:full`).
Every market in coverage must be in the registry; an unknown or excluded
market fails visibly instead of being dropped from totals. Each account exposes its replay eligibility. Eligible accounts have
deterministic realized facts and position state; incomplete history blocks
position replay. Canonical v3 MarkUpdated inputs drive price PnL and source
inspection. Free balance remains null; funded equity/liquidation require a
covered position funding checkpoint and stay null when it is unknown;
legacy profiles retain unavailable marks. An old v1 index is inspection-only.

### Public market input inspection

```powershell
cargo run --locked -p perppulse -- inspect-context --output .scratch/public-market-observation.json
```

This read-only command fetches official Perpl public context without credentials,
checks chain, Exchange and collateral identity, validates market scales and
margin fractions, and exports selected public fields plus a deterministic hash.
It identifies newly listed or changed markets and stale composite state.
Output paths must be new; prior evidence is never overwritten. The
[2026-10-02 observation](docs/evidence/perpl-context-2026-10-02.json) identifies
VVV (70), NEAR (100), and UNI (110) beyond the September fixture registry.

The observation does not update the canonical registry or supply accounting
marks. Composite REST timestamps do not prove mark-update freshness or the
ledger's exact log cutoff. Accounting requires positive finite marks younger
than 60 seconds; duplicate, future, non-positive and ambiguous same-block marks
are rejected. Since methodology v10, a live mark 60 seconds or older makes only
that position's mark-derived facts unavailable, with a warning and a
risk-input-unavailable signal, instead of failing the whole refresh. The
60-second application limit is conservative and still requires
comparison with the live protocol's exact-cutoff limits. Same-block log cutoffs
require the matching block hash and a mark log at or before the selected cutoff.
The September registry remains a frozen golden-fixture input. The dated registry
adds SDK-verified VVV metadata; six selected markets have same-cutoff onchain
mark comparisons in the [v3 proof](docs/verification-2026-10-03-market-mainnet.md).
Unknown markets still fail closed.

The compact PostgreSQL publisher stores one snapshot per source, capped at 1 MB,
with input and content hashes. It does not copy the raw event stream. Set
`PERPPULSE_DATABASE_URL` as process environment using a local PostgreSQL or
loopback [Cloud SQL Auth Proxy](https://cloud.google.com/sql/docs/postgres/sql-proxy)
connection; credentials must never be passed as CLI arguments or committed.

```powershell
cargo run -p perppulse -- publish fixtures/golden/open-position-as-of.json --source fixture-demo
cargo run -p perppulse -- serve-database --source fixture-demo --max-age-seconds 90
cargo run -p perppulse -- serve-envio --accounts 5238 --publish-database
```

Database reads reject absent, stale, or hash-inconsistent snapshots. Event bodies
are explicitly unavailable in compact serving mode; resolve manifest and
position event IDs against Envio. The Cloud SQL migration and cloud deployment
have not been executed by this implementation session.

### Evidence and reconciliation

`/api/manifest` provides stable hashes of ordered event IDs, canonical inputs,
and the versioned [`methodology.json`](docs/methodology.json). CLI evidence export
can write a new file with `--output`; existing evidence is never overwritten.
`--reference <json>` compares a normalized `perpl-dex-sdk` account reference only
when chain, block hash, log cutoff, and timestamp exactly match. Decimal-string
account totals report matched, mismatch, or unverified per field; missing values
do not count as matches. Position state and executable liquidity are outside this
scorecard's scope. A mismatch exits with an error. See the reference contract in
[`docs/serving-and-evidence.md`](docs/serving-and-evidence.md).

The isolated [SDK reference operator](tools/perpl-reference/README.md) executes
the pinned official `SnapshotBuilder` against a fixed historical block behind a
read-only RPC request gate. It replays retained Envio events at the same
end-of-block header and compares position status, size, deposit, direction and
effective entry. Every selected market requires explicit open/closed state;
missing or incomplete references fail. This archival path never replaces the
live API's freshness checks. Its selected mainnet evidence is in
[`docs/verification-2026-10-03-sdk.md`](docs/verification-2026-10-03-sdk.md).

### Container and checks

```powershell
docker build -t perppulse:local .
docker run --rm -p 127.0.0.1:8081:8080 perppulse:local
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
python scripts/check_repository_policy.py --working-tree
```

The container runs as an unprivileged user and defaults to an explicitly
synthetic fixture demo. [`deploy/application.ps1`](deploy/application.ps1)
prepares an immutable-image Cloud Run smoke deployment in the confirmed project
and region, with zero minimum and one maximum instance. Its default is a dry run;
execution requires explicit owner approval for the billable deployment. It keeps
the service authenticated. It is not a mainnet deployment proof.

GitHub Actions runs Rust, PostgreSQL, Graphviz, publication policy, Envio code
generation, typechecking, and handler tests. Local reproducible evidence is
recorded in [`docs/verification-2026-10-01.md`](docs/verification-2026-10-01.md)
and [`docs/verification-2026-10-02.md`](docs/verification-2026-10-02.md).
The [real Envio operating recording](docs/demo/README.md) is available with
English captions. The earlier [demo script](docs/demo-script.md) describes the
synthetic full-product path; it is not evidence of live global analytics.

## Architecture

The framework covers five planes:

- Data: historical backfill, live capture, point-in-time market state, gap and reorg detection, quarantine, and deterministic replay.
- Intelligence: versioned metric definitions, one canonical position lifecycle ledger, point-in-time accounting, deterministic stress scenarios, and fail-closed signal quality gates.
- Experience: protocol, market, wallet, comparison, event evidence, and alert views connected by stable navigation context.
- Evidence: golden fixtures, invariant checks, source reconciliation, freshness measurements, sponsor ablation, and append-only run manifests.
- Deployment: a planned, budget-controlled GCP runtime derived from a proven local reference architecture.

Architecture artifacts:

- [`perppulse_opm.dot`](perppulse_opm.dot): OPM-inspired Graphviz source.
- [`perppulse_opm.svg`](perppulse_opm.svg): rendered architecture map.
- [`docs/architecture-research.md`](docs/architecture-research.md): public-project and sponsor-document research translated into design changes.
- [`docs/gcp-deployment-plan.md`](docs/gcp-deployment-plan.md): useful Google Cloud credit plan and spending guardrails.
- [`docs/grok-operator-guide.md`](docs/grok-operator-guide.md): bounded manual workflows for read-only review, web research, and isolated implementation.
- [`docs/protocol-registry.md`](docs/protocol-registry.md): verified Perpl addresses, decimals, markets, and event list.
- [`docs/data-dictionary.md`](docs/data-dictionary.md): canonical event kinds and derived metric definitions.

Render the diagram with Graphviz:

```powershell
dot -Tsvg perppulse_opm.dot -o perppulse_opm.svg
```

## Repository policy

Public artifacts must contain English content only. `PROGRESS.md` is a local operating ledger and is intentionally ignored by Git. A repository check rejects staged progress ledgers and files containing CJK scripts.

Run the check manually:

```powershell
python scripts/check_repository_policy.py --tracked
python scripts/check_repository_policy.py --staged
```

## Submission compliance

The Metropolis public v3 rules accept Monad mainnet or testnet integration and
require public source, setup, licensing, attribution, build-window history,
documentation, and a three-minute public operating demo. PerpPulse continues
to target mainnet. The owner supplied the [Envio bounty requirements](docs/envio-bounty.md),
including meaningful real onchain data use, a cloud-hosted or self-hosted pipeline,
and an optional video of at most two minutes. Live delivery and sponsor-specific
eligibility remain unverified; see the
[source-linked submission checklist](docs/submission-checklist.md).

This repository is licensed under the Apache License 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

## Attribution and AI disclosure

The architecture is informed by the public projects and official documentation listed in [`docs/architecture-research.md`](docs/architecture-research.md). Application code is independently implemented. Exchange event definitions and effective-entry Q16 semantics come from the MIT-licensed Perpl dex-sdk; see [`envio/abis/SOURCE.md`](envio/abis/SOURCE.md), [`docs/data-dictionary.md`](docs/data-dictionary.md), and the [retained upstream license](LICENSES/Perpl-dex-sdk.txt).

OpenAI Codex has been used for web research, architecture drafting, implementation,
documentation, repository setup, and verification. xAI Grok has been used for
bounded review and for implementing the first ledger slice. Anthropic Claude Code
(Claude Opus 5.5) was used on 2026-10-07 for the protocol analytics, signal,
comparison and global-reader implementation, code review and fixes,
documentation, and local verification, and on 2026-10-11 for the Nansen
removal and a repository audit with its fixes. All generated material
is independently reviewed before adoption. Deterministic code and independently
verifiable data, rather than language-model output, remain the source of analytical facts.
