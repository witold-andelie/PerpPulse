# PerpPulse

Real-time protocol-to-wallet risk intelligence for perpetual markets on Monad.

PerpPulse turns Perpl market state, Envio-indexed onchain events, and Nansen wallet context into a small set of traceable risk signals. A judge or trader can move from a protocol-level anomaly to the affected market, wallet, and source event without losing the selected time or as-of context.

> Status: canonical ledger, coverage-bounded Envio-to-Rust adapter, and
> golden-fixture demo implemented. The Envio `risk-hotpath-v1` index was
> verified against live Monad data with a coverage-aware judge quick start;
> the expanded `risk-hotpath-v2` profile is code-generated and tested but still
> requires a fresh live reindex. Licensed Apache-2.0. GCP foundation is live
> in `europe-west3` on project `project-5e761e8c-65aa-4033-8cb`, ceiling EUR 350.
> No Cloud Run worker is deployed yet. Public repository:
> `https://github.com/witold-andelie/PerpPulse`.

## Competition fit

- Primary track: Onchain Finance & Trading
- Perpl bounty: Best Analytics / Risk Tool
- Envio bounty: Best Use of Envio
- Nansen bounty: Best Use of Nansen

The planned product is read-only. It does not place orders, request private keys, or present AI-generated numbers as financial facts.

## Judge-facing product path

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

Rust 1.85+ and Python 3.12+ are required for the ledger tests and the publication policy check. Node.js 22+ and pnpm are required only to generate and run the Envio indexer.

```powershell
cargo test
cargo run -p perppulse -- demo fixtures/golden/open-increase-reduce-close.json
cargo run -p perppulse -- envio-account 5238 --inspect-only
python scripts/check_repository_policy.py --tracked
```

### Google Cloud foundation

Owner-authorized project `project-5e761e8c-65aa-4033-8cb` in `europe-west3` (Frankfurt). That region is in use: Cloud SQL `perppulse-pg` is `RUNNABLE` there. The script creates Artifact Registry, evidence storage, a runtime service account, Secret Manager, a EUR 350 budget, and Cloud SQL `db-g1-small`. It does not deploy Cloud Run.

```powershell
.\deploy\foundation.ps1
.\deploy\stop.ps1
```

The database password is stored in Secret Manager as `perppulse-db-password` and is not printed.

Envio local indexer (optional, needs Docker Desktop, WSL integration, and an
Envio API token that must not be committed):

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

The Metropolis rules require a working Monad-mainnet product, public GitHub source, an OSI-approved license, setup instructions, attribution, build-window commit history, documentation, contract addresses or transaction hashes, and a public operating demo no longer than three minutes.

This repository is licensed under the Apache License 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

## Attribution and AI disclosure

The architecture is informed by the public projects and official documentation listed in [`docs/architecture-research.md`](docs/architecture-research.md). Application code is original. `envio/abis/Exchange.events.json` is a subset of events extracted from the MIT-licensed Perpl dex-sdk ABI; see [`envio/abis/SOURCE.md`](envio/abis/SOURCE.md).

OpenAI Codex has been used for web research, architecture drafting, documentation, repository setup, and verification. xAI Grok has been used for bounded review and for implementing this first ledger slice. All generated material is independently reviewed before adoption. Deterministic code and independently verifiable data, rather than language-model output, remain the source of analytical facts.
