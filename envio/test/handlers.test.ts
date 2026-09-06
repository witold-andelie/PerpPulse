import assert from "node:assert/strict";
import test from "node:test";

import { TestHelpers } from "generated";

import {
  ABI_FINGERPRINT,
  CLASSIFIER_VERSION,
  HANDLER_VERSION,
  INGESTION_PROFILE,
  SCHEMA_VERSION,
} from "../src/canonical";
import "../src/EventHandlers";

test("PositionLiquidated writes one fully traceable canonical fact", async () => {
  const blockHash = `0x${"11".repeat(32)}`;
  const parentHash = `0x${"22".repeat(32)}`;
  const txHash = `0x${"33".repeat(32)}`;
  const event = TestHelpers.Exchange.PositionLiquidated.createMockEvent({
    perpId: 7n,
    posAccountId: 42n,
    positionType: 1n,
    mockEventData: {
      chainId: 143,
      logIndex: 9,
      block: {
        number: 102_500_000,
        hash: blockHash,
        parentHash,
        timestamp: 1_780_000_000,
      },
      transaction: { hash: txHash },
    },
  });

  const result = await TestHelpers.Exchange.PositionLiquidated.processEvent({
    event,
    mockDb: TestHelpers.MockDb.createMockDb(),
  });
  const rows = result.entities.CanonicalEvent.getAll();
  assert.equal(rows.length, 1);
  assert.deepEqual(rows[0], {
    id: `143:${blockHash}:${txHash}:9`,
    chainId: 143,
    blockNumber: 102_500_000n,
    blockHash,
    parentHash,
    txHash,
    logIndex: 9,
    timestampMs: 1_780_000_000_000n,
    srcAddress: event.srcAddress,
    abiEventName: "PositionLiquidated",
    kind: "POSITION_LIQUIDATED",
    accountId: 42n,
    perpetualId: 7,
    positionType: 1,
    payloadJson: JSON.stringify(
      Object.fromEntries(
        Object.entries(event.params)
          .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
          .map(([key, value]) => [key, typeof value === "bigint" ? value.toString() : value]),
      ),
    ),
    schemaVersion: SCHEMA_VERSION,
    handlerVersion: HANDLER_VERSION,
    classifierVersion: CLASSIFIER_VERSION,
    ingestionProfile: INGESTION_PROFILE,
    abiFingerprint: ABI_FINGERPRINT,
  });
});
