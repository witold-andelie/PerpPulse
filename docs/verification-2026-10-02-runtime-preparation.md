# Isolated runtime preparation and sponsor scope, 2026-10-02

The prepared Envio runtime uses published source
`a8e5254095496e90b32b15cc331782d7cadc11ef`. Eighteen tracked Envio files were
extracted from that exact Git revision and checked by SHA-256 inside the new
container. Host-generated modules were not copied. Linux dependencies come from
the previous passing clean-Linux verification container.

The local-only prepared files are under `.scratch/live-v2-20261002`, and the
masked operator entry point is `.scratch/start-live-indexer.ps1`. None contains
credentials. The token supplied in chat was not used or copied into commands,
files or Docker configuration. A replacement token remains required through
the local hidden-input prompt.

## Isolation and preparation checks

| Item | Recorded result |
| --- | --- |
| Indexer container | `perppulse-envio-v2-20261002`, idle until hidden input |
| Dependency image | `perppulse-envio-runtime:a8e5254`, ID `sha256:687e63550c5ab4500ffad6926c53cce4afc633a561ab8ec0a318b7f30a75155a` |
| Dedicated network | `perppulse-live-v2-20261002` |
| New PostgreSQL container | `perppulse-live-v2-20261002-postgres`, healthy, loopback port 15434 |
| New Hasura container | `perppulse-live-v2-20261002-hasura`, healthy, loopback port 18084 |
| New named volume | `perppulse-live-v2-20261002_db_data` |
| Preparation check | Source hashes and Hasura health passed; reports `liveAcceptance: pending` |
| Offline self-check | Synthetic credential redaction and source identity passed |
| Missing-input check | Empty stdin exited 1 visibly before any HyperSync request; no initialized marker was written |
| Existing v1 preservation | Inspection still reports blocks 102494396-102590850 and 383411 events; v1 remains replay-ineligible |
| Fixture app | Existing explicit synthetic preview retained on loopback 18081 |

Reproduce the completed checks in this prepared workspace:

```powershell
powershell.exe -NoProfile -File .scratch/start-live-indexer.ps1 -CheckOnly
docker exec perppulse-envio-v2-20261002 node /work/launch-live.cjs --self-test
docker exec -i perppulse-envio-v2-20261002 node /work/launch-live.cjs
target/debug/perppulse.exe envio-account 5238 --inspect-only --page-size 1 --max-events 10
```

The empty-input invocation is expected to fail. The actual launcher sends a
newly created token through process stdin, sets it only in the child environment
for authenticated requests and indexing, redacts it from console lines and
disables Envio file logging. Docker container configuration and CLI arguments
do not carry it. The run has a thirty-minute limit and targets the entire child
process group when stopping. Normal resumption preserves the quick config and
uses `start` without reset flags. The authenticated path, catch-up, stop/resume,
provenance rows and live UI remain unverified until actual operating evidence
is obtained.

WSL inspection initially failed on sandbox access, then found that the host
dependency installation lacked a Linux Envio executable. The prepared runtime
uses the verified Linux dependency image instead. The indexer was initially
created with Docker network `none`, which rejected an additional network;
disconnecting `none` before attaching the dedicated network resolved it. A
PowerShell wrapper health check initially lacked sandbox Docker-pipe access;
its approved rerun passed. These preparation failures are not live-indexing
evidence.

## Sponsor scope and Nansen contract correction

The owner supplied Envio, Perpl and Nansen bounty text and explicitly retained
the read-only product. Requirement mappings are in [Envio](envio-bounty.md),
[Perpl](perpl-bounty.md), [Nansen](nansen-bounty.md) and the
[submission checklist](submission-checklist.md). Official catalog URLs and
revisions remain unconfirmed. The supplied Perpl trading-bot task is outside
the selected scope. The working sponsor recording limit is two minutes.

The [official Nansen label contract](https://docs.nansen.ai/api/profiler/address-labels)
allows missing `category`. Adding a label-only row to the local mock reproduced
the previous parser failure: context became unavailable. The parser now
preserves that missing category as null while rejecting invalid supplied
categories. No classification is invented. The same mock retains cache,
request allowance and separate observation-time checks.

```powershell
cargo test --locked -p perppulse
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
```

The test run passed 43 tests with one PostgreSQL integration test intentionally
ignored; that test's database behavior was not changed. Formatting, Clippy,
publication policy and whitespace checks passed. No live Nansen call, cloud
action, transaction, fresh mainnet index or completed bounty claim is included.
