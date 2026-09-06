import { classifyCoverage, type QuarantineRange } from "../src/coverage";

const COVERAGE_QUERY = `
  query PerpPulseCoverage($chainId: Int!) {
    _meta(where: { chainId: { _eq: $chainId } }) {
      chainId
      startBlock
      endBlock
      progressBlock
      bufferBlock
      firstEventBlock
      eventsProcessed
      sourceBlock
      readyAt
      isReady
    }
    CanonicalEvent(
      where: { chainId: { _eq: $chainId } }
      limit: 1
      order_by: [{ blockNumber: desc }, { logIndex: desc }]
    ) {
      id
      blockNumber
      blockHash
      txHash
      logIndex
      abiEventName
    }
  }
`;

const DEFAULT_GRAPHQL_URL = "http://localhost:8080/v1/graphql";
const DEFAULT_HYPERSYNC_HEIGHT_URL = "https://143.hypersync.xyz/height";

type UnknownRecord = Record<string, unknown>;

function record(value: unknown, label: string): UnknownRecord {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value as UnknownRecord;
}

function array(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} must be an array`);
  }
  return value;
}

function bigintField(value: unknown, label: string): bigint {
  if (typeof value === "number" && Number.isSafeInteger(value)) {
    return BigInt(value);
  }
  if (typeof value === "string" && /^-?\d+$/.test(value)) {
    return BigInt(value);
  }
  throw new Error(`${label} must be an integer`);
}

function optionalBigintField(value: unknown, label: string): bigint | undefined {
  return value === null || value === undefined ? undefined : bigintField(value, label);
}

function integerEnvironment(name: string, fallback: bigint): bigint {
  const value = process.env[name];
  if (value === undefined) {
    return fallback;
  }
  return bigintField(value, name);
}

function quarantineFromEnvironment(): QuarantineRange | undefined {
  const from = process.env.PERPPULSE_QUARANTINE_FROM_BLOCK;
  if (from === undefined) {
    return undefined;
  }
  const to = process.env.PERPPULSE_QUARANTINE_TO_BLOCK;
  return {
    fromBlock: bigintField(from, "PERPPULSE_QUARANTINE_FROM_BLOCK"),
    toBlock: to === undefined ? undefined : bigintField(to, "PERPPULSE_QUARANTINE_TO_BLOCK"),
    reason: process.env.PERPPULSE_QUARANTINE_REASON ?? "",
  };
}

async function responseJson(response: Response, label: string): Promise<unknown> {
  if (!response.ok) {
    throw new Error(`${label} returned HTTP ${response.status}`);
  }
  return response.json() as Promise<unknown>;
}

async function readHyperSyncHeight(url: string): Promise<bigint> {
  const token = process.env.ENVIO_API_TOKEN;
  const headers: Record<string, string> = {};
  if (token) {
    headers.authorization = `Bearer ${token}`;
  }
  let response: Response;
  try {
    response = await fetch(url, {
      headers,
      signal: AbortSignal.timeout(10_000),
    });
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`HyperSync height request failed: ${message}`);
  }
  const payload = record(await responseJson(response, "HyperSync height"), "HyperSync height payload");
  return bigintField(payload.height, "HyperSync height");
}

async function readIndexerCoverage(url: string, chainId: number): Promise<UnknownRecord> {
  const headers: Record<string, string> = { "content-type": "application/json" };
  const adminSecret = process.env.HASURA_GRAPHQL_ADMIN_SECRET;
  if (adminSecret) {
    headers["x-hasura-admin-secret"] = adminSecret;
  }
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers,
      body: JSON.stringify({ query: COVERAGE_QUERY, variables: { chainId } }),
      signal: AbortSignal.timeout(10_000),
    });
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Indexer GraphQL request failed: ${message}`);
  }
  const payload = record(await responseJson(response, "Indexer GraphQL"), "Indexer GraphQL payload");
  if (payload.errors !== undefined) {
    const errors = array(payload.errors, "Indexer GraphQL errors");
    throw new Error(`Indexer GraphQL returned ${errors.length} error(s)`);
  }
  return record(payload.data, "Indexer GraphQL data");
}

