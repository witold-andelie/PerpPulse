export const SCHEMA_VERSION = "canonical-event-v4";
export const HANDLER_VERSION = "envio-handlers-v4";
export const CLASSIFIER_VERSION = "exchange-classifier-v3";
export const INGESTION_PROFILE = "risk-hotpath-v2";
export const ABI_FINGERPRINT =
  "sha256:b98e14a49e4201d71feeae380261784fc8872aa45b201d193194c6c5d56adbf1";

export type LifecycleKind =
  | "ACCOUNT_CREATED"
  | "ACCOUNT_LIQUIDATION_CREDIT"
  | "ACCOUNT_TO_PROTOCOL_TRANSFER"
  | "COLLATERAL_DEPOSIT"
  | "COLLATERAL_WITHDRAWAL"
  | "POSITION_OPENED"
  | "POSITION_INCREASED"
  | "POSITION_DECREASED"
  | "POSITION_CLOSED"
  | "POSITION_LIQUIDATED"
  | "POSITION_LIQUIDATION_CREDIT"
  | "POSITION_DELEVERAGED"
  | "POSITION_INVERTED"
  | "POSITION_UNWOUND"
  | "COLLATERAL_INCREASED"
  | "COLLATERAL_DECREASED"
  | "MARKET_FUNDING"
  | "MAKER_FILL"
  | "CONTRACT_ADDED"
  | "PROTOCOL_TO_ACCOUNT_TRANSFER";

type SubjectRule = {
  kind: LifecycleKind;
  accountIdField?: "id" | "accountId" | "posAccountId";
  perpetualIdField?: "perpId";
  positionTypeField?: "positionType";
};

const ACCOUNT = { accountIdField: "accountId" } as const;
const MARKET = { perpetualIdField: "perpId" } as const;
const POSITION = { ...ACCOUNT, ...MARKET, positionTypeField: "positionType" } as const;

const EVENT_RULES = {
  AccountCreated: { kind: "ACCOUNT_CREATED", accountIdField: "id" },
  AccountLiquidationCredit: { kind: "ACCOUNT_LIQUIDATION_CREDIT", ...ACCOUNT, ...MARKET },
  CollateralDeposit: { kind: "COLLATERAL_DEPOSIT", ...ACCOUNT },
  CollateralWithdrawal: { kind: "COLLATERAL_WITHDRAWAL", ...ACCOUNT },
  IncreasePositionCollateral: { kind: "COLLATERAL_INCREASED", ...ACCOUNT, ...MARKET },
  PositionCollateralDecreased: { kind: "COLLATERAL_DECREASED", ...POSITION },
  PositionLiquidationCredit: { kind: "POSITION_LIQUIDATION_CREDIT", ...ACCOUNT, ...MARKET },
  PositionOpened: { kind: "POSITION_OPENED", ...POSITION },
  PositionOpenedV2: { kind: "POSITION_OPENED", ...POSITION },
  PositionIncreased: { kind: "POSITION_INCREASED", ...POSITION },
  PositionIncreasedV2: { kind: "POSITION_INCREASED", ...POSITION },
  PositionDecreased: { kind: "POSITION_DECREASED", ...POSITION },
  PositionClosed: { kind: "POSITION_CLOSED", ...POSITION },
  PositionLiquidated: {
    kind: "POSITION_LIQUIDATED",
    accountIdField: "posAccountId",
    ...MARKET,
    positionTypeField: "positionType",
  },
  PositionDeleveraged: { kind: "POSITION_DELEVERAGED", ...POSITION },
  PositionDeleveragedV2: { kind: "POSITION_DELEVERAGED", ...POSITION },
  PositionInverted: { kind: "POSITION_INVERTED", ...POSITION },
  PositionUnwound: { kind: "POSITION_UNWOUND", ...POSITION },
  PositionUnwoundV2: { kind: "POSITION_UNWOUND", ...POSITION },
  PositionUnwoundWithoutPayment: { kind: "POSITION_UNWOUND", ...POSITION },
  PositionUnwoundWithoutPaymentV2: { kind: "POSITION_UNWOUND", ...POSITION },
  FundingEventCompleted: { kind: "MARKET_FUNDING", ...MARKET },
  MakerOrderFilled: { kind: "MAKER_FILL", ...ACCOUNT, ...MARKET },
  MakerOrderFilledV2: { kind: "MAKER_FILL", ...ACCOUNT, ...MARKET },
  ContractAdded: { kind: "CONTRACT_ADDED", ...MARKET },
  ContractAddedV2: { kind: "CONTRACT_ADDED", ...MARKET },
  TransferAccountToProtocol: { kind: "ACCOUNT_TO_PROTOCOL_TRANSFER", ...ACCOUNT },
  TransferProtocolToAccount: { kind: "PROTOCOL_TO_ACCOUNT_TRANSFER", ...ACCOUNT },
} as const satisfies Record<string, SubjectRule>;

