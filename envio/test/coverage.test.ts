import assert from "node:assert/strict";
import test from "node:test";

import { classifyCoverage } from "../src/coverage";

const base = {
  startBlock: 100n,
  chainHeadBlock: 200n,
  processedBlock: 199n,
  lastMatchedEventBlock: 198n,
  maxCoverageLagBlocks: 2n,
  quietAfterBlocks: 10n,
};

test("reports active when coverage is current and a match is recent", () => {
  assert.deepEqual(classifyCoverage(base), {
    classifierVersion: "coverage-classifier-v1",
    status: "caught_up_active",
    reason: "coverage_current_recent_match",
    coverageLagBlocks: 1n,
    scannedSilenceBlocks: 1n,
  });
});

test("distinguishes a covered quiet range from indexer lag", () => {
  const quiet = classifyCoverage({ ...base, processedBlock: 200n, lastMatchedEventBlock: 150n });
  assert.equal(quiet.status, "caught_up_quiet");
  assert.equal(quiet.coverageLagBlocks, 0n);
  assert.equal(quiet.scannedSilenceBlocks, 50n);

  const noMatches = classifyCoverage({ ...base, processedBlock: 200n, lastMatchedEventBlock: undefined });
  assert.equal(noMatches.status, "caught_up_quiet");
  assert.equal(noMatches.scannedSilenceBlocks, 101n);
});

test("reports lag from processed coverage rather than the last event", () => {
  const result = classifyCoverage({
    ...base,
    chainHeadBlock: 250n,
    processedBlock: 240n,
    lastMatchedEventBlock: 239n,
  });
  assert.equal(result.status, "lagging");
  assert.equal(result.reason, "coverage_lag_exceeded");
  assert.equal(result.coverageLagBlocks, 10n);
});

test("quarantine is visible even when the indexer is otherwise caught up", () => {
  const result = classifyCoverage({
    ...base,
    quarantine: { fromBlock: 170n, toBlock: 180n, reason: "parent hash mismatch" },
  });
  assert.equal(result.status, "quarantined");
  assert.equal(result.reason, "active_quarantine");
});

test("missing and inconsistent observations fail visibly as unknown", () => {
  assert.equal(classifyCoverage({ ...base, chainHeadBlock: undefined }).status, "unknown");
  assert.equal(
    classifyCoverage({ ...base, processedBlock: -1n }).reason,
    "indexer_not_started",
  );
  assert.equal(
    classifyCoverage({ ...base, processedBlock: 201n }).reason,
    "processed_block_ahead_of_chain_head",
  );
  assert.equal(
    classifyCoverage({ ...base, lastMatchedEventBlock: 200n }).reason,
    "last_match_outside_processed_range",
  );
  assert.equal(
    classifyCoverage({ ...base, quietAfterBlocks: 0n }).reason,
    "invalid_classifier_configuration",
  );
});
