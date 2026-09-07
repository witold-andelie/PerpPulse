import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  ABI_FINGERPRINT,
  INGESTION_PROFILE,
  SUPPORTED_ABI_EVENTS,
  canonicalEventId,
  classifyExchangeEvent,
  projectExchangeEvent,
  serializeCanonicalPayload,
  type LifecycleKind,
} from "../src/canonical";

type ClassificationCase = {
  abiEventName: string;
  kind: LifecycleKind;
  accountId?: bigint;
  perpetualId?: number;
  positionType?: number;
};

const params = {
  id: 11n,
  accountId: 12n,
  posAccountId: 13n,
  perpId: 14n,
  positionType: 1n,
};

const cases: ClassificationCase[] = [
  { abiEventName: "AccountCreated", kind: "ACCOUNT_CREATED", accountId: 11n },
  { abiEventName: "AccountLiquidationCredit", kind: "ACCOUNT_LIQUIDATION_CREDIT", accountId: 12n, perpetualId: 14 },
  { abiEventName: "CollateralDeposit", kind: "COLLATERAL_DEPOSIT", accountId: 12n },
  { abiEventName: "CollateralWithdrawal", kind: "COLLATERAL_WITHDRAWAL", accountId: 12n },
  { abiEventName: "IncreasePositionCollateral", kind: "COLLATERAL_INCREASED", accountId: 12n, perpetualId: 14 },
  { abiEventName: "PositionCollateralDecreased", kind: "COLLATERAL_DECREASED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionLiquidationCredit", kind: "POSITION_LIQUIDATION_CREDIT", accountId: 12n, perpetualId: 14 },
  { abiEventName: "PositionOpened", kind: "POSITION_OPENED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionOpenedV2", kind: "POSITION_OPENED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionIncreased", kind: "POSITION_INCREASED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionIncreasedV2", kind: "POSITION_INCREASED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionDecreased", kind: "POSITION_DECREASED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionClosed", kind: "POSITION_CLOSED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionLiquidated", kind: "POSITION_LIQUIDATED", accountId: 13n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionDeleveraged", kind: "POSITION_DELEVERAGED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionDeleveragedV2", kind: "POSITION_DELEVERAGED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionInverted", kind: "POSITION_INVERTED", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionUnwound", kind: "POSITION_UNWOUND", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionUnwoundV2", kind: "POSITION_UNWOUND", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionUnwoundWithoutPayment", kind: "POSITION_UNWOUND", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "PositionUnwoundWithoutPaymentV2", kind: "POSITION_UNWOUND", accountId: 12n, perpetualId: 14, positionType: 1 },
  { abiEventName: "FundingEventCompleted", kind: "MARKET_FUNDING", perpetualId: 14 },
  { abiEventName: "MakerOrderFilled", kind: "MAKER_FILL", accountId: 12n, perpetualId: 14 },
  { abiEventName: "MakerOrderFilledV2", kind: "MAKER_FILL", accountId: 12n, perpetualId: 14 },
  { abiEventName: "ContractAdded", kind: "CONTRACT_ADDED", perpetualId: 14 },
  { abiEventName: "ContractAddedV2", kind: "CONTRACT_ADDED", perpetualId: 14 },
  { abiEventName: "TransferAccountToProtocol", kind: "ACCOUNT_TO_PROTOCOL_TRANSFER", accountId: 12n },
  { abiEventName: "TransferProtocolToAccount", kind: "PROTOCOL_TO_ACCOUNT_TRANSFER", accountId: 12n },
];

test("classifies every configured Exchange event without a fallback", () => {
  assert.deepEqual(new Set(SUPPORTED_ABI_EVENTS), new Set(cases.map((item) => item.abiEventName)));
  for (const item of cases) {
    const actual = classifyExchangeEvent(item.abiEventName, params);
    assert.equal(actual.kind, item.kind, item.abiEventName);
    assert.equal(actual.accountId, item.accountId, item.abiEventName);
    assert.equal(actual.perpetualId, item.perpetualId, item.abiEventName);
    assert.equal(actual.positionType, item.positionType, item.abiEventName);
  }
});