function absoluteDifference(left: bigint, right: bigint): bigint {
  return left >= right ? left - right : right - left;
}

function printable(value: unknown): string {
  return JSON.stringify(
    value,
    (_key, nested) => (typeof nested === "bigint" ? nested.toString() : nested),
    2,
  );
}

async function main(): Promise<void> {
  const chainId = Number(integerEnvironment("PERPPULSE_CHAIN_ID", 143n));
  if (!Number.isSafeInteger(chainId) || chainId < 0) {
    throw new Error("PERPPULSE_CHAIN_ID must fit a non-negative JavaScript integer");
  }

  const graphqlUrl = process.env.PERPPULSE_GRAPHQL_URL ?? DEFAULT_GRAPHQL_URL;
  const heightUrl = process.env.PERPPULSE_HYPERSYNC_HEIGHT_URL ?? DEFAULT_HYPERSYNC_HEIGHT_URL;
  const [hyperSyncHead, data] = await Promise.all([
    readHyperSyncHeight(heightUrl),
    readIndexerCoverage(graphqlUrl, chainId),
  ]);

  const metadataRows = array(data._meta, "_meta");
  if (metadataRows.length !== 1) {
    throw new Error(`Expected one _meta row for chain ${chainId}, received ${metadataRows.length}`);
  }
  const metadata = record(metadataRows[0], "_meta row");
  if (Number(bigintField(metadata.chainId, "_meta.chainId")) !== chainId) {
    throw new Error("_meta returned a different chain ID");
  }

  const eventRows = array(data.CanonicalEvent, "CanonicalEvent");
  if (eventRows.length > 1) {
    throw new Error("CanonicalEvent latest-event query returned more than one row");
  }
  const lastEvent = eventRows.length === 0 ? undefined : record(eventRows[0], "CanonicalEvent row");
  const sourceHead = bigintField(metadata.sourceBlock, "_meta.sourceBlock");
  const processedBlock = bigintField(metadata.progressBlock, "_meta.progressBlock");
  const startBlock = bigintField(metadata.startBlock, "_meta.startBlock");
  const lastMatchedEventBlock = lastEvent
    ? bigintField(lastEvent.blockNumber, "CanonicalEvent.blockNumber")
    : undefined;

  const assessment = classifyCoverage({
    startBlock,
    chainHeadBlock: hyperSyncHead,
    processedBlock,
    lastMatchedEventBlock,
    maxCoverageLagBlocks: integerEnvironment("PERPPULSE_MAX_COVERAGE_LAG_BLOCKS", 20n),
    quietAfterBlocks: integerEnvironment("PERPPULSE_QUIET_AFTER_BLOCKS", 100n),
    quarantine: quarantineFromEnvironment(),
  });

  const observation = {
    observationVersion: "coverage-observation-v1",
    observedAt: new Date().toISOString(),
    chainId,
    status: assessment.status,
    reason: assessment.reason,
    classifierVersion: assessment.classifierVersion,
    startBlock,
    processedBlock,
    hyperSyncHead,
    envioSourceHead: sourceHead,
    headDisagreementBlocks: absoluteDifference(hyperSyncHead, sourceHead),
    coverageLagBlocks: assessment.coverageLagBlocks,
    scannedSilenceBlocks: assessment.scannedSilenceBlocks,
    eventsProcessed: bigintField(metadata.eventsProcessed, "_meta.eventsProcessed"),
    bufferBlock: bigintField(metadata.bufferBlock, "_meta.bufferBlock"),
    firstEventBlock: optionalBigintField(metadata.firstEventBlock, "_meta.firstEventBlock"),
    isReady: metadata.isReady,
    readyAt: metadata.readyAt,
    lastMatchedEvent: lastEvent,
  };

  console.log(printable(observation));
  if (!assessment.status.startsWith("caught_up_")) {
    process.exitCode = 2;
  }
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`Coverage probe failed: ${message}`);
  process.exitCode = 1;
});
