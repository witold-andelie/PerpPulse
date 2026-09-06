# Exchange ABI subset

`Exchange.events.json` is a subset of events extracted from the MIT-licensed
Perpl dex-sdk ABI:

- Source: https://github.com/PerplFoundation/dex-sdk/blob/main/crates/sdk/abi/dex/Exchange.json
- Revision recorded in that repository: `rc_v1.1.7-178-g2273779`
- Retrieved: 2026-09-02

Only lifecycle, fill, request, funding, and market-addition events evaluated by
PerpPulse are included. The default `risk-hotpath-v1` config subscribes to the
state-changing lifecycle, maker-fill, funding, and market-addition subset. The
full ABI is not vendored.
