# Envio bounty requirements and evidence

The owner supplied the authenticated bounty text on 2026-10-02. This document
maps that supplied text to the implementation and outstanding operating proof.
The authenticated catalog URL and bounty revision were not supplied, so this is
not an independently verified catalog observation. The separately supplied
[Perpl task](perpl-bounty.md) requires trading outside the chosen read-only
scope. [Nansen requirements](nansen-bounty.md) need a meaningful context feature
and live verification.

## Eligibility and deliverables

The project must use HyperIndex, HyperSync, or HyperRPC to power a useful
application feature with real onchain data. Installation alone is insufficient.
The bounty is track-agnostic. Envio Cloud and self-hosted pipelines are accepted.

| Supplied requirement | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| Meaningful Envio use with real onchain data | HyperIndex canonical handlers and Rust replay; [restart and browser evidence](verification-2026-10-03-envio.md) records 69,317 events, four covered accounts, shared-cutoff export and event drill-down | Complete full SDK reconciliation before claiming independently verified account PnL or risk |
| Working indexer or pipeline, deployed to Envio Cloud or self-hosted | Public [config](../envio/config.yaml), [schema](../envio/schema.graphql), [handlers](../envio/src/EventHandlers.ts), [run instructions](../envio/README.md); isolated self-hosted mainnet run with preserved restart and independent one-block lag observation | Continuous public availability remains pending; 26.25-second marker-to-ready timing excludes setup and is not full cold-start timing |
| Useful frontend, dashboard, agent, bot, or API consuming the data | Four-account actual browser acceptance; source-event evidence, cutoff-preserving UI manifest, visible unavailable marks, invalid-range rejection and stopped-source failure | Eligible marks, full SDK correctness verification and global analytics remain outside this accepted account-only path |
| Short end-to-end video or live link | [80-second real operating recording](demo/README.md), English captions and [recording provenance](demo/recording.json) | Check submission-portal codec acceptance or publish a hosted player; no continuous public app URL is claimed |
| Submission explanation of meaningful use | Updated draft below, linked source/mainnet evidence and [verified implementation 4b9135b](evidence/ci-4b9135b-2026-10-03.json) | Catalog URL/revision, final form and owner submission |
| Optional Envio demo video, at most two minutes | VP8 WebM duration 80.20 seconds; actual mainnet event-to-feature workflow | Confirm the main competition and Envio forms accept the same recording |

The optional Envio video does not replace the main competition's required public
operating video. A single video no longer than two minutes is the working plan,
subject to the final submission form accepting the same artifact.

## Judging criteria

| Supplied criterion | Implementation direction | Evidence needed |
| --- | --- | --- |
| Depth of use | Non-trivial lifecycle schema, source provenance, processed-coverage metadata, derived Rust positions and deterministic analytics | A real v2 event-to-feature trace and exact-cutoff replay evidence; aggregate only sufficiently covered data |
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

The bounded self-hosted run resumed with preserved coverage and 69,317 events.
Four accounts created inside that coverage passed actual browser acceptance.
An [80-second recording](demo/README.md) shows positions, event evidence,
manifest export and advancing coverage. A separate selected position diagnostic
matched 23 fields but is not a full SDK replay at an identical log cutoff.
Global historical analytics, accounting marks, full SDK reconciliation and
continuous public hosting remain pending. The verified implementation is
`4b9135b4d9921ec1368d99c65f93ce83d4118ef3`; the indexer source was pinned to
`a8e5254095496e90b32b15cc331782d7cadc11ef`. Final submission still needs
confirmed catalog/form links and an accepted recording/player format.
