# Covered position funding and funded risk verification

Observed 2026-10-03T20:16:19.598847Z. This is retained historical mainnet
verification, not current source freshness or a new live browser acceptance.
The thirty-minute Envio launch has ended; the live API continues to fail visibly.

Implementation `73f75a2e3124520bcd028732746c69380ffffe37` is published,
independently remote-SHA checked and accepted by
[analytics CI](https://github.com/witold-andelie/PerpPulse/actions/runs/37151154005)
and [repository policy](https://github.com/witold-andelie/PerpPulse/actions/runs/37151154013).
The [exact acceptance record](evidence/ci-funding-checkpoints-2026-10-03.json)
includes the fresh PostgreSQL, Envio, SDK and DOT jobs. An anonymous pinned
evidence download matched its SHA-256.

## Accepted scope

The [selected SDK execution](evidence/sdk-funding-checkpoints-2026-10-03.json)
uses official SnapshotBuilder at SDK revision
`dbb37c59f6aef03e38d0787eb9c968f59f652617`. Its fixed cutoff is chain 143,
block 110245407, hash
`0x0cd0fdff14fe0a4a6b42ada5e4c33d2581ba103ef92f3fee6c7ddab72ba19c65`,
timestamp 1791050177000 ms, end of block. Accounts are
5382/5383/5384/5385; markets are 1/10/40/70/90. The 180 canonical market
inputs have the same hash as the earlier accepted
[market execution](evidence/sdk-market-execution-2026-10-03.json).
Envio remains pinned to `9e0a3673c8bbfc8c5ccc4a34bdacab4c5ff60269`.

| Position | Canonical funding | SDK funding/risk checks | Evidence boundary |
| --- | --- | --- | --- |
| 5383 / VVV 70 | Proven zero | Five matched | Increase reset at block 110244913 |
| 5383 / PUMP 90 | Proven zero | Five matched | Increase reset at block 110245294 |
| 5384 / BTC 1 | Proven zero | Five matched | Increase reset at block 110244169 |
| 5382 / BTC 1 | Unknown | Zero checks; explicitly unverified | Open reset 109947828 precedes the first observed effective schedule 109948788 |

Each eligible position compares unsettled funding, total unrealized PnL,
equity, maintenance buffer and funded liquidation. The existing 68 position,
16 risk and ten native mark/time checks also match. The execution made
58 gated public RPC requests, with zero rejections or error responses.
An overall matched execution means its nonempty supported checks matched;
it does not turn the unknown BTC checkpoint into a verified result.

## Canonical method

`replay_with_funding_coverage` extends the existing lifecycle ledger; it does
not create another position ledger. The caller supplies complete bounded
market-publication coverage from the Envio reader. Bare replay preserves
unknown funding. The [official funding description](https://docs.perpl.xyz/exchange/funding)
and pinned SDK establish effective-block application and position resets;
the independent implementation is attributed in NOTICE.

A reset at or after the first observed effective schedule bounds the unknown
pre-window pending payment under Perpl's single-pending-schedule rule. Older
resets and missing baselines stay unknown. Funding is applied before every
size/side change in its effective block, including a quiet block at the final
cutoff. Positive payments debit longs and credit shorts. Same-target replacement
requires authorization and the same prior sum, and applies once. Pending
payments retain their units at publication even if the scale changes later.

Open, increase and inversion reset unsettled funding. Partial decrease,
liquidation and deleveraging subtract the event's actual collateral-native
funding settlement. Collateral adjustments preserve it. A nonzero payment
requires an observed scaling anchor; missing units invalidate the amount until
another proven reset. The retained index has no scale-update anchor, so these
three zero results cannot be generalized to nonzero funding amounts.

Checkpoints retain reset, baseline, payment/scale and settlement event IDs,
coverage start and cutoff. Canonical manifests hash the checkpoints alongside
the source events and marks. Eligible funding enables total PnL, equity,
liquidation and buffer; unknown funding leaves those facts null. A wallet
aggregate remains null if any required open-position input is unknown.

The dashboard adds a checkpoint inspection button. HTTP integration verifies
that complete market pages enable funded facts and hash their proof. No new
mainnet browser recording is claimed; the earlier actual browser evidence
and video preserve their original implementation and cutoff.

## Precision diagnosis and tests

The first SDK comparison correctly reported a one-unit equity mismatch for
5384/BTC. A fixed-number regression reproduced it before the verifier fix:
exact price PnL is -0.08951400421142578125; the onchain SDK snapshot loads
deltaPnlCNS as -0.089514. Adding collateral 40.002876 after component rounding
gives 39.913362, whereas truncating the final exact equity gives 39.913361.

The verifier now projects price and funding separately to collateral units
before addition. Buffer also uses the previously validated SDK-width
canonical maintenance projection. Exact canonical values remain in every
check; production precision is unchanged. Funded liquidation is compared at
price ticks. This is a documented representation contract, not a widened
tolerance. The original full historical comparison passed after the fix.

Reproducible local checks: 85 Rust tests passed, one disposable PostgreSQL
integration ignored locally; 12 funding tests include block ordering,
settlements/resets, publication scale, overwrite, missing data, wallet
aggregation and exact native overflow. Fourteen HTTP adapter tests include
the covered reset proof. Five SDK tests and three RPC gate/redaction tests
pass. Root/operator formatting and Clippy, locked operator build, publication
policy and the credential-free evidence audit pass. PostgreSQL and DOT are
also checked by the publication workflow; its result is recorded separately.

## Reproduction and remaining work

Use the retained v3 Hasura database and its local process environment
authentication. Build the isolated operator as documented in
[its README](../tools/perpl-reference/README.md), then run:

```powershell
py -3 scripts/run_sdk_reference.py `
  --binary tools/perpl-reference/target/debug/perppulse-perpl-reference.exe `
  --block 110245407 `
  --block-hash 0x0cd0fdff14fe0a4a6b42ada5e4c33d2581ba103ef92f3fee6c7ddab72ba19c65 `
  --accounts 5382,5383,5384,5385 --markets 1,10,40,70,90 `
  --graphql http://127.0.0.1:18086/v1/graphql `
  --registry fixtures/protocol/mainnet-registry-2026-10-03.json `
  --risk-diagnostics --market-diagnostics --funding-checkpoints --allowance 96 `
  --output sdk-funding-reference.json

py -3 scripts/check_market_evidence.py
cargo test --locked
```

SDK calls pin the block number and verify its header before/after acquisition;
they do not bind every subcall through EIP-1898. No SDK or REST funding scale,
mark or balance is adopted as canonical history. Two bounded public log probes
were rejected with HTTP 413; they supplied no bootstrap facts. Completing the
older BTC checkpoint needs canonical historical scale and pending-schedule
anchors. Nonzero mainnet checkpoint reconstruction, lifetime totals, global
analytics, continuous public hosting and live Nansen remain open. No wallet
key, transaction, paid API or cloud action was used.