export type ExchangeAbiEventName = keyof typeof EVENT_RULES;

export const SUPPORTED_ABI_EVENTS = Object.freeze(
  Object.keys(EVENT_RULES) as ExchangeAbiEventName[],
);

export type ClassifiedSubjects = {
  kind: LifecycleKind;
  accountId?: bigint;
  perpetualId?: number;
  positionType?: number;
};

export type CanonicalProjection = {
  owner?: string;
  leverageHdths?: bigint;
  lotLns?: bigint;
  startLotLns?: bigint;
  endLotLns?: bigint;
  liqLotLns?: bigint;
  pricePns?: bigint;
  markPricePns?: bigint;
  liqPricePns?: bigint;
  amountCns?: bigint;
  balanceCns?: bigint;
  startBalanceCns?: bigint;
  depositCns?: bigint;
  startDepositCns?: bigint;
  endDepositCns?: bigint;
  deltaPnlCns?: bigint;
  fundingCns?: bigint;
  insFeeCns?: bigint;
  protFeeCns?: bigint;
  feeCns?: bigint;
  fundingRatePct100k?: bigint;
  fundingPricePns?: bigint;
  fundingPaymentPns?: bigint;
  fundingSumPns?: bigint;
  positionFmvCns?: bigint;
  paymentCns?: bigint;
  amountOwedCns?: bigint;
};

export type CanonicalEventIdentity = {
  chainId: number;
  blockHash: string;
  txHash: string;
  logIndex: number;
};

function requireUnsignedBigInt(
  params: Readonly<Record<string, unknown>>,
  field: string,
  abiEventName: string,
): bigint {
  const value = params[field];
  let parsed: bigint;

  if (typeof value === "bigint") {
    parsed = value;
  } else if (typeof value === "number" && Number.isSafeInteger(value)) {
    parsed = BigInt(value);
  } else if (typeof value === "string" && /^\d+$/.test(value)) {
    parsed = BigInt(value);
  } else {
    throw new Error(`${abiEventName}.${field} must be an unsigned integer`);
  }

  if (parsed < 0n) {
    throw new Error(`${abiEventName}.${field} must be non-negative`);
  }
  return parsed;
}

function requireSignedBigInt(
  params: Readonly<Record<string, unknown>>,
  field: string,
  abiEventName: string,
): bigint {
  const value = params[field];
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number" && Number.isSafeInteger(value)) {
    return BigInt(value);
  }
  if (typeof value === "string" && /^-?\d+$/.test(value)) {
    return BigInt(value);
  }
  throw new Error(`${abiEventName}.${field} must be an integer`);
}

function requireAddress(
  params: Readonly<Record<string, unknown>>,
  field: string,
  abiEventName: string,
): string {
  const value = params[field];
  if (typeof value !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(value)) {
    throw new Error(`${abiEventName}.${field} must be a 20-byte address`);
  }
  return value;
}

function requireGraphqlInt(
  params: Readonly<Record<string, unknown>>,
  field: string,
  abiEventName: string,
): number {
  const parsed = requireUnsignedBigInt(params, field, abiEventName);
  if (parsed > 2_147_483_647n) {
    throw new Error(`${abiEventName}.${field} exceeds the GraphQL Int range`);
  }
  return Number(parsed);
}

