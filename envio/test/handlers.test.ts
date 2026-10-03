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
    markPricePNS: 700_000n,
    liqPricePNS: 625_000n,
    liqLotLNS: 50_000n,
    posLotLNS: 25_000n,
    deltaPnlCNS: -7_500_000_000n,
    fundingCNS: -2_000_000n,
    posDepositCNS: 2_500_000_000n,
    accBalanceCNS: 9_000_000_000n,
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
    owner: undefined,
    leverageHdths: undefined,
    lotLns: undefined,
    startLotLns: undefined,
    markPricePns: 700_000n,
    liqPricePns: 625_000n,
    liqLotLns: 50_000n,
    endLotLns: 25_000n,
    pricePns: undefined,
    amountCns: undefined,
    depositCns: 2_500_000_000n,
    startBalanceCns: undefined,
    startDepositCns: undefined,
    endDepositCns: undefined,
    deltaPnlCns: -7_500_000_000n,
    fundingCns: -2_000_000n,
    balanceCns: 9_000_000_000n,
    insFeeCns: undefined,
    protFeeCns: undefined,
    feeCns: undefined,
    fundingRatePct100k: undefined,
    fundingPricePns: undefined,
    fundingPaymentPns: undefined,
    fundingSumPns: undefined,
    fundingEventBlock: undefined,
    fundingAllowOverwrite: undefined,
    fundingScalingExp: undefined,
    positionFmvCns: undefined,
    paymentCns: undefined,
    amountOwedCns: undefined,
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

test("MarkUpdated and funding handlers preserve event identity and effective block", async () => {
  const data = {chainId:143, logIndex:2, block:{number:100,hash:"0xblock",parentHash:"0xparent",timestamp:1000},transaction:{hash:"0xtx"}};
  const mark = TestHelpers.Exchange.MarkUpdated.createMockEvent({perpId:1n,pricePNS:710000n,mockEventData:data});
  const marked = await TestHelpers.Exchange.MarkUpdated.processEvent({event:mark,mockDb:TestHelpers.MockDb.createMockDb()});
  const row = marked.entities.CanonicalEvent.getAll()[0];
  assert.equal(row.markPricePns,710000n);
  assert.equal(row.kind,"MARK_UPDATED");
  assert.equal(row.accountId,undefined);
  assert.equal(row.id,"143:0xblock:0xtx:2");
  const funding = TestHelpers.Exchange.FundingEventCompleted.createMockEvent({perpId:1n,fundingEventBlock:120n,
    specifiedRatePct100k:-5n,actualRatePct100k:-5n,fundingPricePNS:710000n,fundingPaymentPNS:-8n,fundingSumPNS:-10n,allowOverwrite:true,mockEventData:{...data,logIndex:3}});
  const funded = await TestHelpers.Exchange.FundingEventCompleted.processEvent({event:funding,mockDb:marked});
  const observation = funded.entities.CanonicalEvent.getAll().find((e) => e.kind === "MARKET_FUNDING")!;
  assert.equal(observation.fundingEventBlock,120n);
  assert.equal(observation.fundingAllowOverwrite,true);
  assert.equal(observation.fundingPaymentPns,-8n);
  assert.equal(observation.timestampMs,1000000n);
});
