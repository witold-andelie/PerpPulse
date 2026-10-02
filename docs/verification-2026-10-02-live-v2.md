# Mainnet v2 indexing and reader corrections

The isolated Envio runtime indexed real Monad chain 143 events on 2026-10-02.
Its 18 source files were verified against commit
`a8e5254095496e90b32b15cc331782d7cadc11ef`. PostgreSQL and Hasura used a
separate named volume and loopback ports 15434 and 18084. The legacy index and
the synthetic preview were preserved.

## Observed mainnet evidence

Selected observations, rather than complete provider responses, are recorded in
[the evidence summary](evidence/live-v2-2026-10-02.json).

- Coverage began at block 109944714; the first matched event was at 109944715.
- Envio reported readiness at `2026-10-02T20:55:40.982Z`.
- At `20:57:27.834960Z`, 58,539 events were stored, processed/source block was
  109995190, and an independent public Monad RPC reported chain 143 and head
  109995193. The observed processed lag was three blocks.
- After the bounded run stopped, the retained database contained 61,636 events
  at processed block 110000666. Its last source-height observation was
  110000665. This stopped state is unavailable to the live reader.
- Accounts 5382, 5383, 5384, and 5385 were created inside this coverage. Each had
  a successful CLI replay observation after the direction corrections. These
  were separate observations, not a synchronized reconciliation scorecard.

The runtime used `canonical-event-v4`, `envio-handlers-v4`,
`exchange-classifier-v3`, `risk-hotpath-v2`, and ABI fingerprint
`sha256:b98e14a49e4201d71feeae380261784fc8872aa45b201d193194c6c5d56adbf1`.

The hidden-input credential remained in process memory. No credential,
generated quick configuration, raw response, or local progress file is part of
the public evidence. Precise cold-start duration was not retained and is not
claimed by this first-run observation. The later
[resume/browser verification](verification-2026-10-03-envio.md) records a
preserved restart and a narrowly defined marker-to-readiness interval.

## Reproduced failures and corrections

The real reader rejected wire-side zero at a position opening. A regression
through the HTTP GraphQL adapter reproduced that failure and also showed wire
side one incorrectly becoming a long position. The pinned
[Perpl SDK](https://github.com/PerplFoundation/dex-sdk/blob/dbb37c59f6aef03e38d0787eb9c968f59f652617/crates/sdk/src/state/position.rs)
defines long as zero and short as one. The adapter now verifies the raw column
against the payload, maps to ledger long=1/short=2, retains the raw payload, and
rejects other wire values.

Account 5383 then exposed an inversion failure: the ledger compared the new
direction to the previous direction before applying the flip. A lifecycle
regression reproduced it. Inversion now checks the previous size and collateral,
requires a changed direction, and applies the resulting state. Tests reject
unchanged direction, wrong starting size, and wrong starting collateral.

Envio 2.32.6 separately polls source height and commits batch progress. During
active indexing, committed progress was sometimes one or two blocks ahead of
the last source-height poll. Retrying an entirely new cutoff kept chasing this
moving difference. A moving-watermark HTTP mock reproduced that failure. The
reader now keeps the original immutable cutoff and seeks at most three later
source-height observations, 200 ms apart, that cover it. It still rejects changed
coverage, regressed progress, persistent inconsistency, and a stale or impossible
cutoff against the independent Monad RPC.

These accounting and coverage rules are recorded in methodology v3. A source
failure also changes the page badge to `SOURCE UNAVAILABLE`, clears financial
rows, and disables manifest export.

## Reproduction and checks

With the isolated Envio source running, use the documented local Hasura test
credential and public RPC. This credential is a disposable local default.

```powershell
$env:HASURA_GRAPHQL_ADMIN_SECRET = 'testing'
cargo run --locked -p perppulse -- envio-account 5383 --graphql-url http://127.0.0.1:18084/v1/graphql --page-size 100 --max-events 10000
cargo run --locked -p perppulse -- serve-envio --accounts 5385,5384,5383,5382 --graphql-url http://127.0.0.1:18084/v1/graphql --bind 127.0.0.1:18082 --refresh-seconds 5 --page-size 100 --max-events 10000
```

Open `http://127.0.0.1:18082/`. The API independently verifies the public Monad
head. A stopped or stale source must return HTTP 503 instead of old facts.

The local final verification passed:

```powershell
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
py -3 scripts/check_repository_policy.py --working-tree
git diff --check
```

There were 47 passing Rust tests and one ignored disposable-PostgreSQL test.
That database test was not rerun locally for this reader correction. Five HTTP-adapter
tests cover both directions, unknown direction, moving source height, persistent
inconsistency, stale coverage, incorrect subjects, and event limits. The
inversion regression exercises the ledger lifecycle.

Implementation `ab8b06b954ebdf6689b2aa1b5ec450ee40cef74a` subsequently passed
[public CI](https://github.com/witold-andelie/PerpPulse/actions/runs/37068601422),
including a fresh disposable PostgreSQL integration test, Envio generation,
typechecking/tests and DOT validation. The separate
[repository policy run](https://github.com/witold-andelie/PerpPulse/actions/runs/37068601478)
also passed. [Selected CI acceptance evidence](evidence/ci-ab8b06b-2026-10-02.json)
records the exact implementation and job IDs.

The live API returned `mode=live`, as-of and processed block 110000196 before
the run stopped. At `21:29:53.875136Z`, an actual browser check of the stopped
source verified HTTP 503, a visible source error, empty financial rows, and
disabled export. Successful browser acceptance of the final corrected reader
and restart recovery were pending at this first-run observation and subsequently
passed in the [follow-up verification](verification-2026-10-03-envio.md).

No independent SDK account or position reconciliation is claimed. Point-in-time
marks, free balance, global protocol totals, Nansen live enrichment and hosted
deployment remain unverified or unavailable. The follow-up adds a bounded real
operating recording and a selected position diagnostic without closing full SDK
reconciliation. No
orders, signing, custody, paid Nansen calls, or cloud provisioning occurred.
