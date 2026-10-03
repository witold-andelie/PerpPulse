# Entry-based maintenance and explicit funding uncertainty

The prior fixture model used mark notional for maintenance and exposed a
liquidation price that excluded unsettled funding. On 2026-10-03, a bounded
official SDK execution reproduced the difference at the previously accepted
historical cutoff. The corrected model uses effective entry notional for
maintenance. Price PnL is now separate from total unrealized PnL, and actual
equity/liquidation values remain null for open positions until canonical
unsettled funding is reconstructed.

This is selected formula verification with reference inputs, not acceptance of
a live mark/funding pipeline. No orders, signing, custody, paid provider calls,
new cloud resources or changes to Envio handlers/schema were involved.

## Immutable scope and retained evidence

- Base publication: `2f4f8b6572a6a323f20fbcbfa1a3287c2d13edd6`.
- SDK: `dbb37c59f6aef03e38d0787eb9c968f59f652617`, MIT licensed.
- Envio source: `a8e5254095496e90b32b15cc331782d7cadc11ef`.
- Chain 143, block `110018014`, end-of-block log cutoff null.
- Block hash: `0x4bfb8da1589c2f8d1eb49adf854c371febe112c63257115eacc06d5b1abcb447`.
- Block timestamp: `1790981535000` ms.
- Accounts 5382-5385; requested markets 1/BTC, 10/MON and 40/HYPE.
- Retained canonical account events: 2,820, with eligible birth/provenance coverage.
- Final observation: `2026-10-03T09:32:57.160784Z`.

[SDK execution](evidence/sdk-risk-execution-2026-10-03.json) retains 40 matching
position-field checks, six matching risk comparisons for the two open positions,
selected SDK observations, source/lockfile/binary hashes, and request counters.
Its SHA-256 is
`7d82ccba0d61a4487d4590df241a238a05d5ee90a41ee45ab99b7a3f65d96588`.
Exactly 43 bounded public RPC requests were used: one chain identity, three
block-header requests and 39 contract reads. No errors or rejections occurred.
The allowance remains 96 and the hard maximum 256. Header, archive, request
scope and timeout restrictions are described in the
[operator instructions](../tools/perpl-reference/README.md).

## Formula and precision contract

