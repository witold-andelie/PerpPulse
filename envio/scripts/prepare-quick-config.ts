import { readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  configuredStartBlock,
  quickStartBlock,
  renderQuickConfig,
} from "../src/quickConfig";

const DEPLOYMENT_START_BLOCK = 54_773_010n;
const DEFAULT_WINDOW_BLOCKS = 50_000n;
const DEFAULT_HEIGHT_URL = "https://143.hypersync.xyz/height";

type UnknownRecord = Record<string, unknown>;

function record(value: unknown, label: string): UnknownRecord {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value as UnknownRecord;
}

function unsignedInteger(value: unknown, label: string): bigint {
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) {
    return BigInt(value);
  }
  if (typeof value === "string" && /^\d+$/.test(value)) {
    return BigInt(value);
  }
  throw new Error(`${label} must be a non-negative integer`);
}

function environmentInteger(name: string, fallback: bigint): bigint {
  const value = process.env[name];
  return value === undefined ? fallback : unsignedInteger(value, name);
}

async function readHeadBlock(): Promise<bigint> {
  const override = process.env.PERPPULSE_QUICK_HEAD_BLOCK;
  if (override !== undefined) {
    return unsignedInteger(override, "PERPPULSE_QUICK_HEAD_BLOCK");
  }

  const headers: Record<string, string> = {};
  const token = process.env.ENVIO_API_TOKEN;
  if (token) {
    headers.authorization = `Bearer ${token}`;
  }
  const response = await fetch(
    process.env.PERPPULSE_HYPERSYNC_HEIGHT_URL ?? DEFAULT_HEIGHT_URL,
    { headers, signal: AbortSignal.timeout(10_000) },
  );
  if (!response.ok) {
    throw new Error(`HyperSync height returned HTTP ${response.status}`);
  }
  const payload = record(await response.json(), "HyperSync height payload");
  return unsignedInteger(payload.height, "HyperSync height");
}

async function main(): Promise<void> {
  const projectDirectory = join(dirname(fileURLToPath(import.meta.url)), "..");
  const outputPath = join(projectDirectory, "config.quick.yaml");
  const source = await readFile(join(projectDirectory, "config.yaml"), "utf8");
  const forceRefresh = process.env.PERPPULSE_QUICK_REFRESH === "1";
  const hasWindowOverride = process.env.PERPPULSE_QUICK_WINDOW_BLOCKS !== undefined;

  if (!forceRefresh && !hasWindowOverride) {
    try {
      const existing = await readFile(outputPath, "utf8");
      const existingStartBlock = configuredStartBlock(existing);
      if (
        existingStartBlock >= DEPLOYMENT_START_BLOCK &&
        existing === renderQuickConfig(source, existingStartBlock)
      ) {
        console.log(
          JSON.stringify({
            mode: "quick",
            configPath: "config.quick.yaml",
            reused: true,
            startBlock: existingStartBlock.toString(),
          }),
        );
        return;
      }
    } catch {
      // A missing or stale generated config is replaced below.
    }
  }

  const windowBlocks = environmentInteger(
    "PERPPULSE_QUICK_WINDOW_BLOCKS",
    DEFAULT_WINDOW_BLOCKS,
  );
  const headBlock = await readHeadBlock();
  const startBlock = quickStartBlock(
    DEPLOYMENT_START_BLOCK,
    headBlock,
    windowBlocks,
  );
  await writeFile(outputPath, renderQuickConfig(source, startBlock), "utf8");
  console.log(
    JSON.stringify({
      mode: "quick",
      configPath: "config.quick.yaml",
      reused: false,
      headBlock: headBlock.toString(),
      startBlock: startBlock.toString(),
      windowBlocks: windowBlocks.toString(),
    }),
  );
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`Quick config preparation failed: ${message}`);
  process.exitCode = 1;
});
