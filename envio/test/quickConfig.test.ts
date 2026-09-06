import assert from "node:assert/strict";
import test from "node:test";

import {
  configuredStartBlock,
  quickStartBlock,
  renderQuickConfig,
} from "../src/quickConfig";

test("quick start selects an inclusive recent window without crossing deployment", () => {
  assert.equal(quickStartBlock(100n, 1_000n, 250n), 751n);
  assert.equal(quickStartBlock(900n, 1_000n, 250n), 900n);
});

test("quick start rejects invalid or pre-deployment observations", () => {
  assert.throws(() => quickStartBlock(100n, 99n, 10n), /before the Exchange deployment/);
  assert.throws(() => quickStartBlock(100n, 1_000n, 0n), /must be positive/);
});

test("quick config changes only the single network start block", () => {
  const source = [
    "name: perppulse",
    "networks:",
    "  - id: 143",
    "    start_block: 54773010",
    "    contracts: []",
    "",
  ].join("\n");
  assert.equal(
    renderQuickConfig(source, 102_000_000n),
    source.replace("54773010", "102000000"),
  );
  assert.equal(configuredStartBlock(source), 54_773_010n);
  assert.throws(
    () => renderQuickConfig(`${source}    start_block: 102000000\n`, 10n),
    /exactly one/,
  );
});
