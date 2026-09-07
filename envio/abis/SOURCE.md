# Exchange ABI subset

`Exchange.events.json` is a subset of events extracted from the MIT-licensed
Perpl dex-sdk ABI:

- Source revision: https://github.com/PerplFoundation/dex-sdk/blob/dbb37c59f6aef03e38d0787eb9c968f59f652617/crates/sdk/abi/dex/Exchange.json
- Verified state semantics: https://github.com/PerplFoundation/dex-sdk/tree/dbb37c59f6aef03e38d0787eb9c968f59f652617/crates/sdk/src/state
- Revision recorded in that repository: `rc_v1.1.7-178-g2273779`
- Source commit: `dbb37c59f6aef03e38d0787eb9c968f59f652617`
- Retrieved: 2026-09-07

Only lifecycle, fill, request, funding, and market-addition events evaluated by
PerpPulse are included. The default `risk-hotpath-v2` config subscribes to the
state-changing lifecycle, liquidation-credit, account/protocol transfer,
maker-fill, funding, and market-addition subset. It includes both no-payment
unwind variants and deliberately excludes request and subjectless taker-fill
logs from persistence. The full ABI is not vendored.
