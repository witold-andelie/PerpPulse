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

## Planned live Envio recording

The owner-supplied Envio bounty allows an optional video of at most two minutes
and requires an end-to-end demonstration of useful real onchain data. This plan
must be recorded only after v2 live acceptance; the fixture sequence above is
development evidence.

| Time | Action | Required operating evidence |
| --- | --- | --- |
| 0:00-0:20 | Show the Monad source and self-hosted HyperIndex coverage | Chain 143, `risk-hotpath-v2`, start and processed blocks, independently checked head and lag |
| 0:20-0:45 | Open the live application and a covered account | Actual Envio data, live source badge, an account-creation event inside coverage, and one shared cutoff |
| 0:45-1:10 | Explain a position transition or realized fact | Deterministic replay from eligible history, native decimal scales, and visible unavailable fields |
| 1:10-1:35 | Open event evidence and its transaction | The feature connects to the canonical block hash, transaction and log at the displayed cutoff |
| 1:35-1:50 | Show incomplete-history or source-failure behavior | Unsupported facts stay unavailable and missing sources fail visibly |
| 1:50-2:00 | Open the manifest and public repo | Exact release, config, schema, handlers, source/input hashes and reproducible setup |

First rebuild `risk-hotpath-v2`, obtain a covered account, and run `serve-envio`.
The live watchlist withholds global protocol totals and mark-derived risk until
those inputs are proven. A public video and final sponsor-catalog validation
remain owner-controlled submission steps. A current live index alone does not
establish SDK reconciliation or complete arbitrary-wallet history.
