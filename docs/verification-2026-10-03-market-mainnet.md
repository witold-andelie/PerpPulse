# Mainnet canonical market acceptance

Envio v3 now powers real price PnL and source-event inspection in the read-only
dashboard. The bounded self-hosted observation used indexer implementation
`9e0a3673c8bbfc8c5ccc4a34bdacab4c5ff60269`, the exact v3 provenance tuple in
[the implementation record](verification-2026-10-03-market-v3.md), and a separate
database beginning at block 109944714. The retained v2 archive was preserved.
This is a workstation operating proof, not continuous public hosting.

## Operating evidence

At 2026-10-03T17:56:24.823364Z, Envio had processed 357,925 events through block
110245427. Its observed source height was also 110245427; independent public
RPC reported 110245430, a three-block lag. The
[selected source observation](evidence/live-market-source-2026-10-03.json)
records coverage, the independent head, source revision and config hash.

The [actual browser record](evidence/live-market-browser-2026-10-03.json) at
18:07:32.786379Z has four eligible accounts and four open positions at block
110247586/log 74. Ten checks passed: account navigation, canonical mark-driven
price PnL, unknown funding/actual risk, native mark source inspection, preserved
cutoff, immutable UI export, invalid-range rejection, global totals 503, writes
405 and no JavaScript errors. The
[screenshot](evidence/live-market-browser-2026-10-03.png) was visually inspected;
the [downloaded manifest](evidence/live-market-manifest-2026-10-03.json) matches
the browser's cutoff and input hashes. Its reconciliation is explicitly
unverified: the separate SDK proof below uses a different end-of-block cutoff.

The masked-input launcher ended at its bounded authentication window. The
database remains available for historical verification. Retained `isReady`
metadata is not fresh-source evidence after shutdown.
The [actual stopped-source browser check](evidence/live-market-stopped-2026-10-03.json)
then passed four checks: snapshot 503, source-unavailable badge, cleared financial
rows and disabled export, with no JavaScript errors. Its
[viewport screenshot](evidence/live-market-stopped-2026-10-03.png) was inspected.

## Independent SDK comparison

The official SDK `SnapshotBuilder::build`, pinned to MIT-licensed revision
`dbb37c59f6aef03e38d0787eb9c968f59f652617`, executed at block 110245407,
hash `0x0cd0fdff14fe0a4a6b42ada5e4c33d2581ba103ef92f3fee6c7ddab72ba19c65`,
timestamp 1791050177000 ms, end-of-block. Both reads verified that header.

| Selected proof | Scope | Matching checks | Read-only RPC requests |
| --- | --- | --- | --- |
| [Primary execution](evidence/sdk-market-execution-2026-10-03.json) | Accounts 5382-5385; markets 1, 10, 40, 70, 90 | 68 position, 10 exact native mark/time, 16 risk diagnostic | 58 |
| [SOL execution](evidence/sdk-market-sol-2026-10-03.json) | Same accounts and header; market 31 | 12 position, 2 exact native mark/time | 32 |

Together these cover 24 explicit account/market states and all six markets
used by the selected retained account histories. Four open positions have
matching price PnL at collateral units, maintenance at SDK arithmetic, and
reference-funded liquidation scenarios at price ticks. The last scenario is
a verifier calculation; actual canonical funded liquidation remains null.
All requests passed the fixed-block, contract and method gate with zero
rejections or RPC errors. The SDK pins block numbers and verifies the header
before/after acquisition; it does not bind every subcall with EIP-1898.

Canonical marks come exclusively from Envio MarkUpdated events. The proof
retains source event IDs, native integers, timestamps, input/mark hashes,
source artifact hashes and binary hash. SDK marks are comparison observations.
No SDK or REST mark is adopted into the product ledger.

### SDK arithmetic representation

Two actual entries exposed the upstream `fastnum::UD64` coefficient-width
flooring. The exact VVV entry is `27.93071435089111328125`; the SDK displays
`27.93071435089111328`. This is not a fixed decimal-place policy. The verifier
projects the canonical native stored entry and Q16 residue through the pinned
SDK's UD64 Floor representation, then uses UD128 for the margin comparison.
It retains the exact ledger entry and maintenance alongside the projection.
Production arithmetic is unchanged. Regression tests failed before the
projection and passed afterward, including rejection of a one-Q16-unit change.
The nested verifier declares the already locked `fastnum = 0.7.5` directly;
no dependency version or root lockfile changed. Attribution is in
[NOTICE](../NOTICE) and the [retained MIT license](../LICENSES/Perpl-dex-sdk.txt).

## Registry adoption and funding causality

The September registry correctly rejected newly used market 70. A separate
[dated registry](../fixtures/protocol/mainnet-registry-2026-10-03.json) adds only
VVV, preserving all previous market definitions and exclusions. SDK execution
at the selected historical header independently confirms VVV price/size scales
4/2 and margin inverses 3/10. The SDK names market 31 `SOL_v2`; the existing
display registry names that same ID `SOL`. NEAR 100 and UNI 110 remain unsupported
and fail closed. A metadata observation never supplies accounting marks.

The [earlier execution](evidence/sdk-funding-pending-2026-10-03.json) independently
verifies block 110240124/hash
`0xdae4d43d111f8266631e0fb097e1204e66622960d2035b43ffed64b234000b44`.
BTC funding published in that block targets 110240202, 78 blocks later. The
same source event is pending at publication and active at 110245407, with
unchanged signed native payment/sum. This execution has 32 matching checks and
33 gated RPC requests. It proves scheduling, not a position funding amount;
`positionFundingAmount`, open-position funding, total PnL, actual equity and
actual liquidation remain null until checkpoint reconstruction is verified.

## Reproduction and failure boundaries

The [SDK operator instructions](../tools/perpl-reference/README.md) include the
new `--market-diagnostics` mode. Historical market reads require end-of-block,
the target inside both committed and observed-source coverage, stable retained
event identity, exact provenance and nonregressing final watermarks. They are
an explicit verification API; live serving continues to reject stale,
unready or inconsistent sources. A regression first reproduced the stopped
source failure, then verified archival success, live refusal, lookahead refusal,
partial-log refusal and coverage-regression refusal.

```powershell
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --manifest-path tools/perpl-reference/Cargo.toml --locked
cargo clippy --manifest-path tools/perpl-reference/Cargo.toml --locked --all-targets -- -D warnings
py -3 scripts/test_sdk_reference.py
py -3 scripts/check_market_evidence.py
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
```

Results: 72 root tests passed, one PostgreSQL integration ignored locally;
four SDK and three Python gate/redaction tests passed. Root/operator formatting,
Clippy, locked build and selected-evidence audit passed. Envio sources/schema
and OPM diagrams did not change; their previous 19-test Linux acceptance remains
recorded, and publication CI independently repeats Envio, PostgreSQL and DOT
checks. No wallet, order, custody, Nansen, cloud or billable action occurred.
Global analytics, lifetime totals, continuous hosting and live Nansen remain open.