test("the risk hot path excludes intent-only and duplicate fill evidence", async () => {
  const config = await readFile(new URL("../config.yaml", import.meta.url), "utf8");
  const configuredEvents = [...config.matchAll(/^\s+- event: (\w+)$/gm)].map(
    (match) => match[1],
  );
  assert.deepEqual(new Set(configuredEvents), new Set(SUPPORTED_ABI_EVENTS));
  assert.equal(INGESTION_PROFILE, "risk-hotpath-v2");
  assert.doesNotMatch(config, /(?:OrderRequest|TakerOrderFilled)/);
});

test("projects exact state fields and preserves signed financial values", () => {
  assert.deepEqual(
    projectExchangeEvent("PositionLiquidated", {
      markPricePNS: 700_000n,
      liqPricePNS: 625_000n,
      liqLotLNS: 50_000n,
      posLotLNS: 25_000n,
      posDepositCNS: 2_500_000_000n,
      deltaPnlCNS: -7_500_000_000n,
      fundingCNS: -2_000_000n,
      accBalanceCNS: 9_000_000_000n,
    }),
    {
      markPricePns: 700_000n,
      liqPricePns: 625_000n,
      liqLotLns: 50_000n,
      endLotLns: 25_000n,
      depositCns: 2_500_000_000n,
      deltaPnlCns: -7_500_000_000n,
      fundingCns: -2_000_000n,
      balanceCns: 9_000_000_000n,
    },
  );
  assert.deepEqual(
    projectExchangeEvent("PositionCollateralDecreased", {
      markPricePNS: 710_000n,
      endEntryPricePNS: 705_000n,
      startDepositCNS: 10n,
      endDepositCNS: 8n,
      balanceCNS: 2n,
    }),
    {
      markPricePns: 710_000n,
      pricePns: 705_000n,
      startDepositCns: 10n,
      endDepositCns: 8n,
      balanceCns: 2n,
    },
  );
  assert.throws(
    () => projectExchangeEvent("PositionClosed", { pricePNS: 1n, deltaPnlCNS: "NaN", fundingCNS: 0n }),
    /must be an integer/,
  );
});

test("fails closed for unknown events and invalid subject values", () => {
  assert.throws(() => classifyExchangeEvent("UnknownEvent", params), /Unsupported Exchange event/);
  assert.throws(
    () => classifyExchangeEvent("PositionOpened", { ...params, perpId: 2_147_483_648n }),
    /GraphQL Int range/,
  );
  assert.throws(
    () => classifyExchangeEvent("PositionLiquidated", { ...params, posAccountId: undefined }),
    /must be an unsigned integer/,
  );
});

test("canonical identity changes when a reorg changes the block hash", () => {
  const shared = { chainId: 143, txHash: "0xtx", logIndex: 7 };
  const first = canonicalEventId({ ...shared, blockHash: "0xaaa" });
  assert.equal(first, canonicalEventId({ ...shared, blockHash: "0xaaa" }));
  assert.notEqual(first, canonicalEventId({ ...shared, blockHash: "0xbbb" }));
});

test("payload serialization is stable, bigint-safe, and finite", () => {
  const first = serializeCanonicalPayload({ z: 1n, a: { y: 2n, x: "value" } });
  const second = serializeCanonicalPayload({ a: { x: "value", y: 2n }, z: 1n });
  assert.equal(first, '{"a":{"x":"value","y":"2"},"z":"1"}');
  assert.equal(first, second);
  assert.throws(() => serializeCanonicalPayload({ value: Number.NaN }), /non-finite/);
  assert.throws(() => serializeCanonicalPayload({ value: undefined }), /undefined/);
});

test("the embedded ABI fingerprint matches the indexed ABI", async () => {
  const abi = await readFile(new URL("../abis/Exchange.events.json", import.meta.url), "utf8");
  const fingerprint = `sha256:${createHash("sha256").update(abi).digest("hex")}`;
  assert.equal(ABI_FINGERPRINT, fingerprint);
});

test("event handlers contain no wall-clock or random reads", async () => {
  const source = await readFile(new URL("../src/EventHandlers.ts", import.meta.url), "utf8");
  assert.doesNotMatch(source, /Date\.now|new Date|performance\.now|Math\.random/);
});
