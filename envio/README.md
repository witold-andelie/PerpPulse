# PerpPulse Envio indexer

This HyperIndex project writes **canonical lifecycle events** from the Perpl
Exchange proxy on Monad mainnet. It does not reconstruct positions. The Rust
ledger in `crates/perppulse` is the only position and PnL engine.

## Network

| Field | Value |
| --- | --- |
| Chain ID | 143 |
| HyperSync | https://143.hypersync.xyz |
| Exchange proxy | `0x34B6552d57a35a1D042CcAe1951BD1C370112a6F` |
| Start block | 54773010 |

Use the proxy address, not the implementation address.

## Local run

```powershell
cd envio
pnpm install
pnpm codegen
pnpm dev
```

`pnpm codegen` generates the `generated` module imported by `src/EventHandlers.ts`.
Local `envio dev` may require Docker and `ENVIO_API_TOKEN`. Do not put secrets in
Git. This indexer is read-only: it does not send transactions.

## Output

Entities:

- `CanonicalEvent` — one row per indexed log, identity `(chainId, blockHash, txHash, logIndex)`
- `IndexerCheckpoint` — last processed block and handler/schema versions
