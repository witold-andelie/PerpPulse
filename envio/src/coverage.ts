export const COVERAGE_CLASSIFIER_VERSION = "coverage-classifier-v1";

export type CoverageStatus =
  | "caught_up_active"
  | "caught_up_quiet"
  | "lagging"
  | "quarantined"
  | "unknown";

export type QuarantineRange = {
  fromBlock: bigint;
  toBlock?: bigint;
  reason: string;
};

export type CoverageInput = {
  startBlock: bigint;
  chainHeadBlock?: bigint;
  processedBlock?: bigint;
  lastMatchedEventBlock?: bigint;
  maxCoverageLagBlocks: bigint;
  quietAfterBlocks: bigint;
  quarantine?: QuarantineRange;
};

export type CoverageAssessment = {
  classifierVersion: typeof COVERAGE_CLASSIFIER_VERSION;
  status: CoverageStatus;
  reason: string;
  coverageLagBlocks?: bigint;
  scannedSilenceBlocks?: bigint;
};

function result(
  status: CoverageStatus,
  reason: string,
  coverageLagBlocks?: bigint,
  scannedSilenceBlocks?: bigint,
): CoverageAssessment {
  return {
    classifierVersion: COVERAGE_CLASSIFIER_VERSION,
    status,
    reason,
    coverageLagBlocks,
    scannedSilenceBlocks,
  };
}

function isNonNegative(value: bigint | undefined): boolean {
  return value === undefined || value >= 0n;
}

export function classifyCoverage(input: CoverageInput): CoverageAssessment {
  if (
    input.startBlock < 0n ||
    input.maxCoverageLagBlocks < 0n ||
    input.quietAfterBlocks < 1n
  ) {
    return result("unknown", "invalid_classifier_configuration");
  }
  if (
    !isNonNegative(input.chainHeadBlock) ||
    !isNonNegative(input.lastMatchedEventBlock)
  ) {
    return result("unknown", "negative_block_observation");
  }

  if (input.processedBlock === -1n) {
    return result("unknown", "indexer_not_started");
  }

  const minimumProgress = input.startBlock - 1n;
  if (input.processedBlock !== undefined && input.processedBlock < minimumProgress) {
    return result("unknown", "processed_block_before_configured_range");
  }

  if (input.quarantine) {
    const { fromBlock, toBlock, reason } = input.quarantine;
    if (
      fromBlock < 0n ||
      (toBlock !== undefined && toBlock < fromBlock) ||
      reason.trim().length === 0
    ) {
      return result("unknown", "invalid_quarantine_range");
    }
    return result("quarantined", "active_quarantine");
  }

  if (input.chainHeadBlock === undefined) {
    return result("unknown", "missing_chain_head");
  }
  if (input.processedBlock === undefined) {
    return result("unknown", "missing_processed_block");
  }
  if (input.processedBlock > input.chainHeadBlock) {
    return result("unknown", "processed_block_ahead_of_chain_head");
  }
  if (
    input.lastMatchedEventBlock !== undefined &&
    (input.lastMatchedEventBlock < input.startBlock ||
      input.lastMatchedEventBlock > input.processedBlock)
  ) {
    return result("unknown", "last_match_outside_processed_range");
  }

  const coverageLagBlocks = input.chainHeadBlock - input.processedBlock;
  if (coverageLagBlocks > input.maxCoverageLagBlocks) {
    return result("lagging", "coverage_lag_exceeded", coverageLagBlocks);
  }

  if (input.lastMatchedEventBlock === undefined) {
    const scannedSilenceBlocks =
      input.processedBlock >= input.startBlock
        ? input.processedBlock - input.startBlock + 1n
        : 0n;
    return result(
      "caught_up_quiet",
      "covered_range_has_no_matched_events",
      coverageLagBlocks,
      scannedSilenceBlocks,
    );
  }

  const scannedSilenceBlocks = input.processedBlock - input.lastMatchedEventBlock;
  if (scannedSilenceBlocks >= input.quietAfterBlocks) {
    return result(
      "caught_up_quiet",
      "coverage_current_event_stream_quiet",
      coverageLagBlocks,
      scannedSilenceBlocks,
    );
  }
  return result(
    "caught_up_active",
    "coverage_current_recent_match",
    coverageLagBlocks,
    scannedSilenceBlocks,
  );
}
