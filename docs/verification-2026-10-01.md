# Local verification, 2026-10-01

This record covers the application completion work originally based on local
commit `c863573`. It records fixture, mock-provider, packaging, and disposable
PostgreSQL verification. It is not evidence of a new live mainnet reindex,
external SDK reconciliation, paid Nansen usage, or a cloud deployment.

## Rust and PostgreSQL

Rust 1.95 on Windows completed 38 tests: 11 library tests, 16 lifecycle tests,
2 mock HTTP live-adapter tests, and 9 service tests, including the normally
ignored PostgreSQL integration test. There were zero failures and zero ignored
tests in the explicit integration run.

Create a disposable database bound only to loopback, then run:

```powershell
docker run --name perppulse-verification-db -d -p 127.0.0.1:15432:5432 -e POSTGRES_HOST_AUTH_METHOD=trust -e POSTGRES_DB=perppulse postgres:16-alpine
docker exec perppulse-verification-db pg_isready -U postgres
$env:PERPPULSE_TEST_DATABASE_URL='host=127.0.0.1 port=15432 user=postgres dbname=perppulse'
cargo test --locked -- --include-ignored
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Use a different container name if a previous run already exists. The database
must be disposable: the integration test deliberately changes its own
`integration-test` row to test invalid stored data.

Covered behavior includes lifecycle accounting, retained realized facts across
close/reopen, native aggregate and notional overflow rejection, timestamp
lookahead rejection, Exchange identity checks, bounded ranges, deterministic
manifests, exact-cutoff scorecards, missing marks and balances, and explicit
unavailable results. Mock live ingestion checks the independent Monad chain
identity and head, subject identity, bounded paging, and shared cutoff. The live
worker rejects stalls and latches quarantine when canonical events, block hash,
registry inputs, or marks change at the same cutoff.

PostgreSQL checks cover idempotent publication, compact snapshots without raw
events, missing/stale rows, content-hash mismatch, regressing coverage, and
changed inputs at the same cutoff. Nansen tests use a local mock server and
check the cache, process request allowance, and separate observation time.

## Envio

A clean Linux `node:22-bookworm` container completed frozen dependency
installation with pnpm 10.5.2, ABI code generation, TypeScript checking, and
all 17 handler/coverage/configuration tests. No host `node_modules` or generated
module was reused. Run from the repository root:

```powershell
docker run --name perppulse-envio-check-v2 --mount 'type=bind,source=D:\AI_Models\hackson\monad,target=/source,readonly' --workdir /work node:22-bookworm sh -c 'corepack enable && cp /source/envio/package.json /source/envio/pnpm-lock.yaml /source/envio/tsconfig.json /source/envio/config.yaml /source/envio/schema.graphql . && cp -r /source/envio/src /source/envio/test /source/envio/scripts /source/envio/abis . && pnpm install --frozen-lockfile && pnpm codegen && pnpm install --frozen-lockfile && pnpm typecheck && pnpm test'
```

The container exited with status 0. Native Windows installation was not accepted
as evidence because of existing WSL-linked dependencies and unavailable native
Envio support. Pinning pnpm 10.5.2 and explicitly permitting the required
`esbuild` and `rescript` build scripts made the isolated Linux installation
reproducible.

## Container, HTTP, and browser

The production Dockerfile built a Linux release binary and ran successfully
without the source tree. The container user was `uid=10001(perppulse)`.
Packaging verification caught and fixed registry path resolution that had
depended on the build-time source directories.

The final local image index digest was
`sha256:37094f51d8601386ff9617e9f92bb7061a8d8209cd44e6291d64a01dd190cb68`.
It has not been pushed to a registry. Base tags can change between builds;
this digest identifies the image actually checked in this session.

```powershell
docker build -t perppulse:verification .
docker run --name perppulse-preview -d -p 127.0.0.1:18081:8080 perppulse:verification
Invoke-RestMethod http://127.0.0.1:18081/health
Invoke-RestMethod http://127.0.0.1:18081/api/snapshot
docker exec perppulse-preview id
```

The open-position fixture returned chain 143, cutoff 54,773,030, 2 wallets,
3 events, volume `70000.000000`, and open interest `71000.000000`. These are
synthetic fixture facts. HTTP checks returned 405 for POST, 400 for an event
range beginning before coverage, and 409 for a conflicting `asOfBlock`.

Chrome verification exercised the account selector, account 42's BTC long
position, its event evidence button, an invalid block range, and the
selected-account filter. The restored valid range showed 2 of 2 account events
at the original cutoff. A browser screenshot confirmed that the fixture badge,
metrics, coverage, account details, and evidence controls rendered correctly.
This was manual browser verification, not a committed browser regression suite.

The Windows debug CLI and Linux container release CLI produced the same snapshot
hash for `fixtures/golden/open-position-as-of.json`:

| Evidence | SHA-256 |
| --- | --- |
| Snapshot | `c2befb1f569113e3a5da7074a280aa639f85d91e299329a8b0687a9db59b1ab0` |
| Canonical inputs | `2e15219239a27e4c60acd0a1b486aac1792c9530922c45625e83856558beb887` |
| Registry inputs | `03bab77b773bfecda7c480a7880de94224cd8a7e7d3563959e8078b4faeab993` |
| Market marks | `8bdc1cb031952bba3512c7cc34ef2a3c9b4ce8629bdf3fa53216180e7ef24c2a` |

Reproduce with `cargo run --locked -p perppulse -- evidence
fixtures/golden/open-position-as-of.json` and `docker exec perppulse-preview
perppulse evidence fixtures/golden/open-position-as-of.json`. Hash equality binds
these deterministic inputs; it does not prove independent finality or ingestion
completeness.

## Repository and delivery checks

```powershell
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
dot -Tsvg perppulse_opm.dot -o perppulse_opm.svg
.\deploy\application.ps1 -ImageUri 'europe-west3-docker.pkg.dev/project-5e761e8c-65aa-4033-8cb/perppulse/perppulse@sha256:0000000000000000000000000000000000000000000000000000000000000000'
```

The English publication scan and whitespace check passed. Graphviz validated the
DOT and regenerated its tracked SVG. The deployment script's default dry run
validated the immutable-image URI and displayed the bounded, authenticated
Cloud Run command. The illustrative URI is not a published image. No deployment
was executed. GitHub Actions configuration was added but has not run remotely.
The disposable PostgreSQL container was stopped after verification; the final
fixture preview was left running on loopback port 18081.

## Remaining external and product evidence

- A fresh temporary Envio token is required for a live `risk-hotpath-v2` rebuild.
- Full-history mainnet account replay and independently obtained Perpl SDK
  references have not been reconciled by this session.
- Verified live marks, global historical protocol metrics, wallet comparison,
  and alerts remain product work; watchlist data cannot establish global totals.
- Nansen integration is mock-tested only. Real requests require a key, owner
  approval for costs, and confirmed data-use permission.
- The compact Cloud SQL migration, registry image push, Cloud Run execution,
  public demo video, and authenticated sponsor catalog checks remain pending.
- Verification performed no order placement or custody operation. Git
  publication is a separate owner-authorized step. Local operational progress
  remains ignored by Git.
