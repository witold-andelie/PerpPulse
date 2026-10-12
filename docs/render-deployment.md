# Render historical application release

The free Render web service uses the root Dockerfile and `render.yaml`,
with automatic deployment disabled. No database, disk, worker, trading key
or API credential is needed. The image entrypoint requires explicit
historical environment variables; its default fixture command remains a
synthetic local preview.

Public demo: **https://perppulse.onrender.com**.
Service `srv-db61siqjnfac73c47mug`, deployment `dep-db61sj2jnfac73c47o50`,
application commit `1cd3425385eb514a55f8c9bbe58b869fe98ad1f3`.
Render reported deployment live at 2026-10-11T23:42:58.744909Z.
[Public acceptance](evidence/render-public-2026-10-12.json) records exact
source/registry hashes and selected checks. HTTP 200 was verified without
credentials at 2026-10-11T23:43:33.925223Z. Actual browser inspection confirmed
the historical badge, real July cutoff, protocol flows, account switching,
source-linked risk signals and enabled manifest export. Source-failure
behavior was separately checked locally: HTTP 503, cleared rows and disabled
export. This does not claim a public upstream outage was induced.

Cold-start recovery was observed on 2026-10-12: a new process logged startup
at 19:17:38.250645Z and the full 49,204-event scope at 19:17:57.678896Z.
The independent public HTTP check at 19:18:10.630289Z matched the original
canonical, registry and proof hashes. These are startup/read markers, not a
complete cold-start-duration measurement. No additional manual restart was
needed after this recovery evidence was observed.

[Analytics CI](https://github.com/witold-andelie/PerpPulse/actions/runs/38185114041)
and [publication policy](https://github.com/witold-andelie/PerpPulse/actions/runs/38185113928)
both passed for the exact deployed commit. Rust/fresh PostgreSQL/DOT job
114605660526, Envio job 114605660321 and pinned SDK job 114605660484 all
succeeded. Local Rust tests, historical regressions, formatting, Clippy,
entrypoint shell syntax and staged publication checks also passed.

## Data and financial scope

Envio deployment `8ac8f338a87c94c07c9eb685bcf5e5d6fe4484e2` on `envio-free`
uses `envio/config.free.yaml`. Its public source is
`https://indexer.dev.hyperindex.xyz/972db24/v1/graphql`. Do not push application
releases to that branch or silently change the fixed indexed range.

The application reads all **49,204** canonical events in Monad blocks
**89,200,000–89,210,000**, 11:43:01–12:49:49 UTC on 2026-07-21.
It checks independent start/end RPC headers before and after acquisition,
the exact processed range/count, canonical provenance, latest-event anchor,
and a reviewed full-input hash. The shared cutoff is block 89,210,000,
hash `0xb0c80ee5b4f9b2026930d1550f4d20cf306e82bc4847683c2d572dc720ceea19`,
timestamp 1784638189000 ms. After acquisition, availability, pinned scope,
latest anchor and cutoff header are rechecked every 60 seconds. Failed
checks clear the served snapshot; changed pinned scope/anchors quarantine
it until restart and reconciliation. Availability checks expire after
150 seconds. The cached artifact is fixed historical evidence, not a live
freshness certificate or an independent certificate of every upstream log.

The window drives 19,738 maker fills, AUSD volume **1,953,921.189425**,
protocol fees **481.270924**, one liquidation and 354 active accounts.
The selected accounts 4198, 4199, 4200 and 4201 were born inside the window.
Their position replay uses the existing canonical ledger. Only their
account/supporting-market events, all liquidations and the latest anchor
are exposed for inspection (512 events); the manifest hashes all 49,204.
Global open interest, position collateral, skew and incomplete 24h/7d/30d
totals stay unavailable. Unknown funding keeps actual equity and liquidation
risk unavailable; zero-funding stress scenarios remain conditional.

The dedicated [historical registry](../fixtures/protocol/mainnet-registry-block-89210000.json)
contains exactly the six event markets. The pinned official SDK
`dbb37c59f6aef03e38d0787eb9c968f59f652617` independently verifies their
native scales, collateral decimals and cutoff margin parameters. This
found ZEC's historical inverses were **8x / 15x**, rather than the later
10x / 18x values. Other markets matched the prior registry.
Two `SnapshotBuilder::build` executions matched **78 position checks** for
four accounts at the same block/hash/time. The
[primary](evidence/sdk-historical-primary-2026-10-12.json) and
[secondary](evidence/sdk-historical-secondary-2026-10-12.json) selected
records contain 57 and 26 gated read-only RPC calls respectively. Each
includes one expected legacy `getContractVersion` revert, handled by the
official SDK fallback. No orders, signing or state overrides were permitted.
RPC requests were serialized at two per second to respect public limits.
SDK marks, funding and balances were not adopted. Position reconciliation
in the served manifest remains unverified because these separate verifier
artifacts are not injected into the served ledger.

The [runtime proof](../fixtures/protocol/historical-market-proof-89210000.json)
binds the full canonical input hash, registry hash, cutoff and all event
market parameters. Missing or changed proof inputs fail closed. SDK calls
are number-pinned with before/after hash verification, not per-subcall
EIP-1898 binding. This is a historical demo, not a production trading tool.

## Reproduce locally

```sh
cargo build --locked
target/debug/perppulse serve-envio-archive \
  --accounts 4198,4199,4200,4201 \
  --graphql-url https://indexer.dev.hyperindex.xyz/972db24/v1/graphql \
  --registry fixtures/protocol/mainnet-registry-block-89210000.json \
  --registry-proof fixtures/protocol/historical-market-proof-89210000.json \
  --start-block 89200000 --end-block 89210000 \
  --expected-event-count 49204 --bind 127.0.0.1:8081
```

`/health` returns 503 while initialization or source verification fails.
`/api/manifest`, `/api/protocol`, `/api/wallets`, `/api/analytics` and
`/api/signals` share the cutoff. Write methods return 405. The page explicitly
shows HISTORICAL ENVIO WINDOW and the actual July timestamp.

For SDK reproduction, build `tools/perpl-reference` using its locked manifest,
run `scripts/run_sdk_reference.py` against the same historical registry,
accounts, GraphQL endpoint, block/hash, and markets `1,10,20,31,40` then `50`.
Use an officially documented historical-state RPC via `MONAD_RPC_URL`.
Public RPC transport/rate failures are rejected; they do not produce
successful evidence. The existing request/method/target/size/time bounds
still apply.

## Free hosting limits

The owner-authorized service plan is `free` in Frankfurt. At the read-only
account check on 2026-10-12, the workspace was Hobby, with no payment card,
3.77/750 free instance hours, 3 MB/5 GB bandwidth, 2/500 pipeline minutes,
and USD 0 month-to-date charges. These observations are not future usage
guarantees. Do not add payment details, choose a paid plan, change unrelated
services or use GCP credits without owner approval.

Render free web services sleep after 15 minutes without inbound traffic;
opening the application can take about a minute to wake it, followed by
historical-source loading. The 750 hours are shared by the workspace.
Do not create artificial keep-awake requests. Envio Development also has
inactivity/storage limits and a maximum deployment age of 30 days. The
49,204-event cap alone does not provide permanent hosting. If the source
expires, the app visibly becomes unavailable.

References: [Render free services](https://render.com/docs/free),
[Blueprint specification](https://render.com/docs/blueprint-spec),
[Envio development limits](https://docs.envio.dev/docs/HyperIndex/hosted-service-deployment),
[Monad historical RPC information](https://docs.monad.xyz/developer-essentials/network-information).
