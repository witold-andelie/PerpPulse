export const SCHEMA_VERSION = "canonical-event-v3";
export const HANDLER_VERSION = "envio-handlers-v3";
export const CLASSIFIER_VERSION = "exchange-classifier-v2";
export const INGESTION_PROFILE = "risk-hotpath-v1";
export const ABI_FINGERPRINT =
  "sha256:43f05c149262dbc7627530cb2e24d7ff56ff932c625b2e3fde4161558b86d267";

export type LifecycleKind =
  | "ACCOUNT_CREATED"
  | "COLLATERAL_DEPOSIT"
  | "COLLATERAL_WITHDRAWAL"
  | "POSITION_OPENED"
  | "POSITION_INCREASED"
  | "POSITION_DECREASED"
  | "POSITION_CLOSED"
  | "POSITION_LIQUIDATED"
  | "POSITION_DELEVERAGED"
  | "POSITION_INVERTED"
  | "POSITION_UNWOUND"
  | "COLLATERAL_INCREASED"
  | "COLLATERAL_DECREASED"
  | "MARKET_FUNDING"
  | "MAKER_FILL"
  | "CONTRACT_ADDED";

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
  CollateralDeposit: { kind: "COLLATERAL_DEPOSIT", ...ACCOUNT },
  CollateralWithdrawal: { kind: "COLLATERAL_WITHDRAWAL", ...ACCOUNT },
  IncreasePositionCollateral: { kind: "COLLATERAL_INCREASED", ...ACCOUNT, ...MARKET },
  PositionCollateralDecreased: { kind: "COLLATERAL_DECREASED", ...POSITION },
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
  FundingEventCompleted: { kind: "MARKET_FUNDING", ...MARKET },
  MakerOrderFilled: { kind: "MAKER_FILL", ...ACCOUNT, ...MARKET },
  MakerOrderFilledV2: { kind: "MAKER_FILL", ...ACCOUNT, ...MARKET },
  ContractAdded: { kind: "CONTRACT_ADDED", ...MARKET },
  ContractAddedV2: { kind: "CONTRACT_ADDED", ...MARKET },
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
