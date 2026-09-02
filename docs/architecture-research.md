# Architecture Research Notes

Snapshot date: 2026-09-02.

This document records the public projects and primary documentation that materially changed the PerpPulse architecture. Star counts are only a popularity signal and will become stale; the applied engineering pattern is the important part. Application code is original. The Envio indexer includes a subset of Exchange event definitions extracted from the MIT-licensed Perpl dex-sdk ABI; see `envio/abis/SOURCE.md`.

## Public project benchmark

| Project | Stars at snapshot | Pattern applied to PerpPulse |
| --- | ---: | --- |
| [Freqtrade](https://github.com/freqtrade/freqtrade) | 53,955 | Reproducible performance reports, open-trade visibility, drawdown, profit factor, Sharpe/Sortino-style test discipline |
| [Cube](https://github.com/cube-js/cube) | 20,763 | Define metrics once in a versioned semantic contract; expose consistent APIs and bounded pre-aggregations |
| [SubQuery](https://github.com/subquery/subql) | 18,749 | Multichain schema and query separation |
| [Blockscout](https://github.com/blockscout/blockscout) | 4,663 | Separate live and catch-up indexing, retry failed batches, and refetch inconsistent ranges |
| [Rotki](https://github.com/rotki/rotki) | 3,999 | One accounting ledger, decoded lifecycle events, point-in-time prices, and oracle provenance |
| [Graph Node](https://github.com/graphprotocol/graph-node) | 3,145 | Deterministic versus nondeterministic failures, reorg-aware processing, and reproducible proofs |
| [Dune Spellbook](https://github.com/duneanalytics/spellbook) | 1,514 | Incremental models, seed fixtures, unique/non-null tests, and CI validation |
| [DefiLlama Adapters](https://github.com/DefiLlama/DefiLlama-Adapters) | 1,250 | Transparent protocol methodology and external comparison contracts |
| [Ponder](https://github.com/ponder-sh/ponder) | 1,124 | Typed EVM indexing backed by PostgreSQL with GraphQL and SQL access |
| [Aave V3 Core](https://github.com/aave/aave-v3-core) | 1,117 | Explicit liquidation and risk invariants as a reference for test design, not copied financial formulas |
| [Envio HyperIndex](https://github.com/enviodev/hyperindex) | 546 | Sponsor-aligned historical and real-time indexing with reorg handling and generated GraphQL |

[DefiLlama Dimension Adapters](https://github.com/DefiLlama/dimension-adapters) is also directly relevant despite a lower star count. Its guidelines shaped the metric contract: prefer onchain logs, document methodology and breakdowns, expose failures, count taker-side perpetual volume consistently, and distinguish fees from protocol revenue.

## Sponsor and protocol findings

### Envio

Envio is a core data-plane dependency, not a logo-only integration. HyperSync performs historical range retrieval, while HyperIndex or HyperRPC supports live event processing. The system records checkpoints, finality, handler and schema versions, detects gaps and reorgs, quarantines affected ranges, and deterministically replays them. The product API is derived from the canonical Envio-backed event ledger.

Primary source: [Envio HyperIndex](https://github.com/enviodev/hyperindex).

### Perpl

Perpl exposes public REST and market WebSocket data including context, candles, funding, oracle and mark prices, bid and ask, open interest, TVL, trades, order books, and a heartbeat sequence. A heartbeat gap triggers reconnect and resnapshot. Authenticated account history cannot be treated as a global wallet database; global wallet reconstruction must come from onchain events.

The `dex-sdk` can produce exchange snapshots at a block and traces from initial state through events to final state. PerpPulse uses those outputs as golden reconciliation evidence, not as a second source of truth.

Primary sources: [Perpl API documentation](https://github.com/PerplFoundation/api-docs), [Networks and configuration](https://docs.perpl.xyz/resources/for-developers/networks-and-configuration.md), [Overview](https://docs.perpl.xyz/resources/for-developers/overview.md), [Margin](https://docs.perpl.xyz/exchange/margin.md), [Perpl dex-sdk](https://github.com/PerplFoundation/dex-sdk), and live `GET https://app.perpl.xyz/api/v1/pub/context` on 2026-09-02.

Verified mainnet facts used by the first implementation slice:

- Exchange proxy `0x34B6552d57a35a1D042CcAe1951BD1C370112a6F` on chain 143, deployment block `54773010`.
- AUSD collateral `0x00000000eFE302BEAA2b3e6e1b18d08D69a9012a`, 6 decimals.
- Fee rates are per 100,000 (0.1 bps). Margin fractions are hundredths used as `IMR = N / IMF`.
- Live markets on 2026-09-02 include BTC, MON, ETH, SOL 31, HYPE, ZEC, plus LIT 60 and PUMP 90, which the static docs table omitted. Legacy SOL 30 remains excluded.
- Position lifecycle is reconstructed from Exchange logs (`PositionOpened` through close/liquidation). `TakerOrderFilled` has no `perpId`; per-market volume uses `MakerOrderFilled`.
- Envio HyperSync supports Monad chain 143 at https://143.hypersync.xyz.

### Nansen

Nansen supports Monad address labels and provides Profiler and Smart Money surfaces. PerpPulse caches enrichment with observed-at time, endpoint, provenance, cache age, credit budget, and warning state. Nansen context may modify explanation, cohort comparison, and ranking, but never the canonical position ledger or financial arithmetic. Missing Nansen data results in a visible degraded mode rather than false empty facts.

Primary sources: [address labels](https://docs.nansen.ai/api/profiler/address-labels), [API overview](https://docs.nansen.ai/api/overview), [Smart Money](https://docs.nansen.ai/api/smart-money), and [data methodology](https://docs.nansen.ai/guides/data-methodology-and-technical-reference).

## Applied design improvements

1. One canonical position lifecycle ledger; no dual-ledger ambiguity.
2. Stable event, request, scenario, and result identities with input hashes.
3. Strict as-of cutoffs for market references, wallet accounting, replay, and evaluation.
4. Fail-closed quality gates that reject stale, incomplete, non-finite, or unreconciled signals.
5. Visible quarantines and errors instead of successful empty outputs.
6. Golden wallets and full lifecycle fixtures, including partial close, funding, fees, liquidation, missing ranges, and reorgs.
7. Deterministic stress scenarios that quantify sensitivity without pretending to forecast prices.
8. Sponsor ablation showing that Envio is structurally necessary and Nansen adds product-level decision context.
9. Signal-first navigation from protocol to market to wallet to event evidence.
10. Append-only run manifests containing as-of cutoff, inputs, outputs, versions, Git SHA, timestamps, and errors.

## Official submission constraint

The [Metropolis Hackathon Rules and Guidelines v2](https://hackathon.monad.xyz/api/v1/policies/current) require public source code on GitHub under an OSI-approved license, complete setup instructions, attribution, build-window commit history, an AI-tool disclosure, documentation, a working Monad-mainnet deployment, and a public operating demo no longer than three minutes.
