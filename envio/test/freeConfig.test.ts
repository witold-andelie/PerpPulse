import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

import { HANDLER_VERSION, INGESTION_PROFILE, SCHEMA_VERSION } from "../src/canonical";

function read(relative: string): string {
  return readFileSync(new URL(relative, import.meta.url), "utf8").replace(/\r\n/g, "\n");
}

const proof = JSON.parse(read("../free-window.json"));
const full = read("../config.yaml");
const free = read("../config.free.yaml");

test("free deployment is a finite counted window with unchanged event coverage", () => {
  assert.equal(proof.chainId, 143);
  assert.equal(proof.freePlanEventThreshold, 100_000);
  assert.equal(proof.safetyBudget, 50_000);
  assert.ok(Number.isSafeInteger(proof.canonicalEventCount));
  assert.ok(proof.canonicalEventCount > 0 && proof.canonicalEventCount <= proof.safetyBudget);
  assert.ok(Number.isSafeInteger(proof.startBlock) && Number.isSafeInteger(proof.endBlock));
  assert.ok(proof.startBlock >= 54_773_010 && proof.endBlock >= proof.startBlock);
  assert.equal([...free.matchAll(/^\s*start_block:/gm)].length, 1);
  assert.equal([...free.matchAll(/^\s*end_block:/gm)].length, 1);
  assert.ok(free.includes(`    start_block: ${proof.startBlock}\n`));
  assert.ok(free.includes(`    end_block: ${proof.endBlock}\n`));
  const restored = free
    .replace(/^# Historical window only\. Do not remove end_block on the free plan\.\n/, "")
    .replace("description: Fixed historical Perpl lifecycle window for the PerpPulse free demo",
      "description: Canonical Perpl position lifecycle indexer for PerpPulse on Monad")
    .replace(`    start_block: ${proof.startBlock}\n    end_block: ${proof.endBlock}\n`,
      "    start_block: 54773010\n");
  assert.equal(restored, full, "Free config must preserve every event, address and field selection");
  assert.equal(Object.values(proof.eventsByType).reduce<number>((sum, count) => {
    assert.ok(Number.isSafeInteger(count) && Number(count) > 0);
    return sum + Number(count);
  }, 0), proof.canonicalEventCount);
  for (const event of [proof.firstEvent, proof.lastEvent]) {
    assert.ok(Number(event.blockNumber) >= proof.startBlock && Number(event.blockNumber) <= proof.endBlock);
  }
});

test("count evidence remains bound to the indexed schema, handlers and ABI", () => {
  assert.equal(proof.source.ingestionProfile, INGESTION_PROFILE);
  assert.equal(proof.source.schemaVersion, SCHEMA_VERSION);
  assert.equal(proof.source.handlerVersion, HANDLER_VERSION);
  const expectedPaths = ["envio/config.yaml", "envio/schema.graphql",
    "envio/src/EventHandlers.ts", "envio/src/canonical.ts", "envio/abis/Exchange.events.json"];
  assert.deepEqual(Object.keys(proof.sourceArtifactSha256).sort(), expectedPaths.sort());
  for (const path of expectedPaths) {
    const digest = createHash("sha256").update(read(`../../${path}`)).digest("hex");
    assert.equal(digest, proof.sourceArtifactSha256[path], `Recount the window after changing ${path}`);
  }
});
