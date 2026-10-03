# Independent Perpl SDK reference

This operator executes the official `perpl-sdk` `SnapshotBuilder::build` at
Git revision `dbb37c59f6aef03e38d0787eb9c968f59f652617`. It is an independent
read-only verifier of the Envio lifecycle ledger. It does not serve production
accounting or maintain a second persistent ledger. The upstream SDK is MIT
licensed; its retained license is [here](../../LICENSES/Perpl-dex-sdk.txt).

The nested Cargo workspace and lockfile isolate SDK dependencies from the
production application. Build requires Rust 1.95.0 and network access for the
pinned Git dependency and locked crates. No private key or Envio token is needed
for public historical RPC reads. An already indexed, retained Hasura dataset is
required; its admin secret is read from `HASURA_ADMIN_SECRET` process environment
and never exported. Use a secure shell prompt or secret manager for credentials.

```powershell
cargo build --manifest-path tools/perpl-reference/Cargo.toml --locked
py -3 scripts/run_sdk_reference.py `
  --binary tools/perpl-reference/target/debug/perppulse-perpl-reference.exe `
  --block 110018014 `
  --block-hash 0x4bfb8da1589c2f8d1eb49adf854c371febe112c63257115eacc06d5b1abcb447 `
  --accounts 5382,5383,5384,5385 --markets 1,10,40 `
  --graphql http://127.0.0.1:18084/v1/graphql `
  --output sdk-reference.json
```

On Linux use `python3` and omit `.exe`. A custom `CARGO_TARGET_DIR` changes the
binary path. Existing output files are rejected. Exit 0 means all selected
position checks match; exit 2 writes a mismatch scorecard and requires review;
exit 1 means acquisition or validation failed and no successful evidence was
written. The evidence path must have an existing parent directory.

The default public RPC is `https://rpc.monad.xyz`; `MONAD_RPC_URL` can supply an
approved HTTPS endpoint. The wrapper starts a loopback gate that accepts only
chain identity, fixed-number block headers, and fixed-block contract reads of
the Exchange or Multicall3. Transaction submission, signing, latest tags,
other contracts and state overrides are rejected. The SDK receives only this
local URL. No provider response or child stderr is written to disk. Selected
numeric fields, input hashes and request counters are exported. Failures expose
only fixed local reason codes; arbitrary child/provider text is discarded.

Default allowance is 96 RPC requests (hard maximum 256), including failed
attempts; batches count each request. The gate limits request bodies to 64 KiB,
responses to 4 MiB and network waits to 15 seconds. SDK construction has a
120-second deadline; the child has a 180-second deadline. Scope is bounded to
20 accounts, five markets, 10,000 events per account and 100,000 combined events.
SDK construction also reads the selected markets' order books; these are not
exported or adopted as executable-liquidity evidence.

Both acquisitions use the same chain, block number, hash, timestamp and
end-of-block cutoff (`asOfLogIndex: null`). SDK calls pin the block number; the
operator verifies its hash before and after acquisition. This is not EIP-1898
hash binding on every SDK subcall. Envio archival reads require the target
inside both committed and observed-source coverage, verify the retained latest
event, reject regressing metadata and validate target-block event headers.
Account birth or deployment coverage and the exact eligible provenance are
required before replay. A stopped indexer can support a covered historical
read; its retained `isReady` flag does not establish current source freshness.

SDK snapshots omit positions outside `with_perpetuals`; completeness is therefore
restricted to `marketIds`. Explicit closed rows denote absence in the completed
SDK snapshot for a requested market. Comparisons include status, size and
deposit for each account/market, plus side and effective entry for open states.
Unexpected SDK open positions become mismatches. Empty scopes, duplicate or
missing rows, partial log cutoffs and malformed/nonfinite amounts fail.

Lifetime realized PnL, funding and fees are not available from this plain SDK
snapshot. The risk-hotpath profile also cannot establish exact free balances.
These reference fields stay null. Market marks and margin parameters are
selected reference observations only; they do not enter canonical accounting.
See [mainnet verification](../../docs/verification-2026-10-03-sdk.md) for the
accepted execution and remaining limits.

Add `--risk-diagnostics` to export selected SDK price PnL, premium PnL,
maintenance and liquidation observations plus a scenario scorecard. The
canonical entry, size and deposit are combined with SDK reference marks solely
inside the verifier, after native scales and margin parameters match the
registry. Maintenance uses the pinned SDK's UD64 Floor entry representation
followed by UD128 arithmetic; the exact canonical value is also retained.
Entry comparisons project native stored price/Q16 residue through that same
representation and retain the exact ledger entry. This is coefficient-width
rounding, not a fixed decimal-place tolerance. Price PnL is compared after
truncation toward zero to collateral native units; liquidation with SDK
reference premium PnL is compared at market price ticks. Raw values and the
precision contract are retained. These reference inputs never become canonical
marks or funding. Actual canonical liquidation remains null. See the
[risk verification](../../docs/verification-2026-10-03-risk.md).

Add `--market-diagnostics` to acquire canonical v3 MarkUpdated and funding/scale
events at that same historical end-of-block cutoff. The operator compares exact
native mark prices and timestamps, retains funding schedules, includes market
events and mark hashes in canonical manifests, and, with `--risk-diagnostics`,
also compares canonical mark-driven price PnL at collateral units. SDK/REST marks
remain reference observations. Historical market reads use retained coverage
with nonregression/header/subject checks and never relax the live serving gate.

```powershell
py -3 scripts/run_sdk_reference.py `
  --binary tools/perpl-reference/target/debug/perppulse-perpl-reference.exe `
  --block 110245407 `
  --block-hash 0x0cd0fdff14fe0a4a6b42ada5e4c33d2581ba103ef92f3fee6c7ddab72ba19c65 `
  --accounts 5382,5383,5384,5385 --markets 1,10,40,70,90 `
  --graphql http://127.0.0.1:18086/v1/graphql `
  --registry fixtures/protocol/mainnet-registry-2026-10-03.json `
  --risk-diagnostics --market-diagnostics --allowance 96 `
  --output sdk-market-reference.json
```

This requires the corresponding indexed v3 coverage, not an empty database.
For the sixth historical market, run a separate scope `--markets 31` at the same
header and a new output path. For the actual BTC pending funding observation,
use block 110240124/hash
`0xdae4d43d111f8266631e0fb097e1204e66622960d2035b43ffed64b234000b44`
and `--markets 1`. The [mainnet evidence](../../docs/verification-2026-10-03-market-mainnet.md)
records scope, counts, precision, pending-to-active identity and remaining limits.
An offline artifact/source audit is `py -3 scripts/check_market_evidence.py`
from the evidence publication checkout.

```powershell
cargo fmt --manifest-path tools/perpl-reference/Cargo.toml --check
cargo clippy --manifest-path tools/perpl-reference/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path tools/perpl-reference/Cargo.toml --locked
py -3 scripts/test_sdk_reference.py
cargo test --locked --test position_reference --test live_adapter
```
