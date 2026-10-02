# PerpPulse

Real-time protocol-to-wallet risk intelligence for perpetual markets on Monad.

PerpPulse turns Perpl market state, Envio-indexed onchain events, and Nansen wallet context into a small set of traceable risk signals. A judge or trader can move from a protocol-level anomaly to the affected market, wallet, and source event without losing the selected time or as-of context.

> Status: canonical ledger, coverage-bounded Envio-to-Rust adapter,
> golden-fixture web demo, read-only fixture and live-account APIs, compact PostgreSQL serving,
> hashed evidence, reconciliation scorecards, and optional Nansen label context implemented. The Envio `risk-hotpath-v1` index was
> verified against live Monad data with a coverage-aware judge quick start;
> the expanded `risk-hotpath-v2` profile is code-generated and tested but still
> requires a fresh live reindex. Licensed Apache-2.0. GCP foundation was provisioned
> in `europe-west3` on project `project-5e761e8c-65aa-4033-8cb`, ceiling EUR 350;
> its current state has not been rechecked.
> No Cloud Run worker is deployed yet. Public repository:
> `https://github.com/witold-andelie/PerpPulse`.

## Competition fit

- Primary track: Onchain Finance & Trading
- Perpl: analytics/risk target; an applicable read-only bounty remains unconfirmed
- Envio bounty: Best Use of Envio
- Nansen bounty: Best Use of Nansen

The owner-supplied [Perpl task](docs/perpl-bounty.md) requires trading activity
and is outside the retained read-only scope. [Envio](docs/envio-bounty.md) and
[Nansen](docs/nansen-bounty.md) requirements are mapped to remaining live evidence.

The product is read-only. It does not place orders, request private keys, or present AI-generated numbers as financial facts.

## Target judge-facing product path

The following is the intended full product scope. The current web application
implements fixture metrics, account drill-down, and event evidence. Live serving
implements bounded account watchlists; global historical analytics, comparison,
and alerts remain pending.

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
- Nansen labels, Profiler, and Smart Money data provide behavioral context and ranking features; they never overwrite canonical facts.
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

Envio's default `risk-hotpath-v2` profile scans from Exchange deployment and
includes the low-frequency credit, transfer, and no-payment unwind transitions
required for position reconstruction. It omits intent-only order requests and
subjectless duplicate taker-fill logs. This keeps judge startup and the serving
database small without dropping inputs used by position, PnL, or protocol
metrics. Because a taker-fill balance cannot be attributed to an account from
that log alone, arbitrary-wallet free-balance history remains explicitly
degraded rather than being presented as complete.

Verified Perpl mainnet facts live in [`docs/protocol-registry.md`](docs/protocol-registry.md) and [`fixtures/protocol/mainnet-registry.json`](fixtures/protocol/mainnet-registry.json). Exchange proxy: `0x34B6552d57a35a1D042CcAe1951BD1C370112a6F` on Monad chain 143, start block `54773010`.

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

`envio-account` keyset-pages one account through Envio GraphQL, binds the result
to `_meta` coverage, verifies every payload subject and provenance tuple, and
rechecks the stable as-of event after paging. A v1 database is inspection-only;
financial position replay is enabled only for v2 coverage that starts at
deployment or includes the account-creation event.

`serve` exposes the same fixture pulse as a read-only web and JSON surface. The
HTTP transport uses the standard library. It serves `GET /health`, `/api/protocol`,
`/api/wallets`, `/api/wallet/<accountId>`, `/api/events`,
`/api/event/<eventId>`, and `/api/coverage`. Unknown paths return 404,
non-GET methods return 405, and every range remains bounded by the fixture
`startBlock`.

Open `http://127.0.0.1:8081` for the embedded web application. It shows the
fixture badge, processed coverage, protocol metrics, account positions, event
evidence, and a downloadable range manifest. Financial values are decimal
strings; the browser does not calculate PnL. Wallet, market, and block filters
operate on one immutable snapshot obtained through `/api/snapshot`.

### Live serving and compact PostgreSQL

```powershell
cargo run -p perppulse -- serve-envio --accounts 5238
cargo run -p perppulse -- evidence fixtures/golden/open-position-as-of.json
```

Live serving reads the same Envio adapter on a bounded watchlist (1 to 20
accounts, at most 100,000 combined events). It checks an independent Monad RPC
chain identity and head, shares one event cutoff across accounts, and replaces
the entire snapshot atomically. Source failures return 503; an observation older
than 90 seconds or processed coverage stalled for 120 seconds is unavailable.
Coverage regression or changed facts at the same cutoff quarantine the process.
Restart only after the source has been reconciled.

A watchlist cannot prove global protocol totals, so the live protocol endpoint
returns 503. Each account exposes its replay eligibility. Eligible accounts have
deterministic realized facts and position state; incomplete history blocks
position replay. Free balance and open-position mark-derived facts remain null
until their missing inputs are proven. An old v1 index is inspection-only.

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
than 60 seconds; duplicate, future, stale, and ambiguous same-block marks are
rejected. The 60-second application limit is conservative and still requires
comparison with the live protocol's exact-cutoff limits. Same-block log cutoffs
require the matching block hash and a mark log at or before the selected cutoff.
The September registry remains a frozen golden-fixture input; live metadata
adoption and independent onchain mark verification remain pending.

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

### Evidence, reconciliation, and optional context

`/api/manifest` provides stable hashes of ordered event IDs, canonical inputs,
and the versioned [`methodology.json`](docs/methodology.json). CLI evidence export
can write a new file with `--output`; existing evidence is never overwritten.
`--reference <json>` compares a normalized `perpl-dex-sdk` account reference only
when chain, block hash, log cutoff, and timestamp exactly match. Decimal-string
account totals report matched, mismatch, or unverified per field; missing values
do not count as matches. Position state and executable liquidity are outside this
scorecard's scope. A mismatch exits with an error. See the reference contract in
[`docs/serving-and-evidence.md`](docs/serving-and-evidence.md).

Optional Nansen common labels use the official
[Address Labels endpoint](https://docs.nansen.ai/api/profiler/address-labels),
with five-minute caching, no retries or premium calls, and an explicit allowance
of 1 to 10 requests per process. By default no requests are made. After owner
approval of the billable calls, supply `NANSEN_API_KEY` as process environment
and `--nansen-max-requests <count>` to `serve-envio`. Restarts reset this local
allowance; it is not an account-wide spending cap. Errors and exhausted allowance
degrade context while canonical facts stay available. Label observation time and
point-in-time eligibility are separate from the ledger cutoff. No live Nansen
request was used as verification evidence for this release.

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
The [`two-minute demo script`](docs/demo-script.md) is ready for recording.

## Architecture

The framework covers five planes:

- Data: historical backfill, live capture, point-in-time market state, gap and reorg detection, quarantine, deterministic replay, and Nansen enrichment.
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

The architecture is informed by the public projects and official documentation listed in [`docs/architecture-research.md`](docs/architecture-research.md). Application code is original. `envio/abis/Exchange.events.json` is a subset of events extracted from the MIT-licensed Perpl dex-sdk ABI; see [`envio/abis/SOURCE.md`](envio/abis/SOURCE.md).

OpenAI Codex has been used for web research, architecture drafting, implementation,
documentation, repository setup, and verification. xAI Grok has been used for
bounded review and for implementing the first ledger slice. All generated material
is independently reviewed before adoption. Deterministic code and independently
verifiable data, rather than language-model output, remain the source of analytical facts.