export function classifyExchangeEvent(
  abiEventName: string,
  params: Readonly<Record<string, unknown>>,
): ClassifiedSubjects {
  const rule = (EVENT_RULES as Readonly<Record<string, SubjectRule>>)[abiEventName];
  if (!rule) {
    throw new Error(`Unsupported Exchange event: ${abiEventName}`);
  }

  return {
    kind: rule.kind,
    accountId: rule.accountIdField
      ? requireUnsignedBigInt(params, rule.accountIdField, abiEventName)
      : undefined,
    perpetualId: rule.perpetualIdField
      ? requireGraphqlInt(params, rule.perpetualIdField, abiEventName)
      : undefined,
    positionType: rule.positionTypeField
      ? requireGraphqlInt(params, rule.positionTypeField, abiEventName)
      : undefined,
  };
}

export function projectExchangeEvent(
  abiEventName: ExchangeAbiEventName,
  params: Readonly<Record<string, unknown>>,
): CanonicalProjection {
  const unsigned = (field: string): bigint =>
    requireUnsignedBigInt(params, field, abiEventName);
  const signed = (field: string): bigint => requireSignedBigInt(params, field, abiEventName);

  switch (abiEventName) {
    case "AccountCreated":
      return { owner: requireAddress(params, "account", abiEventName) };
    case "AccountLiquidationCredit":
      return {
        startBalanceCns: unsigned("startBalanceCNS"),
        balanceCns: unsigned("endBalanceCNS"),
      };
    case "CollateralDeposit":
    case "CollateralWithdrawal":
    case "TransferAccountToProtocol":
    case "TransferProtocolToAccount":
      return {
        amountCns: unsigned("amountCNS"),
        balanceCns: unsigned("balanceCNS"),
      };
    case "IncreasePositionCollateral":
      return {
        depositCns: unsigned("positionDepositCNS"),
        amountCns: unsigned("amountCNS"),
        balanceCns: unsigned("balanceCNS"),
      };
    case "PositionCollateralDecreased":
      return {
        markPricePns: unsigned("markPricePNS"),
        pricePns: unsigned("endEntryPricePNS"),
        startDepositCns: unsigned("startDepositCNS"),
        endDepositCns: unsigned("endDepositCNS"),
        balanceCns: unsigned("balanceCNS"),
      };
    case "PositionLiquidationCredit":
      return {
        startDepositCns: unsigned("startDepositCNS"),
        endDepositCns: unsigned("endDepositCNS"),
      };
    case "PositionOpened":
    case "PositionOpenedV2":
      return {
        leverageHdths: unsigned("leverageHdths"),
        depositCns: unsigned("depositCNS"),
        pricePns: unsigned("pricePNS"),
        lotLns: unsigned("lotLNS"),
        insFeeCns: unsigned("insFeeCNS"),
        protFeeCns: unsigned("protFeeCNS"),
      };
    case "PositionIncreased":
    case "PositionIncreasedV2":
      return {
        leverageHdths: unsigned("leverageHdths"),
        startDepositCns: unsigned("startDepositCNS"),
        endDepositCns: unsigned("endDepositCNS"),
        pricePns: unsigned("pricePNS"),
        startLotLns: unsigned("startLotLNS"),
        endLotLns: unsigned("endLotLNS"),
        insFeeCns: unsigned("insFeeCNS"),
        protFeeCns: unsigned("protFeeCNS"),
      };
    case "PositionDecreased":
      return {
        startDepositCns: unsigned("startDepositCNS"),
        endDepositCns: unsigned("endDepositCNS"),
        startLotLns: unsigned("startLotLNS"),
        endLotLns: unsigned("endLotLNS"),
        deltaPnlCns: signed("deltaPnlCNS"),
        fundingCns: signed("fundingCNS"),
      };
    case "PositionClosed":
      return {
        pricePns: unsigned("pricePNS"),
        deltaPnlCns: signed("deltaPnlCNS"),
        fundingCns: signed("fundingCNS"),
      };
    case "PositionLiquidated":
      return {
        markPricePns: unsigned("markPricePNS"),
        liqPricePns: unsigned("liqPricePNS"),
        liqLotLns: unsigned("liqLotLNS"),
        endLotLns: unsigned("posLotLNS"),
        depositCns: unsigned("posDepositCNS"),
        deltaPnlCns: signed("deltaPnlCNS"),
        fundingCns: signed("fundingCNS"),
        balanceCns: unsigned("accBalanceCNS"),
      };
    case "PositionDeleveraged":
    case "PositionDeleveragedV2":
      return {
        markPricePns: unsigned("markPricePNS"),
        pricePns: unsigned("deleveragePricePNS"),
        startDepositCns: unsigned("startDepositCNS"),
        endDepositCns: unsigned("endDepositCNS"),
        startLotLns: unsigned("startLotLNS"),
        endLotLns: unsigned("endLotLNS"),
        deltaPnlCns: signed("deltaPnlCNS"),
        fundingCns: signed("fundingCNS"),
        balanceCns: unsigned("balanceCNS"),
      };
    case "PositionInverted":
      return {
        leverageHdths: unsigned("leverageHdths"),
        startDepositCns: unsigned("startDepositCNS"),
        endDepositCns: unsigned("endDepositCNS"),
        pricePns: unsigned("pricePNS"),
        startLotLns: unsigned("startLotLNS"),
        endLotLns: unsigned("endLotLNS"),
        deltaPnlCns: signed("deltaPnlCNS"),
        fundingCns: signed("fundingCNS"),
        insFeeCns: unsigned("insFeeCNS"),
        protFeeCns: unsigned("protFeeCNS"),
      };
    case "PositionUnwound":
    case "PositionUnwoundV2":
      return {
        markPricePns: unsigned("markPricePNS"),
        pricePns: unsigned("pricePNS"),
        lotLns: unsigned("lotLNS"),
        depositCns: unsigned("depositCNS"),
        positionFmvCns: signed("positionFmvCNS"),
        paymentCns: unsigned("paymentCNS"),
        balanceCns: unsigned("balanceCNS"),
      };
    case "PositionUnwoundWithoutPayment":
    case "PositionUnwoundWithoutPaymentV2":
      return {
        markPricePns: unsigned("markPricePNS"),
        pricePns: unsigned("pricePNS"),
        lotLns: unsigned("lotLNS"),
        depositCns: unsigned("depositCNS"),
        positionFmvCns: signed("positionFmvCNS"),
        amountOwedCns: unsigned("amountOwedCNS"),
      };
    case "FundingEventCompleted":
      return {
        fundingRatePct100k: signed("actualRatePct100k"),
        fundingPricePns: unsigned("fundingPricePNS"),
        fundingPaymentPns: signed("fundingPaymentPNS"),
        fundingSumPns: signed("fundingSumPNS"),
      };
    case "MakerOrderFilled":
    case "MakerOrderFilledV2":
      return {
        pricePns: unsigned("pricePNS"),
        lotLns: unsigned("lotLNS"),
        feeCns: unsigned("feeCNS"),
        amountCns: signed("amountCNS"),
        balanceCns: unsigned("balanceCNS"),
      };
    case "ContractAdded":
    case "ContractAddedV2":
      return {};
  }
}

