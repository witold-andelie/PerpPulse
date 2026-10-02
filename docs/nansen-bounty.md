# Nansen bounty requirements and planned feature

The owner supplied the authenticated bounty text on 2026-10-02. Its catalog
title, URL and revision were not supplied. The requirements below record that
text; they are not an independently verified catalog observation.

## Requirement-to-evidence mapping

| Supplied requirement | Current evidence | Remaining work |
| --- | --- | --- |
| Meaningfully integrate at least one API endpoint, MCP tool or CLI command | Budgeted common-label API adapter and mock regression checks in [context.rs](../crates/perppulse/src/context.rs) | Verify permitted Monad requests and selected-field results with real data |
| Nansen drives a core product feature | Optional observed wallet labels are implemented | Build and validate participant discovery/filtering and explanation using those labels; a decorative badge does not establish this requirement |
| Working product, prototype or demo | Read-only fixture app and bounded live-account serving implementation | Complete the actual data-to-feature workflow |
| Explain endpoints, data categories, tools or commands used | Endpoint and observation-time contract documented below | Add sanitized operating evidence, request allowance and verified cost/data-use terms |
| Public repository or technical documentation | Public source and [serving contract](serving-and-evidence.md) | Publish the final feature and reproducible operating steps |
| Short video or live demo; optional submission video at most two minutes | [Demo plan](demo-script.md) | Show the Nansen-driven interaction and its connection to actual Envio facts in the recording |

## Core feature plan

Participant intelligence will let a user select a Nansen label or category and
inspect the covered accounts associated with it, then drill into eligible
position transitions and realized facts from Envio. Labels explain who is in the
selected watchlist. They do not establish protocol-wide cohorts, profitability,
or historical ownership and do not alter accounting. Partial label pages and
missing context remain visible.

The current adapter uses `POST /api/v1/profiler/address/labels` with `chain=monad`.
The [official contract](https://docs.nansen.ai/api/profiler/address-labels), checked
2026-10-02, lists Monad support and current non-premium labels. It does not offer
historical label queries; smart-money and alpha-trader labels require the
separate premium endpoint. `label` is required, while `category` and `kind` are
optional. The adapter preserves a missing category as null; it does not invent
a classification. A mixed categorized/uncategorized mock response verifies this
behavior. No premium request is authorized or planned.

Every context view must show its observation time separately from the ledger
cutoff and attribute Nansen. A label observed later cannot be presented as
historical evidence at that cutoff. Request and credit allowance, public data-use
permission and safe local credential input must be confirmed before paid calls.
No real Nansen call or sponsor acceptance is claimed here.
