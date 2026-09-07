# Protocol Registry

Snapshot date: 2026-09-02.

This file records the Perpl mainnet facts PerpPulse uses for indexing and
accounting. Live market decimals and newly listed markets were read from the
public context endpoint; contract addresses and the deployment block come from
official Perpl documentation and the dex-sdk `Chain::mainnet()` constructor.

Machine-readable copy: [`fixtures/protocol/mainnet-registry.json`](../fixtures/protocol/mainnet-registry.json).

## Chain and contracts

| Field | Value | Source |
| --- | --- | --- |
| Network | Monad mainnet | [Networks](https://docs.perpl.xyz/resources/for-developers/networks-and-configuration.md) |
| Chain ID | 143 | same |
| Exchange proxy | `0x34B6552d57a35a1D042CcAe1951BD1C370112a6F` | same; Envio must index the proxy |
| Collateral | AUSD `0x00000000eFE302BEAA2b3e6e1b18d08D69a9012a` | same |
| Collateral decimals | 6 | same; raw amounts are `10^6` |
| Deployment block | 54773010 | dex-sdk `Chain::mainnet().deployed_at_block()` and the Networks page |
| ABI revision | `rc_v1.1.7-178-g2273779` | dex-sdk `crates/sdk/abi/dex/REVISION` |
| Envio HyperSync | https://143.hypersync.xyz | [Envio networks](https://docs.envio.dev/docs/HyperIndex/supported-networks) |
| REST | https://app.perpl.xyz/api | Networks page |
| Market WebSocket | wss://app.perpl.xyz | Networks page |

Testnet is recorded only for contrast. PerpPulse targets mainnet.

| Field | Testnet |
| --- | --- |
| Chain ID | 10143 |
| Exchange | `0x1964C32f0bE608E7D29302AFF5E61268E72080cc` |
| Collateral | aUSD `0xa9012a055bd4e0eDfF8Ce09f960291C09D5322dC` |
| Deployment block | 62953 |

## Decimal and fee scales

| Quantity | Native unit | Scale |
| --- | --- | --- |
| Collateral, fees, PnL, funding on positions | CNS | 6 decimals |
| Price | PNS | per-market `price_decimals` |
| Size / lots | LNS | per-market `size_decimals` |
| Fee rate | per 100,000 | 5 decimals; 1 = 0.1 bps (`dex-sdk` `FEE_SCALE`) |
| Margin fraction | hundredths | `initMarginFracHdths` / `maintMarginFracHdths`. Official docs define `IMR = N / IMF`, so IMF 15.00 is 6.67 percent initial margin |
| Leverage | hundredths | `leverageHdths` |

## Markets at snapshot

Live `GET https://app.perpl.xyz/api/v1/pub/context` on 2026-09-02 listed the
markets below. The static docs table is missing LIT (60) and PUMP (90) and still
documents legacy SOL (30). The registry treats 30 as unlisted.

| perpetual_id | Symbol | price_decimals | size_decimals | init_margin_frac_hdths | maint_margin_frac_hdths |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | BTC | 1 | 5 | 1500 | 2500 |
| 10 | MON | 6 | 0 | 1000 | 2000 |
| 20 | ETH | 2 | 3 | 1200 | 2000 |
| 30 | SOL-legacy | 3 | 3 | 1200 | 2000 |
| 31 | SOL | 3 | 3 | 1200 | 2000 |
| 40 | HYPE | 4 | 2 | 1000 | 2000 |
| 50 | ZEC | 2 | 4 | 1000 | 1800 |
| 60 | LIT | 5 | 1 | 300 | 1000 |
| 90 | PUMP | 6 | 0 | 500 | 1000 |

Market lists change. Runtime ingestion must refresh from `/v1/pub/context` and
from `ContractAdded` / `ContractAddedV2` logs. Unknown markets fail closed.

## Lifecycle events indexed

Position reconstruction uses Exchange logs, not authenticated account history:

`AccountCreated`, `CollateralDeposit`, `CollateralWithdrawal`,
`AccountLiquidationCredit`, `TransferAccountToProtocol`,
`TransferProtocolToAccount`,
`PositionOpened`/`V2`, `PositionIncreased`/`V2`, `PositionDecreased`,
`PositionClosed`, `PositionLiquidated`, `PositionDeleveraged`/`V2`,
`PositionInverted`, payment and no-payment `PositionUnwound` variants,
`IncreasePositionCollateral`, `PositionCollateralDecreased`,
`PositionLiquidationCredit`, `FundingEventCompleted`, `MakerOrderFilled`/`V2`,
and `ContractAdded`/`V2`. Event-field semantics were checked against official
dex-sdk state processing at commit
`dbb37c59f6aef03e38d0787eb9c968f59f652617`.

The default risk hot path intentionally excludes `OrderRequest`/`V2`, which are
execution intents rather than position or balance mutations. It also excludes
`TakerOrderFilled`/`V2`: those logs have neither `accountId` nor `perpId`, while
the corresponding maker fill supplies both and is counted once for per-market
volume. Executed position state remains traceable through the lifecycle events.
An exact arbitrary-wallet free balance is not claimed because a subjectless
taker fill can carry a resulting balance that cannot be attributed without
correlating the excluded request stream. Raw request and duplicate taker-fill
evidence may be archived in a separate bounded pipeline, but they must not
inflate or become a second canonical position ledger.

The official dex-sdk still does not process funding into its in-memory cache.
PerpPulse records `FundingEventCompleted` and realized `fundingCNS` on
decrease/close/liquidation events. Unrealized funding on open positions is
marked unavailable rather than invented.

## Safety

The product remains read-only. Registry files contain public addresses only.
