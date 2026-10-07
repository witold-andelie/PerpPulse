# Two-minute local demo

This script demonstrates synthetic deterministic analytics. It is not a mainnet operating proof.

Start `cargo run -p perppulse -- serve fixtures/golden/watchlist-cohort.json` and open `http://127.0.0.1:8081`.
The container image serves the same fixture by default.

| Time | Action | Narration |
| --- | --- | --- |
| 0:00-0:15 | Show the fixture badge, block cutoff and coverage | PerpPulse fixes one point in time from protocol totals through wallets to source events. This cohort is synthetic: seven accounts, three markets, ten days. |
| 0:15-0:40 | Read the three Risk Pulse cards | Account 102's BTC long is 0.73% from its funded liquidation price, MON liquidated 65% of its open interest in 24 hours, and account 105 uses 96.6% of ETH's initial leverage limit. Each card names its rule, basis and cutoff. |
| 0:40-0:55 | Click Inspect evidence on the first card | The signal carries its rule hash, inputs and the position, mark and funding-reset events. Funding is proven because coverage starts at deployment. |
| 0:55-1:10 | Scroll to Protocol flows and state | 24-hour, 7-day and 30-day windows are complete because history starts at deployment; each window hashes its event IDs. ETH is fully long and MON fully short. |
| 1:10-1:25 | Show alerts filters and stress scenarios | A 5% BTC drop breaches only account 102; a 20% rise breaches 103's short. MON funding is deliberately uncovered, so it stays a visible data-quality signal rather than a risk fact. |
| 1:25-1:45 | Compare accounts 101, 102 and 103 | Leverage and PnL percentiles rank only known values; overlap shows 101 and 102 are both long BTC while 103 is short. Nansen labels would filter this list without changing numbers. |
| 1:45-2:00 | Open account 102, then download the manifest | Navigation keeps the cutoff. Input hashes and the versioned methodology make every number replayable. The product remains read-only. |

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
