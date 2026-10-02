# Verification, 2026-10-02

This change starts from published commit
`bbdcf52ca426b3369b9b30293c666ea44292f72f`. It fixes the published Envio CI
failure, hardens local mark and registry validation, and adds sanitized public
metadata observations. It does not complete live v2 indexing, SDK position
reconciliation, global aggregation, paid Nansen verification, or cloud deployment.

## Published CI diagnosis

The [repository-policy run](https://github.com/witold-andelie/PerpPulse/actions/runs/36931893836)
passed. In the [analytics run](https://github.com/witold-andelie/PerpPulse/actions/runs/36931893868),
Rust/PostgreSQL, formatting, Clippy, policy, and DOT passed; Envio failed one
ABI fingerprint test (16 of 17 tests passed).

The Windows ABI worktree contained 2,037 CRLF endings among 2,284 newline
sequences. Its byte hash was `16b3a481...`. The Git blob contained only LF and
hashed to `b98e14a4...`. Converting CRLF to LF produced byte-for-byte equality
with the published blob. There was no semantic ABI change. A Linux reproduction
failed the same fingerprint assertion before the fix; an initial reduced
harness also omitted `config.yaml`, which was corrected for the complete check.

The v2 writer and reader now require
`sha256:b98e14a49e4201d71feeae380261784fc8872aa45b201d193194c6c5d56adbf1`.
The fingerprint test checks both LF and CRLF copies. The Rust adapter rejects
the obsolete mixed-ending v2 fingerprint. V1 remains inspection-only.

## Reproducible checks

Run from the repository root using Rust 1.95 and a fresh disposable database:

```powershell
docker run --name perppulse-verification-20261002-db -d -p 127.0.0.1:15433:5432 -e POSTGRES_HOST_AUTH_METHOD=trust -e POSTGRES_DB=perppulse postgres:16-alpine
docker exec perppulse-verification-20261002-db pg_isready -U postgres
$env:PERPPULSE_TEST_DATABASE_URL='host=127.0.0.1 port=15433 user=postgres dbname=perppulse'
cargo test --locked -- --include-ignored
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
dot -Tsvg perppulse_opm.dot -o .scratch/perppulse_opm-20261002.svg
```

All 44 Rust tests passed: 11 library, 16 lifecycle, 2 mock live adapter,
6 market-input tests, and 9 service tests including PostgreSQL. Zero failed or
ignored. Formatting, Clippy, policy, and whitespace checks passed. DOT rendered
successfully; its output and tracked SVG both hashed to
`59f8df9f36aea0f9a8f5c77384727d2eb95163b99812b8c0fb18551ae398bdd2`.

The new tests exercise malformed context, subject and scale mismatches,
new/changed metadata, selected-field export, deterministic hashes, stale state,
mark staleness, nonpositive/overflowing prices, duplicate marks, same-block
log lookahead, wrong block hashes, and checked financial arithmetic.

Reusing the previous day's database initially rejected the changed mark-input
hash at the same synthetic cutoff. That is the intended immutable-publication
guard. The passing integration run used the fresh isolated database above.
Existing index databases were not reset or migrated.

A clean Linux container completed frozen installation, code generation,
typechecking, and all 17 Envio tests:

```powershell
docker run --name perppulse-envio-check-20261002 --mount 'type=bind,source=D:\AI_Models\hackson\monad,target=/source,readonly' --workdir /work node:22-bookworm sh -c 'corepack enable && cp /source/envio/package.json /source/envio/pnpm-lock.yaml /source/envio/tsconfig.json /source/envio/config.yaml /source/envio/schema.graphql . && cp -r /source/envio/src /source/envio/test /source/envio/scripts /source/envio/abis . && pnpm install --frozen-lockfile && pnpm codegen && pnpm install --frozen-lockfile && pnpm typecheck && pnpm test'
```

The container exited 0. No host generated module or dependencies were reused.
Use fresh container names for reproduction; previous evidence containers are
retained. Generated dependency warnings remain; dependencies were not upgraded.

## Public observations and preserved index

```powershell
cargo run --locked -p perppulse -- inspect-context --output docs/evidence/perpl-context-2026-10-02.json
cargo run --locked -p perppulse -- envio-account 5238 --inspect-only --page-size 1 --max-events 10
```

The new adapter made one successful public Perpl context request without any
credentials. Its [selected-field observation](evidence/perpl-context-2026-10-02.json)
is timestamped `1790970891882` ms and has fields hash
`sha256:6aa93a552eba7c6efb11b364c8e58d45413e84e3070521df004a9714aa2792ff`.
It observes 11 markets, including new VVV (70), NEAR (100), and UNI (110), and
contract version 1.7.5. State observation ages were 882-1,882 ms. That is not
proof of mark-update age. The adapter marks every REST price ineligible for
accounting and leaves the canonical fixture registry unchanged. Current REST
fees use micros; the inspector does not import those schedules.

The existing local index still reports v1, start block 102,494,396, processed
block 102,590,850, and 383,411 cumulative indexed events. Account 5238 has two
rows and is correctly ineligible for financial replay. This was preservation
and compatibility inspection, not current mainnet health verification.

Public hackathon rules were rechecked as v3 and mapped to the
[submission checklist](submission-checklist.md). Authenticated sponsor rules
remain unavailable. No Nansen request or GCP action was performed.

## Linux image and HTTP evidence

```powershell
docker build -t perppulse:20261002 .
docker run --name perppulse-preview-20261002 -d -p 127.0.0.1:18081:8080 perppulse:20261002
docker exec perppulse-preview-20261002 id
Invoke-RestMethod http://127.0.0.1:18081/health
cargo run --locked -p perppulse -- evidence fixtures/golden/open-position-as-of.json
docker exec perppulse-preview-20261002 perppulse evidence fixtures/golden/open-position-as-of.json
```

The image index is
`sha256:87a2220c03c26d3f359731a48b29e938a2b8d2e473ed864a549fe85330a0c5d0`.
It runs as UID 10001 and serves the explicit fixture preview on loopback.
The page returns 200, POST returns 405, a pre-coverage event range returns 400,
and a conflicting cutoff returns 409. Fixture health reports chain 143 and
cutoff 54,773,030. These are synthetic facts, not mainnet service evidence.

Windows and Linux fixture snapshot hashes match:
`sha256:0d49e89b3741a671582f7f0b8120147100a7451d7132302214ded1c52534caa8`.
Canonical and registry hashes are unchanged from October 1. Mark hash is now
`sha256:6e00e7229cf2edb9db384c696f95e407f77dbfef644ffa9b8daddec6a36ac2d6`;
methodology v2 hash is
`sha256:7ccec73ef1203fb039161d708c6626be1886d2886e132224926802d678f61d14`.
New optional mark provenance fields and the versioned quality contract change
the snapshot hash, so October 1 hashes remain historical evidence.

The local preview remains available. Disposable test databases are stopped
after verification. `PROGRESS.md` remains ignored and excluded from publication.
