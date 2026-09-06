const START_BLOCK_PATTERN = /^(\s*start_block:\s*)(\d+)(\s*(?:#.*)?)$/gm;

function requireNonNegative(value: bigint, label: string): void {
  if (value < 0n) {
    throw new Error(`${label} must be non-negative`);
  }
}

export function quickStartBlock(
  deploymentStartBlock: bigint,
  headBlock: bigint,
  windowBlocks: bigint,
): bigint {
  requireNonNegative(deploymentStartBlock, "deploymentStartBlock");
  requireNonNegative(headBlock, "headBlock");
  if (windowBlocks < 1n) {
    throw new Error("windowBlocks must be positive");
  }
  if (headBlock < deploymentStartBlock) {
    throw new Error("headBlock is before the Exchange deployment block");
  }

  const requestedStart = headBlock >= windowBlocks - 1n
    ? headBlock - windowBlocks + 1n
    : 0n;
  return requestedStart > deploymentStartBlock ? requestedStart : deploymentStartBlock;
}

function startBlockMatches(source: string): RegExpMatchArray[] {
  return [...source.matchAll(START_BLOCK_PATTERN)];
}

export function configuredStartBlock(source: string): bigint {
  const matches = startBlockMatches(source);
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one network start_block, found ${matches.length}`);
  }
  return BigInt(matches[0][2]);
}

export function renderQuickConfig(source: string, startBlock: bigint): string {
  requireNonNegative(startBlock, "startBlock");
  const matches = startBlockMatches(source);
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one network start_block, found ${matches.length}`);
  }

  const match = matches[0];
  const index = match.index;
  if (index === undefined) {
    throw new Error("Unable to locate the network start_block");
  }
  const replacement = `${match[1]}${startBlock}${match[3]}`;
  return `${source.slice(0, index)}${replacement}${source.slice(index + match[0].length)}`;
}
