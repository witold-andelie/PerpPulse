# Two-minute local demo

This script demonstrates synthetic deterministic analytics. It is not a mainnet operating proof.

Start `cargo run -p perppulse -- serve fixtures/golden/open-position-as-of.json` and open `http://127.0.0.1:8081`.

| Time | Action | Narration |
| --- | --- | --- |
| 0:00-0:20 | Show the fixture badge, block cutoff, and coverage | PerpPulse preserves one point in time from protocol totals through wallet positions to source events. This run uses a synthetic fixture. |
| 0:20-0:45 | Show volume 70000, open interest 71000, and position collateral 10000 | These numbers come from native-scale deterministic accounting. Maker fills count volume once. Position collateral is isolated collateral, not total protocol TVL. |
| 0:45-1:10 | Select account 42 | The BTC long is one unit at an entry of 70000. With the fixture's eligible mark of 71000, unrealized PnL is 1000. Settled funding is separate. |
| 1:10-1:30 | Click the position's Evidence button | The position leads back to the canonical block, transaction, and log. The cutoff is preserved. |
| 1:30-1:45 | Apply an out-of-coverage block range | Unsupported ranges fail visibly. Missing live history, marks, and context are also visible rather than converted into zeros. |
| 1:45-2:00 | Download the manifest and open methodology | Input hashes and versioned methodology make replay checkable. Perpl is an external verifier and Nansen is optional context. The product remains read-only. |

For a mainnet demonstration, first rebuild `risk-hotpath-v2`, obtain a covered account, and run `serve-envio`. The live watchlist deliberately withholds global protocol totals and mark-derived risk until those inputs are proven. A public video and final sponsor-catalog validation remain owner-controlled submission steps.
