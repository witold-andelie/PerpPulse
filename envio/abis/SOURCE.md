# Exchange ABI subset

`Exchange.events.json` is a subset of events extracted from the MIT-licensed
Perpl dex-sdk ABI:

- Source revision: https://github.com/PerplFoundation/dex-sdk/blob/dbb37c59f6aef03e38d0787eb9c968f59f652617/crates/sdk/abi/dex/Exchange.json
- Verified state semantics: https://github.com/PerplFoundation/dex-sdk/tree/dbb37c59f6aef03e38d0787eb9c968f59f652617/crates/sdk/src/state
- Revision recorded at that source commit: `rc_v1.1.7-203-g0e5902dd`
- Source commit: `dbb37c59f6aef03e38d0787eb9c968f59f652617`
- Retrieved: 2026-09-07

Only lifecycle, fill, request, funding, mark, scaling and market-addition events evaluated by
PerpPulse are included. The default `risk-hotpath-v3` config subscribes to the
state-changing lifecycle, liquidation-credit, account/protocol transfer,
maker-fill, funding, mark, scaling and market-addition subset. It includes both no-payment
unwind variants and deliberately excludes request and subjectless taker-fill
logs from persistence. The full ABI is not vendored.

The v2 ABI fingerprint is SHA-256 of the UTF-8 subset with CRLF converted to
LF, matching Git's `*.json text eol=lf` policy:
`sha256:b98e14a49e4201d71feeae380261784fc8872aa45b201d193194c6c5d56adbf1`.
The earlier v2 fingerprint `16b3a481...` hashed a mixed-line-ending Windows
worktree, rather than the published Git blob. It is rejected for replay; rebuild
any index produced with that fingerprint in an isolated database. The legacy
v1 inspection contract is unchanged. ABI event semantics did not change.


The v3 subset adds `MarkUpdated` and `FundingSumScalingExpUpdated` from the same
pinned MIT source. Its LF-normalized fingerprint is
`sha256:8858f1c8a42836c58459ec37a89359deb015b23e0840c7f21efddf1c3b7315e6`.
It uses canonical-event-v5 / envio-handlers-v5 / exchange-classifier-v4.
The historical v2 subset and accepted fingerprint are retained in Git at
50f56fbf8d5c3ebd86f3df4672cf51e238b6202f. Never relabel or migrate that retained
dataset in place; initialize v3 in a separate database/network.
