# Envio resume, live account workflow and exact entry precision

This verification was completed on October 3, 2026 local time; operating
observations occurred on October 2 UTC. It extends the
[first mainnet run and reader corrections](verification-2026-10-02-live-v2.md).
The product remains read-only.

## Restart and coverage

The operator restarted the isolated runtime through hidden terminal input.
The same database volume, initialized marker, source commit `a8e5254`, quick
configuration hash and start block 109944714 were preserved. Envio reported
readiness at `2026-10-02T22:30:33.941Z`. At `22:31:32.156733Z`, it had 69,317
events and processed block 110013892. Independent Monad RPC reported chain
143 and head 110013893: a processed lag of one block. Previous account-creation
events survived, and the resumed index included newly created account 5386.
See [selected restart evidence](evidence/live-resume-2026-10-02.json).

The initial marker-to-readiness interval was 26.25 seconds. The marker was
written after setup and immediately before starting the indexer. This interval
excludes authentication, code generation and migration, and is not a complete
operator cold-start measurement. Both runs were limited to thirty minutes.
The stopped-source failure checks remain valid; this is not continuous hosting.

## Live browser acceptance

The actual browser served accounts 5382-5385 from Envio. All four accounts were
created inside the verified coverage. At the final acceptance observation, the
selected cutoff was block 110018014; the exported UI manifest used that same
cutoff. The browser verified account switching, source-event drill-down,
preserved cutoffs, out-of-range rejection, visible missing marks, HTTP 503 for
global protocol totals, HTTP 405 for writes, and no JavaScript errors.

[Browser checks](evidence/live-browser-2026-10-02.json) and the
[selected manifest](evidence/live-account-manifest-2026-10-02.json) retain the
source and methodology hashes. Export was tested through the actual UI download,
which belongs to its immutable snapshot; a separate later API request is not
used as evidence of a matching export.

## Independent position diagnostic and precision correction

Using the official Perpl SDK ABI at
[`dbb37c5`](https://github.com/PerplFoundation/dex-sdk/tree/dbb37c59f6aef03e38d0787eb9c968f59f652617),
eleven bounded read-only RPC requests verified chain and block identity,
inspected later Exchange logs, and read selected market/position tuples through
`getPerpetualInfoV2(uint256)` and `getPositionV2(uint256,uint256)`. Their selectors
are `9b335b9e` and `ea3196ec`. The artifact hash and selected decoded fields are
retained in [the diagnostic](evidence/sdk-position-diagnostic-2026-10-02.json).
No signer or SDK state engine was used.

Reproduction requires the pinned SDK ABI, the live application commands in the
earlier verification, and one immutable manifest/wallet snapshot. Verify RPC
chain ID, block hash and timestamp against that manifest, read each selected
position and market at its block number, and decode the tuple fields defined
by the pinned ABI. Keep the end-of-block reference boundary explicit. Do not
reuse the archived values against a new cutoff or attach this diagnostic as an
exact-cutoff normalized SDK scorecard.

All 23 selected comparisons across five positions matched at block 110018014.
Open positions compared native size, collateral, direction, stored price,
Q16 residue and effective entry; closed positions compared closed state, zero
size and zero collateral. Account 5383's MON position had stored price 32086,
residue 32964 and effective entry `0.03208550299072265625`.

The first diagnostic exposed loss of the weighted-entry residue. A regression
through the GraphQL adapter reproduced it before correction. Methodology v4
now preserves V2 Q16 input, including the long ceiling and short floor
corrections. Integer construction keeps the exact decimal or returns a visible
precision error. Reductions preserve residue; repricing and inversion reset it.
Native price and residue are also exported for source verification. Semantics
follow the MIT-licensed SDK; attribution is in [NOTICE](../NOTICE).

The reference calls read the end of the block, while the canonical snapshot
ends at log 48. Later Exchange logs exist, including oracle updates. Matching
these selected fields does not establish an identical log-cutoff SDK replay,
account lifetime PnL, funding, fees, free balance, eligible marks or risk facts.
The diagnostic does not supply accounting marks. Full SDK reconciliation and
point-in-time mark ingestion remain open.

## Verification and demonstration

```powershell
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
```

Local verification passed 49 Rust tests; one disposable PostgreSQL integration
test was ignored locally. The new regressions cover long/short residue, missing
or invalid V2 input, increase/reduction, repricing, inversion and unrepresentable
precision. No dependency or Envio schema/handler change was made.

The earlier publication `3bd9d1a` passed
[analytics CI](https://github.com/witold-andelie/PerpPulse/actions/runs/37072279448)
and [publication policy](https://github.com/witold-andelie/PerpPulse/actions/runs/37072279467).
These runs precede the Q16 correction; its publication acceptance must identify
the new exact implementation SHA separately.

The [80-second recording](demo/README.md) shows the actual end-to-end account
workflow, manifest export and advancing coverage, with English captions.
The WebM is VP8, 1440 by 900, and 80.20 seconds. FFmpeg duration inspection and
visual checks at seconds 20, 55 and 76 are recorded with its hash. No synthetic
financial inputs, credentials or owner addresses appear in the recording.

The public source/recording can be reviewed without access to the running local
services. Continuous public deployment, portal format acceptance, complete SDK
reconciliation, point-in-time marks, global totals, Nansen live data and final
submission remain pending. No cloud provisioning or paid Nansen calls occurred.
