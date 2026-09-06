# PerpPulse Envio indexer

This HyperIndex project writes **canonical lifecycle events** from the Perpl
Exchange proxy on Monad mainnet. It does not reconstruct positions. The Rust
ledger in `crates/perppulse` is the only position and PnL engine.

## Network

| Field | Value |
| --- | --- |
| Chain ID | 143 |
| HyperSync | https://143.hypersync.xyz |
| Exchange proxy | `0x34B6552d57a35a1D042CcAe1951BD1C370112a6F` |
| Start block | 54773010 |

Use the proxy address, not the implementation address.

## Local run

Envio supports Windows through WSL. Docker Desktop must be running with WSL
integration enabled. Run the indexer from the Linux filesystem view rather than
from native PowerShell:

```powershell
wsl -d Ubuntu-20.04
cd /mnt/d/AI_Models/hackson/monad/envio
pnpm install --frozen-lockfile
pnpm codegen
read -rsp "Envio API token: " ENVIO_API_TOKEN; echo
export ENVIO_API_TOKEN
pnpm dev
```

`pnpm codegen` generates the `generated` module imported by `src/EventHandlers.ts`.
`pnpm dev` is the judge-safe smoke path. On its first run it creates an ignored
`config.quick.yaml` whose start block covers the latest 50,000 blocks, then
creates the local PostgreSQL and Hasura services, migrates the schema, catches
up, and continues live indexing. The generated quick config is reused on normal
restarts so a restart resumes instead of silently moving the start block and
rebuilding the database.

Refresh or widen the quick window only when a deliberate local rebuild is
acceptable:

```powershell
pnpm quick:refresh
PERPPULSE_QUICK_WINDOW_BLOCKS=250000 pnpm quick:refresh
pnpm dev
```

Run the deployment-block history path separately:

```powershell
pnpm codegen
pnpm dev:full
```

Switching start blocks changes Envio persisted state and rebuilds its local
tables. `pnpm start` starts only the indexer for the configuration already
generated. A hosted judging endpoint should be indexed before review; judges
should never wait for deployment-block history during page startup.

The API token must remain a process environment variable. Do not place it in a
tracked `.env` file, command history, test fixture, or diagnostic artifact. Unset
it after the process stops. This indexer is read-only: it does not send
transactions.

Run the deterministic checks after code generation:

```powershell
pnpm typecheck
pnpm test
```

## Output

`CanonicalEvent` stores one immutable row per matched Exchange log. Its identity
is `(chainId, blockHash, txHash, logIndex)`. Each row retains the parent block
hash, block timestamp, raw deterministic payload, schema version, handler
version, classifier version, and indexed ABI fingerprint.
The `ingestionProfile` field is `risk-hotpath-v1`.

The default profile scans from the Exchange deployment block while excluding
`OrderRequest`/`V2` and `TakerOrderFilled`/`V2`. Requests are intents rather than
state changes; taker fills lack account and perpetual identifiers and duplicate
the execution counted from the corresponding maker fill. Position lifecycle,
PnL, liquidation, funding, flow, open-interest, TVL, skew, active-user, fee, and
per-market volume inputs remain indexed. In a live historical prefix sampled on
2026-09-06, these exclusions removed approximately 99% of matched rows.

The latest `CanonicalEvent` is the last **matched event**, not proof of processed
chain coverage. HyperIndex's transactional `_meta.progressBlock` is the
processed-coverage watermark, including quiet blocks that contained no matching
Exchange event. `_meta.sourceBlock` and the current HyperSync height are
independent head observations.

Consumers must expose `_meta.startBlock` with every available time range. The
quick smoke path must not label a partially covered 24-hour, 7-day, 30-day, or
historical range as complete.

While the indexer is running, inspect that distinction from a second WSL shell:

```powershell
export HASURA_GRAPHQL_ADMIN_SECRET=testing
pnpm coverage:check
```

The probe emits one JSON observation and classifies it as:

- `caught_up_active`: processed coverage is current and a match is recent.
- `caught_up_quiet`: processed coverage is current but the matched stream is quiet.
- `lagging`: processed coverage is behind the configured tolerance.
- `quarantined`: an operator supplied an active suspect range.
- `unknown`: the indexer is uninitialized or the observations are missing or inconsistent.

The underlying probe exits `0` only for the two caught-up states, `2` for other
valid classifications, and `1` for fetch or response-validation failures. A
package manager may surface any nonzero script result as a lifecycle failure.