The pinned [SDK position implementation](https://github.com/PerplFoundation/dex-sdk/blob/dbb37c59f6aef03e38d0787eb9c968f59f652617/crates/sdk/src/state/position.rs)
uses effective entry notional for maintenance and signed premium PnL in its
liquidation expression. Official [margin documentation](https://docs.perpl.xyz/exchange/margin)
describes maintenance inverses, and [funding documentation](https://docs.perpl.xyz/exchange/funding)
describes cumulative virtual funding payments. The SDK implementation at this
revision defines the numerical comparison here; high-level examples alone do
not establish numerical equivalence.

Let `E` be effective entry including Q16 residue, `S` position size, `D`
isolated deposit, `F` signed unsettled funding PnL, `M` the maintenance inverse,
and `sign` be +1 for long, -1 for short:

- Maintenance: `E * S / M`.
- Price PnL: `sign * (mark - E) * S`.
- Conditional equity: `D + pricePnl + F`.
- Conditional liquidation: `max(0, E + sign * (maintenance - D - F) / S)`.

Checked finite decimal arithmetic rejects overflow, invalid side, nonpositive
entry/size, negative deposit and maintenance inverses at or below one. No float
tolerance is used. Maintenance comparisons are exact. Price PnL is compared
after truncation toward zero to the six collateral decimals. Liquidation
scenarios are compared after truncation to each market's native price decimals;
prices are nonnegative, so this is the corresponding price-tick floor. Both
unrounded values and the comparison precision remain in each check. Unit tests
prove that a one-native-unit or one-price-tick difference still fails.

| Position | Entry-based maintenance | SDK price PnL | SDK unsettled funding | Conditional liquidation using SDK funding |
| --- | --- | --- | --- | --- |
| Account 5382 / BTC long | 562.00723656 | -127.918714 | -2.850694 | 82913.83199757252093 |
| Account 5383 / MON long | 174.75690058927001953125 | 17.156464 | 0 | 0.03048117789423374591 |

BTC's zero-funding scenario is `82896.53199757252093700691832`;
using the reference premium changes it by `17.30`. The MON price formula yields
`17.156464214599609375`, whereas the SDK native-collateral view yields
`17.156464`. Equality is claimed at the documented native precision, not at
every decimal digit.

SDK marks and premium PnL are scenario inputs inside the independent verifier
only. Their use here does not prove independently indexed MarkUpdated events,
funding epoch application, cumulative funding coverage or position checkpoints.
SDK snapshot values never backfill the canonical ledger. Each scorecard retains
`canonicalActualLiquidationPrice: null`.

## API and browser behavior

[Methodology v5](methodology.json) defines these semantics under the existing
nullable snapshot contract:

- `unrealizedPricePnl` is the price component. The wallet aggregate is null if
  any open position lacks an eligible mark, rather than a partial sum.
- `unrealizedPnl` and `unrealizedFunding` remain null for any open position.
  With no open positions they are zero; settled lifecycle funding stays separate.
- `maintenanceMargin` is entry-based and does not need a mark.
- `fairMarketValue`, `liquidationPrice` and `liquidationBuffer` remain null for
  open positions with unverified funding, even when a valid mark exists.
- `zeroFundingEquity`, `zeroFundingLiquidationPrice` and
  `zeroFundingLiquidationBuffer` are explicitly conditional API fields.
- The frontend labels price PnL as excluding funding and shows actual
  liquidation as unavailable. It does not display a scenario as actual risk.

The two initial regressions failed on the old model: maintenance was `2840`
instead of `2800` at the fixture mark, and actual liquidation was `62500`
instead of unavailable. Seven new accounting regressions now pass, including
signed long/short funding effects, overcollateralization, invalid/overflow
inputs, partial mark coverage, API uncertainty and closed positions.

[Browser acceptance](evidence/risk-browser-2026-10-03.json) at
`2026-10-03T09:31:56.182429Z` is explicitly synthetic fixture evidence, separate
from the historical mainnet SDK run. The [inspected screenshot](evidence/risk-browser-2026-10-03.png)
shows price PnL 1000, unavailable total/funding/liquidation and the warning.
Seven browser checks passed, including HTTP 405 for writes, cleared financial
rows/disabled export after a source failure, and no JavaScript errors.

## Reproduction and limits

```powershell
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo fmt --manifest-path tools/perpl-reference/Cargo.toml --check
cargo test --manifest-path tools/perpl-reference/Cargo.toml --locked
cargo clippy --manifest-path tools/perpl-reference/Cargo.toml --locked --all-targets -- -D warnings
py -3 scripts/test_sdk_reference.py
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
```

Local results: 61 root Rust tests passed, one PostgreSQL integration ignored
locally; two SDK unit tests and two Python RPC-gate tests passed. CI separately
executes the disposable PostgreSQL test and validates the unchanged DOT source.
For mainnet reproduction, build the operator and follow its command above with
`--risk-diagnostics`; it requires a retained Envio dataset covering the same
header, not a token, private key or latest-state shortcut.

For browser reproduction, install Playwright/Chromium in a separate test
environment if necessary, then run the fixture service in one terminal:

```powershell
cargo run --locked -- serve fixtures/golden/open-position-as-of.json --bind 127.0.0.1:18085
```

In another terminal, choose new output paths:

```powershell
py -3 scripts/verify_risk_browser.py --output risk-browser.json --screenshot risk-browser.png
```

Canonical point-in-time mark ingestion, complete funding replay and exact
funding checkpoints remain required before actual live risk is available.
Lifetime totals, free balance, global analytics, continuous hosting and live
Nansen integration remain outside this acceptance.
