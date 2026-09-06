import { Exchange, type handlerContext } from "generated";

import {
  ABI_FINGERPRINT,
  CLASSIFIER_VERSION,
  HANDLER_VERSION,
  INGESTION_PROFILE,
  SCHEMA_VERSION,
  canonicalEventId,
  classifyExchangeEvent,
  serializeCanonicalPayload,
  type ExchangeAbiEventName,
} from "./canonical";

type EventLike = {
  chainId: number;
  srcAddress: string;
  logIndex: number;
  block: {
    number: number;
    hash: string;
    parentHash: string;
    timestamp: number;
  };
  transaction: { hash: string };
  params: Readonly<Record<string, unknown>>;
};

function nonNegativeBigInt(value: number, field: string): bigint {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${field} must be a non-negative safe integer`);
  }
  return BigInt(value);
}

function writeCanonical(
  context: handlerContext,
  event: EventLike,
  abiEventName: ExchangeAbiEventName,
): void {
  const classified = classifyExchangeEvent(abiEventName, event.params);
  context.CanonicalEvent.set({
    id: canonicalEventId({
      chainId: event.chainId,
      blockHash: event.block.hash,
      txHash: event.transaction.hash,
      logIndex: event.logIndex,
    }),
    chainId: event.chainId,
    blockNumber: nonNegativeBigInt(event.block.number, "block.number"),
    blockHash: event.block.hash,
    parentHash: event.block.parentHash,
    txHash: event.transaction.hash,
    logIndex: event.logIndex,
    timestampMs: nonNegativeBigInt(event.block.timestamp, "block.timestamp") * 1_000n,
    srcAddress: event.srcAddress,
    abiEventName,
    kind: classified.kind,
    accountId: classified.accountId,
    perpetualId: classified.perpetualId,
    positionType: classified.positionType,
    payloadJson: serializeCanonicalPayload(event.params),
    schemaVersion: SCHEMA_VERSION,
    handlerVersion: HANDLER_VERSION,
    classifierVersion: CLASSIFIER_VERSION,
    ingestionProfile: INGESTION_PROFILE,
    abiFingerprint: ABI_FINGERPRINT,
  });
}

Exchange.AccountCreated.handler(async ({ event, context }) => {
  writeCanonical(context, event, "AccountCreated");
});

Exchange.CollateralDeposit.handler(async ({ event, context }) => {
  writeCanonical(context, event, "CollateralDeposit");
});

Exchange.CollateralWithdrawal.handler(async ({ event, context }) => {
  writeCanonical(context, event, "CollateralWithdrawal");
});

Exchange.IncreasePositionCollateral.handler(async ({ event, context }) => {
  writeCanonical(context, event, "IncreasePositionCollateral");
});

Exchange.PositionCollateralDecreased.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionCollateralDecreased");
});

Exchange.PositionOpened.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionOpened");
});

Exchange.PositionOpenedV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionOpenedV2");
});

Exchange.PositionIncreased.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionIncreased");
});

Exchange.PositionIncreasedV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionIncreasedV2");
});

Exchange.PositionDecreased.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionDecreased");
});

Exchange.PositionClosed.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionClosed");
});

Exchange.PositionLiquidated.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionLiquidated");
});

Exchange.PositionDeleveraged.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionDeleveraged");
});

Exchange.PositionDeleveragedV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionDeleveragedV2");
});

Exchange.PositionInverted.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionInverted");
});

Exchange.PositionUnwound.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionUnwound");
});

Exchange.PositionUnwoundV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "PositionUnwoundV2");
});

Exchange.FundingEventCompleted.handler(async ({ event, context }) => {
  writeCanonical(context, event, "FundingEventCompleted");
});

Exchange.MakerOrderFilled.handler(async ({ event, context }) => {
  writeCanonical(context, event, "MakerOrderFilled");
});

Exchange.MakerOrderFilledV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "MakerOrderFilledV2");
});

Exchange.ContractAdded.handler(async ({ event, context }) => {
  writeCanonical(context, event, "ContractAdded");
});

Exchange.ContractAddedV2.handler(async ({ event, context }) => {
  writeCanonical(context, event, "ContractAddedV2");
});
