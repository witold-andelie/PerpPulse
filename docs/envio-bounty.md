# Envio bounty requirements and evidence

The owner supplied the authenticated bounty text on 2026-10-02 and the
[Envio catalog URL](https://hackathon.monad.xyz/tracks/best-use-of-envio) on
2026-10-03. This document maps that supplied text to implementation and
outstanding operating proof. The browsing tool could not read the linked page;
its current contents and revision are not independently verified. Source
identity and access limits are retained in the
[catalog record](evidence/bounty-sources-2026-10-03.json). The separately supplied
[Perpl task](perpl-bounty.md) requires trading outside the chosen read-only
scope. [Nansen requirements](nansen-bounty.md) need a meaningful context feature
and live verification.

## Eligibility and deliverables

The project must use HyperIndex, HyperSync, or HyperRPC to power a useful
application feature with real onchain data. Installation alone is insufficient.
The bounty is track-agnostic. Envio Cloud and self-hosted pipelines are accepted.

| Supplied requirement | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| Meaningful Envio use with real onchain data | HyperIndex lifecycle and MarkUpdated inputs drive positions, price PnL and source inspection; [v3 mainnet proof](verification-2026-10-03-market-mainnet.md) has 80 matching position and 12 mark/time checks; [covered funding](verification-2026-10-03-funding.md) adds 15 SDK checks for three positions | Lifetime account totals, older checkpoints and nonzero mainnet funding remain unverified |
| Working indexer or pipeline, deployed to Envio Cloud or self-hosted | Public [config](../envio/config.yaml), [schema](../envio/schema.graphql), [handlers](../envio/src/EventHandlers.ts), [run instructions](../envio/README.md); isolated self-hosted mainnet run with preserved restart and independent one-block lag observation | Continuous public availability remains pending; 26.25-second marker-to-ready timing excludes setup and is not full cold-start timing |
| Useful frontend, dashboard, agent, bot, or API consuming the data | Ten actual v3 browser checks; covered funding now enables eligible funded risk and checkpoint inspection, with separate HTTP/SDK verification | New funding UI has no new mainnet browser acceptance; lifetime totals and global analytics remain pending |
| Short end-to-end video or live link | [80-second real operating recording](demo/README.md), English captions and [recording provenance](demo/recording.json) | Check submission-portal codec acceptance or publish a hosted player; no continuous public app URL is claimed |
| Submission explanation of meaningful use | Owner-provided catalog URL, updated draft, source/mainnet evidence and [verified implementation 4b9135b](evidence/ci-4b9135b-2026-10-03.json) | Current catalog revision, final form and owner submission |
| Optional Envio demo video, at most two minutes | VP8 WebM duration 80.20 seconds; actual mainnet event-to-feature workflow | Confirm the main competition and Envio forms accept the same recording |

The optional Envio video does not replace the main competition's required public
operating video. A single video no longer than two minutes is the working plan,
subject to the final submission form accepting the same artifact.

## Judging criteria

| Supplied criterion | Implementation direction | Evidence needed |
| --- | --- | --- |
| Depth of use | Lifecycle schema, canonical marks, effective-dated funding, source provenance, processed coverage and derived Rust positions | Accepted v3 event-to-feature and same-cutoff SDK evidence; aggregate only sufficiently covered data |
| Working product | Read-only app powered by a continuously refreshed source | Live data correctness, catch-up and restart, freshness, quarantine and unavailable-source behavior |
| Originality | Protocol-to-wallet-to-event navigation preserving one cutoff | A concrete useful workflow and an honest explanation of prior work and current limits |
| Craft | Pinned dependencies, readable handlers, explicit schema, bounded adapters and reproducible setup | Green public CI, source/config links and a reproducible live run manifest |

Multichain indexing is a depth example, not a stated eligibility requirement.
PerpPulse remains focused on Monad. A second chain is not required for the
minimum operating submission.

## Submission explanation draft

PerpPulse uses Envio HyperIndex, backed by Monad HyperSync, to index Perpl
Exchange lifecycle events into a provenance-bearing canonical event schema.
A Rust ledger consumes that schema and uses HyperIndex processed-block coverage
to determine which account histories can be replayed at a selected cutoff. The
read-only application connects account positions and realized facts to their
source block, transaction and log, with visible incomplete-history and stale-source
states. Envio therefore supplies the facts behind the user workflow.

The isolated v3 self-hosted run processed 357,925 events at the selected source
observation with a three-block independently observed lag. Four covered
accounts passed ten actual browser checks, including canonical mark-driven
price PnL and mark-event inspection. Official SDK execution at the same
historical end-of-block header matches 80 position checks, 12 exact native
mark/time checks and 16 risk diagnostics across six markets. A real funding
publication stays pending until its future effective block. These results are
recorded in [v3 mainnet verification](verification-2026-10-03-market-mainnet.md).

An [80-second recording](demo/README.md) shows positions, event evidence,
manifest export and advancing coverage from the earlier v2 run; it does not
show the newer v3 marks or funding checkpoints. The later
[funding proof](verification-2026-10-03-funding.md) matches 15 SDK funding/risk
checks for three positions with proven zero unsettled funding. The older BTC
checkpoint, nonzero mainnet funding, lifetime totals, global historical
analytics and continuous public hosting remain pending. The video implementation is
`4b9135b4d9921ec1368d99c65f93ce83d4118ef3`; the indexer source was pinned to
`a8e5254095496e90b32b15cc331782d7cadc11ef`. The owner-provided catalog link is
recorded above. Final submission still needs the current form/revision check
and an accepted recording/player format.
