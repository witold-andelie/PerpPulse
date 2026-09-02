import { Exchange } from "generated";

const SCHEMA_VERSION = "canonical-event-v1";
const HANDLER_VERSION = "envio-handlers-v1";

type LifecycleKind =
  | "ACCOUNT_CREATED"
  | "COLLATERAL_DEPOSIT"
  | "COLLATERAL_WITHDRAWAL"
  | "POSITION_OPENED"
  | "POSITION_INCREASED"
  | "POSITION_DECREASED"
  | "POSITION_CLOSED"
  | "POSITION_LIQUIDATED"
  | "POSITION_DELEVERAGED"
  | "POSITION_INVERTED"
  | "POSITION_UNWOUND"
  | "COLLATERAL_INCREASED"
  | "COLLATERAL_DECREASED"
  | "MARKET_FUNDING"
  | "MAKER_FILL"
  | "TAKER_FILL"
  | "ORDER_REQUEST"
  | "CONTRACT_ADDED";

type EventLike = {
  chainId: number;
  srcAddress: string;
  logIndex: number;
  block: { number: number | bigint; hash: string; timestamp: number | bigint };
  transaction: { hash: string };
  params: Record<string, unknown>;
};

function eventId(event: EventLike): string {
  return `${event.chainId}:${event.block.hash}:${event.transaction.hash}:${event.logIndex}`;
}

function jsonSafe(value: unknown): unknown {
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (Array.isArray(value)) {
    return value.map(jsonSafe);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([key, nested]) => [key, jsonSafe(nested)]),
    );
  }
  return value;
}

function asBigInt(value: unknown): bigint | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number") {
    return BigInt(value);
  }
  if (typeof value === "string" && value !== "") {
    return BigInt(value);
  }
  return undefined;
}

function asInt(value: unknown): number | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "bigint") {
    return Number(value);
  }
  return undefined;
}

function writeCanonical(
  context: { CanonicalEvent: { set: (entity: Record<string, unknown>) => void }; IndexerCheckpoint: { set: (entity: Record<string, unknown>) => void } },
  event: EventLike,
  kind: LifecycleKind,
  abiEventName: string,
  accountId?: bigint,
  perpetualId?: number,
  positionType?: number,
): void {
  context.CanonicalEvent.set({
    id: eventId(event),
    chainId: event.chainId,
    blockNumber: event.block.number,
    blockHash: event.block.hash,
    txHash: event.transaction.hash,
    logIndex: event.logIndex,
    timestamp: new Date(Number(event.block.timestamp) * 1000),
    srcAddress: event.srcAddress,
    abiEventName,
    kind,
    accountId,
    perpetualId,
    positionType,
    payloadJson: JSON.stringify(jsonSafe(event.params)),
  });
  context.IndexerCheckpoint.set({
    id: String(event.chainId),
    processedBlock: event.block.number,
    processedBlockHash: event.block.hash,
    schemaVersion: SCHEMA_VERSION,
    handlerVersion: HANDLER_VERSION,
  });
}

Exchange.AccountCreated.handler(async ({ event, context }) => {
  writeCanonical(context, event, "ACCOUNT_CREATED", "AccountCreated", asBigInt(event.params.id));
});

Exchange.CollateralDeposit.handler(async ({ event, context }) => {
  writeCanonical(context, event, "COLLATERAL_DEPOSIT", "CollateralDeposit", asBigInt(event.params.accountId));
});

Exchange.CollateralWithdrawal.handler(async ({ event, context }) => {
  writeCanonical(context, event, "COLLATERAL_WITHDRAWAL", "CollateralWithdrawal", asBigInt(event.params.accountId));
});

Exchange.IncreasePositionCollateral.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "COLLATERAL_INCREASED",
    "IncreasePositionCollateral",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
  );
});

Exchange.PositionCollateralDecreased.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "COLLATERAL_DECREASED",
    "PositionCollateralDecreased",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionOpened.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_OPENED",
    "PositionOpened",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionOpenedV2.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_OPENED",
    "PositionOpenedV2",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionIncreased.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_INCREASED",
    "PositionIncreased",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionIncreasedV2.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_INCREASED",
    "PositionIncreasedV2",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionDecreased.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_DECREASED",
    "PositionDecreased",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionClosed.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_CLOSED",
    "PositionClosed",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionLiquidated.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_LIQUIDATED",
    "PositionLiquidated",
    asBigInt(event.params.posAccountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionDeleveraged.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_DELEVERAGED",
    "PositionDeleveraged",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionDeleveragedV2.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_DELEVERAGED",
    "PositionDeleveragedV2",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionInverted.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_INVERTED",
    "PositionInverted",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionUnwound.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_UNWOUND",
    "PositionUnwound",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.PositionUnwoundV2.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "POSITION_UNWOUND",
    "PositionUnwoundV2",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
    asInt(event.params.positionType),
  );
});

Exchange.FundingEventCompleted.handler(async ({ event, context }) => {
  writeCanonical(context, event, "MARKET_FUNDING", "FundingEventCompleted", undefined, asInt(event.params.perpId));
});

Exchange.MakerOrderFilled.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "MAKER_FILL",
    "MakerOrderFilled",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
  );
});

Exchange.MakerOrderFilledV2.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "MAKER_FILL",
    "MakerOrderFilledV2",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
  );
});

Exchange.TakerOrderFilled.handler(async ({ event, context }) => {
  writeCanonical(context, event, "TAKER_FILL", "TakerOrderFilled");
});

Exchange.TakerOrderFilledV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "TAKER_FILL", "TakerOrderFilledV2");
});

Exchange.OrderRequest.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "ORDER_REQUEST",
    "OrderRequest",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
  );
});

Exchange.OrderRequestV2.handler(async ({ event, context }) => {
  writeCanonical(
    context,
    event,
    "ORDER_REQUEST",
    "OrderRequestV2",
    asBigInt(event.params.accountId),
    asInt(event.params.perpId),
  );
});

Exchange.ContractAdded.handler(async ({ event, context }) => {
  writeCanonical(context, event, "CONTRACT_ADDED", "ContractAdded", undefined, asInt(event.params.perpId));
});

Exchange.ContractAddedV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "CONTRACT_ADDED", "ContractAddedV2", undefined, asInt(event.params.perpId));
});
