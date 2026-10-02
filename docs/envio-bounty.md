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
| Meaningful Envio use with real onchain data | HyperIndex canonical event handlers, the Envio-to-Rust adapter, and historical v1 operating observations | Rebuild the tested v2 profile; drive a visible application feature from verified current Monad data |
| Working indexer or pipeline, deployed to Envio Cloud or self-hosted | [Config](../envio/config.yaml), [schema](../envio/schema.graphql), [handlers](../envio/src/EventHandlers.ts), and [run instructions](../envio/README.md) in the public repo | Preserve the old index and record the isolated v2 runtime, catch-up, restart, current coverage, and deployment availability |
| Useful frontend, dashboard, agent, bot, or API consuming the data | Read-only `serve-envio`, canonical lifecycle replay, bounded watchlist UI, event evidence, and manifests | Verify an account whose complete history is inside coverage, connect the live UI, and record the common cutoff and source-failure behavior |
| Short end-to-end video or live link | [Demo plan](demo-script.md) | Publish a live operating link or record the real pipeline; synthetic fixtures do not satisfy this proof |
| Submission explanation of meaningful use | Draft below | Replace pending observations with exact release, runtime, feature, and evidence links |
| Optional Envio demo video, at most two minutes | Two-minute recording plan | Record at most two minutes; satisfy the main competition's video requirement as well |

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

This is a description of the implemented integration, not a claim that its v2
live operating proof is complete. Add the final runtime URL, release SHA,
covered-account evidence and recording before submission. Global historical
analytics, independently verified marks and SDK reconciliation are still pending.
