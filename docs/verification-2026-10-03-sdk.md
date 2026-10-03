# Official SDK execution at an identical historical cutoff

The official Perpl SDK state engine was executed on 2026-10-03, using
`SnapshotBuilder::build` at revision
`dbb37c59f6aef03e38d0787eb9c968f59f652617`. It matched **40 position checks**
against the Envio canonical replay for accounts 5382, 5383, 5384 and 5385,
restricted to BTC (1), MON (10) and HYPE (40). The selected result is
[here](evidence/sdk-execution-2026-10-03.json), observed at 08:22:07 UTC.

Implementation `9b644168ac65760203f915ec542774a61fafcff4` is published and
remote-SHA verified. Its Rust/PostgreSQL, Envio, SDK-operator and policy jobs
all passed; [the selected CI record](evidence/ci-9b64416-2026-10-03.json)
links the exact runs and jobs.

Both acquisitions used Monad chain 143, block **110018014**, hash
`0x4bfb8da1589c2f8d1eb49adf854c371febe112c63257115eacc06d5b1abcb447`,
timestamp **1790981535000 ms**, and the **end-of-block** cutoff
(`asOfLogIndex: null`). This resolves the earlier diagnostic's log-48 versus
end-of-block difference. The earlier [ABI diagnostic](verification-2026-10-03-envio.md)
remains a separate, accurately limited observation.

## Acquired inputs and checks

Retained Envio coverage spans 109944714 through committed block 110019606;
observed source height was also 110019606. The archive selected 2,820 events:
3 for 5382, 2,725 for 5383, 55 for 5384 and 37 for 5385. Exact v2 provenance
and account birth inside coverage were required. Per-account canonical,
methodology and registry hashes are retained. The historical `isReady: true`
metadata does not establish present-day freshness or an active indexer.

The completed SDK snapshot supplied all 12 requested account/market states.
Two were open (5382/BTC and 5383/MON); ten were absent/closed. Status, size and
deposit were compared for all 12, plus side and effective entry for the two
open positions: 40 matches. The MON effective entry was exactly
`0.03208550299072265625`, including Q16 residue. No tolerance, floating-point
rounding, missing-field match or synthetic financial input was used.

The SDK omits positions outside `with_perpetuals`, so completeness is restricted
to `marketIds`. Explicit closed rows denote absence only inside that completed
scope. Unexpected open state is a mismatch; missing rows, duplicates and
malformed amounts are errors.

There were **43 read-only RPC requests** against a 96-request allowance: one
`eth_chainId`, three `eth_getBlockByNumber` and 39 `eth_call` requests, with no
gate rejections or RPC errors. Header hash and timestamp were checked before
and after acquisition. SDK subcalls pin the block number; this is not EIP-1898
hash binding on every subcall.

## Reproduction and local verification

Follow the [operator instructions](../tools/perpl-reference/README.md) using a
retained eligible Envio dataset and an approved historical RPC. No private key,
signer or Envio token is needed for the reference. Actual Windows execution
used `.scratch/sdk-reference-target` and loopback GraphQL port 18084. Provider
authentication was supplied only in process environment. The exported result
contains selected numbers and hashes, with no owner addresses, credentials,
raw provider responses or child stderr.

The record identifies the final Windows operator binary SHA-256 and
LF-normalized source/lockfile hashes. The nested Cargo workspace isolates SDK
dependencies from production dependency resolution. The upstream MIT license
is retained and attributed in [NOTICE](../NOTICE).

```powershell
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo fmt --manifest-path tools/perpl-reference/Cargo.toml --check
cargo build --manifest-path tools/perpl-reference/Cargo.toml --locked
cargo clippy --manifest-path tools/perpl-reference/Cargo.toml --locked --all-targets -- -D warnings
py -3 scripts/test_sdk_reference.py
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
```

Local Rust tests passed: **54**, with one disposable PostgreSQL integration
ignored locally. Two Python gate tests passed. Regressions reject transaction
and signing RPCs, latest/other-block reads, other contract targets, state
overrides, exhausted allowance, changed headers, regressing coverage, partial
cutoffs, missing/duplicate market rows, nonfinite/overprecision amounts and
unexpected open positions. Live freshness checks remain enforced; archives
are never their fallback. Public CI builds and checks the isolated operator
without live RPC calls.

## Remaining verification

Plain SDK snapshots cannot supply lifetime realized PnL, settled funding or
fees. Risk-hotpath coverage excludes subjectless taker balances, so exact free
balance is also outside this result. Those reference fields remain null;
manifests label account totals unverified and position verification separately.
This completes selected position verification, not complete account accounting.

SDK market observations supplied scales, margin parameters and historical
marks (BTC 84490, MON 0.032243, HYPE 87.6768), with update times and age limits.
They remain reference observations. Adoption requires a point-in-time mark
contract, registry validation and comparison of risk/PnL semantics before the
live frontend may consume derived risk values. No SDK balance, mark or
order-book state entered the canonical ledger. Continuous hosting, global
aggregation and live Nansen remain open.

The subsequent [risk formula verification](verification-2026-10-03-risk.md)
corrects the fixture model to entry-based maintenance and separates total PnL
from its price component. Six selected scenario comparisons match at explicit
native precision. Canonical marks and unsettled funding remain unverified;
actual liquidation and equity remain null. Position or scenario matches do not
establish a complete live funding/risk pipeline.