export function canonicalEventId(identity: CanonicalEventIdentity): string {
  if (!Number.isSafeInteger(identity.chainId) || identity.chainId < 0) {
    throw new Error("chainId must be a non-negative safe integer");
  }
  if (!Number.isSafeInteger(identity.logIndex) || identity.logIndex < 0) {
    throw new Error("logIndex must be a non-negative safe integer");
  }
  if (identity.blockHash.length === 0 || identity.txHash.length === 0) {
    throw new Error("blockHash and txHash must be non-empty");
  }
  return `${identity.chainId}:${identity.blockHash}:${identity.txHash}:${identity.logIndex}`;
}

function jsonSafe(value: unknown, path: string): unknown {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error(`${path} contains a non-finite number`);
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item, index) => jsonSafe(item, `${path}[${index}]`));
  }
  if (typeof value === "object") {
    const result: Record<string, unknown> = {};
    for (const [key, nested] of Object.entries(value as Record<string, unknown>).sort(
      ([left], [right]) => (left < right ? -1 : left > right ? 1 : 0),
    )) {
      if (nested === undefined) {
        throw new Error(`${path}.${key} is undefined`);
      }
      result[key] = jsonSafe(nested, `${path}.${key}`);
    }
    return result;
  }
  throw new Error(`${path} contains an unsupported JSON value`);
}

export function serializeCanonicalPayload(params: Readonly<Record<string, unknown>>): string {
  return JSON.stringify(jsonSafe(params, "params"));
}
