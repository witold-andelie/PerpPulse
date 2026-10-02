use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::coverage::CoverageEvidence;
use crate::error::{DataQualityError, Result};
use crate::events::{CanonicalEvent, CanonicalProvenance, LifecycleKind};
use crate::identity::AsOf;
use crate::registry::ProtocolRegistry;

const COVERAGE_QUERY: &str = r#"
query PerpPulseRustCoverage($chainId: Int!) {
  _meta(where: {chainId: {_eq: $chainId}}) {
    chainId
    startBlock
    progressBlock
    sourceBlock
    eventsProcessed
    isReady
  }
  CanonicalEvent(
    where: {chainId: {_eq: $chainId}}
    limit: 1
    order_by: [{blockNumber: desc}, {logIndex: desc}]
  ) {
    id
    blockNumber
    blockHash
    logIndex
    timestampMs
  }
}
"#;

const ACCOUNT_EVENTS_QUERY: &str = r#"
query PerpPulseRustAccountEvents(
  $chainId: Int!
  $accountId: numeric!
  $cursorBlock: numeric!
  $cursorLog: Int!
  $endBlock: numeric!
  $limit: Int!
) {
  CanonicalEvent(
    where: {
      chainId: {_eq: $chainId}
      accountId: {_eq: $accountId}
      blockNumber: {_lte: $endBlock}
      _or: [
        {blockNumber: {_gt: $cursorBlock}}
        {_and: [{blockNumber: {_eq: $cursorBlock}}, {logIndex: {_gt: $cursorLog}}]}
      ]
    }
    limit: $limit
    order_by: [{blockNumber: asc}, {logIndex: asc}]
  ) {
    id
    chainId
    blockNumber
    blockHash
    parentHash
    txHash
    logIndex
    timestampMs
    srcAddress
    abiEventName
    kind
    accountId
    perpetualId
    positionType
    payloadJson
    schemaVersion
    handlerVersion
    classifierVersion
    ingestionProfile
    abiFingerprint
  }
}
"#;

const VERIFY_EVENT_QUERY: &str = r#"
query PerpPulseRustVerifyEvent($id: String!) {
  CanonicalEvent(where: {id: {_eq: $id}}, limit: 1) {
    id
    blockNumber
    blockHash
    logIndex
    timestampMs
  }
}
"#;

const LEGACY_SCHEMA_VERSION: &str = "canonical-event-v3";
const LEGACY_HANDLER_VERSION: &str = "envio-handlers-v3";
const LEGACY_CLASSIFIER_VERSION: &str = "exchange-classifier-v2";
const LEGACY_INGESTION_PROFILE: &str = "risk-hotpath-v1";
const LEGACY_ABI_FINGERPRINT: &str =
    "sha256:43f05c149262dbc7627530cb2e24d7ff56ff932c625b2e3fde4161558b86d267";
const CURRENT_SCHEMA_VERSION: &str = "canonical-event-v4";
const CURRENT_HANDLER_VERSION: &str = "envio-handlers-v4";
const CURRENT_CLASSIFIER_VERSION: &str = "exchange-classifier-v3";
const CURRENT_INGESTION_PROFILE: &str = "risk-hotpath-v2";
const LEDGER_ELIGIBLE_PROFILE: &str = "risk-hotpath-v2";
const CURRENT_ABI_FINGERPRINT: &str =
    "sha256:b98e14a49e4201d71feeae380261784fc8872aa45b201d193194c6c5d56adbf1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedPoint {
    pub id: String,
    pub block_number: u64,
    pub block_hash: String,
    pub log_index: u32,
    pub timestamp_ms: i64,
}

#[derive(Clone, Debug)]
pub struct EnvioCoverage {
    pub evidence: CoverageEvidence,
    pub source_block: u64,
    pub events_processed: u64,
    pub is_ready: bool,
    pub latest_event: IndexedPoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayEligibility {
    pub eligible: bool,
    pub basis: String,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct AccountEventSlice {
    pub account_id: u64,
    pub coverage: EnvioCoverage,
    pub as_of: AsOf,
    pub events: Vec<CanonicalEvent>,
}

impl AccountEventSlice {
    pub fn replay_eligibility(&self, registry: &ProtocolRegistry) -> Result<ReplayEligibility> {
        registry.require_chain(self.coverage.evidence.chain_id)?;
        self.coverage.evidence.validate()?;
        if self.events.is_empty() {
            return Err(DataQualityError::msg(format!(
                "account {} has no canonical events in the indexed range",
                self.account_id
            )));
        }

        let mut profiles = BTreeSet::new();
        for event in &self.events {
            if event.account_id != Some(self.account_id) {
                return Err(DataQualityError::msg(format!(
                    "account slice {} contains event for account {:?}",
                    self.account_id, event.account_id
                )));
            }
            if !event
                .contract_address
                .eq_ignore_ascii_case(&registry.exchange_address)
            {
                return Err(DataQualityError::msg(format!(
                    "event {} contract {} does not match registry exchange {}",
                    event.event_id()?.key(),
                    event.contract_address,
                    registry.exchange_address
                )));
            }
            let provenance = event
                .provenance
                .as_ref()
                .ok_or_else(|| DataQualityError::msg("Envio event is missing provenance"))?;
            profiles.insert(provenance.ingestion_profile.as_str());
        }
        if profiles.len() != 1 {
            return Err(DataQualityError::msg(format!(
                "account slice mixes {} ingestion profiles",
                profiles.len()
            )));
        }
        let profile = profiles.iter().next().copied().unwrap_or_default();
        if profile != LEDGER_ELIGIBLE_PROFILE {
            return Ok(ReplayEligibility {
                eligible: false,
                basis: "ingestion-profile".to_string(),
                reason: format!(
                    "profile {profile} is inspection-only; position replay requires {LEDGER_ELIGIBLE_PROFILE} after low-frequency state events are indexed"
                ),
            });
        }

        if self.coverage.evidence.start_block <= registry.deployed_at_block {
            return Ok(ReplayEligibility {
                eligible: true,
                basis: "deployment-history".to_string(),
                reason: "indexed coverage begins at or before Exchange deployment".to_string(),
            });
        }

        let first = &self.events[0];
        if first.kind == LifecycleKind::AccountCreated {
            return Ok(ReplayEligibility {
                eligible: true,
                basis: "account-birth".to_string(),
                reason: format!(
                    "account {} was created inside the indexed range at block {}",
                    self.account_id, first.block_number
                ),
            });
        }

        Ok(ReplayEligibility {
            eligible: false,
            basis: "incomplete-history".to_string(),
            reason: format!(
                "indexed coverage starts at block {} after deployment and the first account event is {:?}",
                self.coverage.evidence.start_block, first.kind
            ),
        })
    }
}

pub struct EnvioClient {
    endpoint: String,
    admin_secret: Option<String>,
    page_size: u32,
    max_events: usize,
    agent: ureq::Agent,
}

impl EnvioClient {
    pub fn new(
        endpoint: impl Into<String>,
        admin_secret: Option<String>,
        page_size: u32,
        max_events: usize,
    ) -> Result<Self> {
        let endpoint = endpoint.into();
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            return Err(DataQualityError::msg(
                "GraphQL endpoint must use http:// or https://",
            ));
        }
        if page_size == 0 || page_size > 1_000 {
            return Err(DataQualityError::msg(
                "GraphQL page_size must be between 1 and 1000",
            ));
        }
        if max_events == 0 {
            return Err(DataQualityError::msg("GraphQL max_events must be positive"));
        }
        Ok(Self {
            endpoint,
            admin_secret: admin_secret.filter(|value| !value.is_empty()),
            page_size,
            max_events,
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(15)))
                .build()
                .into(),
        })
    }

    pub fn fetch_coverage(&self, chain_id: u64) -> Result<EnvioCoverage> {
        if chain_id > i32::MAX as u64 {
            return Err(DataQualityError::msg(
                "chain_id exceeds the GraphQL Int range",
            ));
        }
        let data: RawCoverageData = self.post(
            COVERAGE_QUERY,
            &CoverageVariables {
                chain_id: chain_id as u32,
            },
        )?;
        if data.metadata.len() != 1 {
            return Err(DataQualityError::msg(format!(
                "expected one Envio metadata row for chain {chain_id}, received {}",
                data.metadata.len()
            )));
        }
        if data.latest_events.len() != 1 {
            return Err(DataQualityError::msg(format!(
                "expected one latest canonical event, received {}",
                data.latest_events.len()
            )));
        }
        let metadata = &data.metadata[0];
        if u64::from(metadata.chain_id) != chain_id {
            return Err(DataQualityError::msg(
                "Envio metadata returned a different chain_id",
            ));
        }
        let evidence = CoverageEvidence {
            chain_id,
            start_block: metadata.start_block.to_u64("_meta.startBlock")?,
            processed_block: metadata.progress_block.to_u64("_meta.progressBlock")?,
        };
        evidence.validate()?;
        let source_block = metadata.source_block.to_u64("_meta.sourceBlock")?;
        if evidence.processed_block > source_block {
            return Err(DataQualityError::msg(format!(
                "processed block {} exceeds Envio source block {source_block}",
                evidence.processed_block
            )));
        }
        let latest_event = data.latest_events[0].to_indexed_point()?;
        if latest_event.block_number > evidence.processed_block
            || latest_event.block_number < evidence.start_block
        {
            return Err(DataQualityError::msg(format!(
                "latest event block {} exceeds processed coverage {}",
                latest_event.block_number, evidence.processed_block
            )));
        }
        Ok(EnvioCoverage {
            evidence,
            source_block,
            events_processed: metadata.events_processed.to_u64("_meta.eventsProcessed")?,
            is_ready: metadata.is_ready,
            latest_event,
        })
    }

    pub fn fetch_account(&self, chain_id: u64, account_id: u64) -> Result<AccountEventSlice> {
        let coverage = self.fetch_coverage(chain_id)?;
        self.fetch_account_at(account_id, &coverage)
    }

    /// Read every selected account against the same immutable cutoff.
    pub fn fetch_account_at(
        &self,
        account_id: u64,
        coverage: &EnvioCoverage,
    ) -> Result<AccountEventSlice> {
        let chain_id = coverage.evidence.chain_id;
        if !coverage.is_ready {
            return Err(DataQualityError::msg("Envio indexer is not ready"));
        }
        let as_of = AsOf::new(
            chain_id,
            coverage.latest_event.block_number,
            coverage.latest_event.block_hash.clone(),
            coverage.latest_event.timestamp_ms,
            Some(coverage.latest_event.log_index),
        )?;

        let mut cursor_block = coverage.evidence.start_block;
        let mut cursor_log = -1i64;
        let mut raw_events = Vec::new();
        loop {
            let data: RawAccountData = self.post(
                ACCOUNT_EVENTS_QUERY,
                &AccountVariables {
                    chain_id: chain_id as u32,
                    account_id: account_id.to_string(),
                    cursor_block: cursor_block.to_string(),
                    cursor_log,
                    end_block: as_of.block_number.to_string(),
                    limit: self.page_size,
                },
            )?;
            if data.events.is_empty() {
                break;
            }
            for row in data.events {
                let block_number = row.block_number.to_u64("CanonicalEvent.blockNumber")?;
                let next = (block_number, i64::from(row.log_index));
                if next <= (cursor_block, cursor_log) {
                    return Err(DataQualityError::msg(
                        "GraphQL event cursor did not advance",
                    ));
                }
                cursor_block = next.0;
                cursor_log = next.1;
                raw_events.push(row);
                if raw_events.len() > self.max_events {
                    return Err(DataQualityError::msg(format!(
                        "account event count exceeded max_events {}; no partial slice was returned",
                        self.max_events
                    )));
                }
            }
            if raw_events.len() % self.page_size as usize != 0 {
                break;
            }
        }

        if raw_events.is_empty() {
            return Err(DataQualityError::msg(format!(
                "account {account_id} has no canonical events in blocks {} through {}",
                coverage.evidence.start_block, as_of.block_number
            )));
        }
        let events = raw_events
            .into_iter()
            .map(parse_canonical_event)
            .collect::<Result<Vec<_>>>()?;
        for event in &events {
            if event.chain_id != chain_id
                || event.account_id != Some(account_id)
                || event.block_number < coverage.evidence.start_block
                || !as_of.includes(event.block_number, event.log_index)
                || event.timestamp_ms > as_of.timestamp_ms
            {
                return Err(DataQualityError::msg(
                    "GraphQL returned an event outside the requested account, coverage, or cutoff",
                ));
            }
        }
        self.verify_event(&coverage.latest_event)?;
        let final_coverage = self.fetch_coverage(chain_id)?;
        if !final_coverage.is_ready
            || final_coverage.evidence.start_block != coverage.evidence.start_block
            || final_coverage.evidence.processed_block < coverage.evidence.processed_block
        {
            return Err(DataQualityError::msg(
                "Envio coverage changed incompatibly while the account slice was read",
            ));
        }
        Ok(AccountEventSlice {
            account_id,
            coverage: coverage.clone(),
            as_of,
            events,
        })
    }

    fn verify_event(&self, expected: &IndexedPoint) -> Result<()> {
        let data: RawVerifyData = self.post(
            VERIFY_EVENT_QUERY,
            &VerifyVariables {
                id: expected.id.clone(),
            },
        )?;
        if data.events.len() != 1 {
            return Err(DataQualityError::msg(
                "the account slice as-of event disappeared during the read",
            ));
        }
        let actual = data.events[0].to_indexed_point()?;
        if actual != *expected {
            return Err(DataQualityError::msg(
                "the account slice as-of event changed during the read",
            ));
        }
        Ok(())
    }

    fn post<T: DeserializeOwned, V: Serialize>(&self, query: &str, variables: &V) -> Result<T> {
        let body = GraphQlRequest { query, variables };
        let mut request = self.agent.post(&self.endpoint);
        if let Some(secret) = &self.admin_secret {
            request = request.header("x-hasura-admin-secret", secret);
        }
        let mut response = request.send_json(&body).map_err(|_| {
            DataQualityError::msg("Envio GraphQL request failed; check endpoint and authentication")
        })?;
        let envelope: GraphQlEnvelope<T> = response.body_mut().read_json().map_err(|err| {
            DataQualityError::msg(format!("Envio GraphQL response was not valid JSON: {err}"))
        })?;
        if let Some(errors) = envelope.errors {
            if !errors.is_empty() {
                return Err(DataQualityError::msg(format!(
                    "Envio GraphQL returned {} error(s)",
                    errors.len()
                )));
            }
        }
        envelope
            .data
            .ok_or_else(|| DataQualityError::msg("Envio GraphQL response contained no data"))
    }
}

#[derive(Serialize)]
struct GraphQlRequest<'a, V> {
    query: &'a str,
    variables: &'a V,
}

#[derive(Deserialize)]
struct GraphQlEnvelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoverageVariables {
    chain_id: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountVariables {
    chain_id: u32,
    account_id: String,
    cursor_block: String,
    cursor_log: i64,
    end_block: String,
    limit: u32,
}

#[derive(Serialize)]
struct VerifyVariables {
    id: String,
}

#[derive(Deserialize)]
struct RawCoverageData {
    #[serde(rename = "_meta")]
    metadata: Vec<RawMetadata>,
    #[serde(rename = "CanonicalEvent")]
    latest_events: Vec<RawIndexedPoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMetadata {
    chain_id: u32,
    start_block: NumericScalar,
    progress_block: NumericScalar,
    source_block: NumericScalar,
    events_processed: NumericScalar,
    is_ready: bool,
}

#[derive(Deserialize)]
struct RawAccountData {
    #[serde(rename = "CanonicalEvent")]
    events: Vec<RawCanonicalEvent>,
}

#[derive(Deserialize)]
struct RawVerifyData {
    #[serde(rename = "CanonicalEvent")]
    events: Vec<RawIndexedPoint>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIndexedPoint {
    id: String,
    block_number: NumericScalar,
    block_hash: String,
    log_index: u32,
    timestamp_ms: NumericScalar,
}

impl RawIndexedPoint {
    fn to_indexed_point(&self) -> Result<IndexedPoint> {
        Ok(IndexedPoint {
            id: required_text(&self.id, "CanonicalEvent.id")?,
            block_number: self.block_number.to_u64("CanonicalEvent.blockNumber")?,
            block_hash: required_text(&self.block_hash, "CanonicalEvent.blockHash")?,
            log_index: self.log_index,
            timestamp_ms: self.timestamp_ms.to_i64("CanonicalEvent.timestampMs")?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum NumericScalar {
    Signed(i64),
    Unsigned(u64),
    Text(String),
}

impl NumericScalar {
    fn to_i128(&self, label: &str) -> Result<i128> {
        match self {
            Self::Signed(value) => Ok(i128::from(*value)),
            Self::Unsigned(value) => Ok(i128::from(*value)),
            Self::Text(value) => value.parse::<i128>().map_err(|_| {
                DataQualityError::msg(format!("{label} is outside the signed 128-bit range"))
            }),
        }
    }

    fn to_u64(&self, label: &str) -> Result<u64> {
        let value = self.to_i128(label)?;
        u64::try_from(value)
            .map_err(|_| DataQualityError::msg(format!("{label} is outside the u64 range")))
    }

    fn to_i64(&self, label: &str) -> Result<i64> {
        let value = self.to_i128(label)?;
        i64::try_from(value)
            .map_err(|_| DataQualityError::msg(format!("{label} is outside the i64 range")))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCanonicalEvent {
    id: String,
    chain_id: u64,
    block_number: NumericScalar,
    block_hash: String,
    parent_hash: String,
    tx_hash: String,
    log_index: u32,
    timestamp_ms: NumericScalar,
    src_address: String,
    abi_event_name: String,
    kind: String,
    account_id: Option<NumericScalar>,
    perpetual_id: Option<u32>,
    position_type: Option<u8>,
    payload_json: String,
    schema_version: String,
    handler_version: String,
    classifier_version: String,
    ingestion_profile: String,
    abi_fingerprint: String,
}

fn parse_canonical_event(row: RawCanonicalEvent) -> Result<CanonicalEvent> {
    validate_provenance_tuple(&row)?;

    let payload_value: serde_json::Value =
        serde_json::from_str(&row.payload_json).map_err(|err| {
            DataQualityError::msg(format!("{} payloadJson is invalid: {err}", row.id))
        })?;
    let payload = payload_value.as_object().ok_or_else(|| {
        DataQualityError::msg(format!("{} payloadJson must be an object", row.id))
    })?;
    let values = Payload::new(&row.abi_event_name, payload);
    let kind = lifecycle_kind(&row.abi_event_name)?;
    if row.kind != envio_kind(kind) {
        return Err(DataQualityError::msg(format!(
            "{} kind {} does not match ABI event {}",
            row.id, row.kind, row.abi_event_name
        )));
    }
    let account_id = row
        .account_id
        .as_ref()
        .map(|value| value.to_u64("CanonicalEvent.accountId"))
        .transpose()?;
    verify_subjects(
        &row.abi_event_name,
        &values,
        account_id,
        row.perpetual_id,
        row.position_type,
    )?;

    let mut event = CanonicalEvent {
        chain_id: row.chain_id,
        block_hash: row.block_hash,
        tx_hash: row.tx_hash,
        log_index: row.log_index,
        block_number: row.block_number.to_u64("CanonicalEvent.blockNumber")?,
        timestamp_ms: row.timestamp_ms.to_i64("CanonicalEvent.timestampMs")?,
        contract_address: row.src_address,
        abi_event_name: row.abi_event_name.clone(),
        kind,
        account_id,
        perpetual_id: row.perpetual_id,
        position_type: row.position_type,
        owner: None,
        leverage_hdths: None,
        lot_lns: None,
        start_lot_lns: None,
        end_lot_lns: None,
        liq_lot_lns: None,
        price_pns: None,
        mark_price_pns: None,
        liq_price_pns: None,
        amount_cns: None,
        balance_cns: None,
        start_balance_cns: None,
        deposit_cns: None,
        start_deposit_cns: None,
        end_deposit_cns: None,
        delta_pnl_cns: None,
        funding_cns: None,
        ins_fee_cns: None,
        prot_fee_cns: None,
        fee_cns: None,
        funding_rate_pct100k: None,
        funding_price_pns: None,
        funding_payment_pns: None,
        funding_sum_pns: None,
        position_fmv_cns: None,
        payment_cns: None,
        amount_owed_cns: None,
        provenance: Some(CanonicalProvenance {
            envio_id: row.id,
            parent_hash: row.parent_hash,
            payload_json: row.payload_json,
            schema_version: row.schema_version,
            handler_version: row.handler_version,
            classifier_version: row.classifier_version,
            ingestion_profile: row.ingestion_profile,
            abi_fingerprint: row.abi_fingerprint,
        }),
    };

    match row.abi_event_name.as_str() {
        "AccountCreated" => event.owner = Some(values.address("account")?),
        "AccountLiquidationCredit" => {
            event.start_balance_cns = Some(values.unsigned("startBalanceCNS")?);
            event.balance_cns = Some(values.unsigned("endBalanceCNS")?);
        }
        "CollateralDeposit"
        | "CollateralWithdrawal"
        | "TransferAccountToProtocol"
        | "TransferProtocolToAccount" => {
            event.amount_cns = Some(values.unsigned("amountCNS")?);
            event.balance_cns = Some(values.unsigned("balanceCNS")?);
        }
        "IncreasePositionCollateral" => {
            event.deposit_cns = Some(values.unsigned("positionDepositCNS")?);
            event.amount_cns = Some(values.unsigned("amountCNS")?);
            event.balance_cns = Some(values.unsigned("balanceCNS")?);
        }
        "PositionCollateralDecreased" => {
            event.mark_price_pns = Some(values.unsigned("markPricePNS")?);
            event.price_pns = Some(values.unsigned("endEntryPricePNS")?);
            event.start_deposit_cns = Some(values.unsigned("startDepositCNS")?);
            event.end_deposit_cns = Some(values.unsigned("endDepositCNS")?);
            event.balance_cns = Some(values.unsigned("balanceCNS")?);
        }
        "PositionLiquidationCredit" => {
            event.start_deposit_cns = Some(values.unsigned("startDepositCNS")?);
            event.end_deposit_cns = Some(values.unsigned("endDepositCNS")?);
        }
        "PositionOpened" | "PositionOpenedV2" => {
            event.leverage_hdths = Some(values.u32("leverageHdths")?);
            event.deposit_cns = Some(values.unsigned("depositCNS")?);
            event.price_pns = Some(values.unsigned("pricePNS")?);
            event.lot_lns = Some(values.unsigned("lotLNS")?);
            event.ins_fee_cns = Some(values.unsigned("insFeeCNS")?);
            event.prot_fee_cns = Some(values.unsigned("protFeeCNS")?);
        }
        "PositionIncreased" | "PositionIncreasedV2" => {
            event.leverage_hdths = Some(values.u32("leverageHdths")?);
            event.start_deposit_cns = Some(values.unsigned("startDepositCNS")?);
            event.end_deposit_cns = Some(values.unsigned("endDepositCNS")?);
            event.price_pns = Some(values.unsigned("pricePNS")?);
            event.start_lot_lns = Some(values.unsigned("startLotLNS")?);
            event.end_lot_lns = Some(values.unsigned("endLotLNS")?);
            event.ins_fee_cns = Some(values.unsigned("insFeeCNS")?);
            event.prot_fee_cns = Some(values.unsigned("protFeeCNS")?);
        }
        "PositionDecreased" => {
            event.start_deposit_cns = Some(values.unsigned("startDepositCNS")?);
            event.end_deposit_cns = Some(values.unsigned("endDepositCNS")?);
            event.start_lot_lns = Some(values.unsigned("startLotLNS")?);
            event.end_lot_lns = Some(values.unsigned("endLotLNS")?);
            event.delta_pnl_cns = Some(values.signed("deltaPnlCNS")?);
            event.funding_cns = Some(values.signed("fundingCNS")?);
        }
        "PositionClosed" => {
            event.price_pns = Some(values.unsigned("pricePNS")?);
            event.delta_pnl_cns = Some(values.signed("deltaPnlCNS")?);
            event.funding_cns = Some(values.signed("fundingCNS")?);
        }
        "PositionLiquidated" => {
            event.mark_price_pns = Some(values.unsigned("markPricePNS")?);
            event.liq_price_pns = Some(values.unsigned("liqPricePNS")?);
            event.liq_lot_lns = Some(values.unsigned("liqLotLNS")?);
            event.end_lot_lns = Some(values.unsigned("posLotLNS")?);
            event.deposit_cns = Some(values.unsigned("posDepositCNS")?);
            event.delta_pnl_cns = Some(values.signed("deltaPnlCNS")?);
            event.funding_cns = Some(values.signed("fundingCNS")?);
            event.balance_cns = Some(values.unsigned("accBalanceCNS")?);
        }
        "PositionDeleveraged" | "PositionDeleveragedV2" => {
            event.mark_price_pns = Some(values.unsigned("markPricePNS")?);
            event.price_pns = Some(values.unsigned("deleveragePricePNS")?);
            event.start_deposit_cns = Some(values.unsigned("startDepositCNS")?);
            event.end_deposit_cns = Some(values.unsigned("endDepositCNS")?);
            event.start_lot_lns = Some(values.unsigned("startLotLNS")?);
            event.end_lot_lns = Some(values.unsigned("endLotLNS")?);
            event.delta_pnl_cns = Some(values.signed("deltaPnlCNS")?);
            event.funding_cns = Some(values.signed("fundingCNS")?);
            event.balance_cns = Some(values.unsigned("balanceCNS")?);
        }
        "PositionInverted" => {
            event.leverage_hdths = Some(values.u32("leverageHdths")?);
            event.start_deposit_cns = Some(values.unsigned("startDepositCNS")?);
            event.end_deposit_cns = Some(values.unsigned("endDepositCNS")?);
            event.price_pns = Some(values.unsigned("pricePNS")?);
            event.start_lot_lns = Some(values.unsigned("startLotLNS")?);
            event.end_lot_lns = Some(values.unsigned("endLotLNS")?);
            event.delta_pnl_cns = Some(values.signed("deltaPnlCNS")?);
            event.funding_cns = Some(values.signed("fundingCNS")?);
            event.ins_fee_cns = Some(values.unsigned("insFeeCNS")?);
            event.prot_fee_cns = Some(values.unsigned("protFeeCNS")?);
        }
        "PositionUnwound" | "PositionUnwoundV2" => {
            event.mark_price_pns = Some(values.unsigned("markPricePNS")?);
            event.price_pns = Some(values.unsigned("pricePNS")?);
            event.lot_lns = Some(values.unsigned("lotLNS")?);
            event.deposit_cns = Some(values.unsigned("depositCNS")?);
            event.position_fmv_cns = Some(values.signed("positionFmvCNS")?);
            event.payment_cns = Some(values.unsigned("paymentCNS")?);
            event.balance_cns = Some(values.unsigned("balanceCNS")?);
        }
        "PositionUnwoundWithoutPayment" | "PositionUnwoundWithoutPaymentV2" => {
            event.mark_price_pns = Some(values.unsigned("markPricePNS")?);
            event.price_pns = Some(values.unsigned("pricePNS")?);
            event.lot_lns = Some(values.unsigned("lotLNS")?);
            event.deposit_cns = Some(values.unsigned("depositCNS")?);
            event.position_fmv_cns = Some(values.signed("positionFmvCNS")?);
            event.amount_owed_cns = Some(values.unsigned("amountOwedCNS")?);
        }
        "FundingEventCompleted" => {
            event.funding_rate_pct100k = Some(values.signed("actualRatePct100k")?);
            event.funding_price_pns = Some(values.unsigned("fundingPricePNS")?);
            event.funding_payment_pns = Some(values.signed("fundingPaymentPNS")?);
            event.funding_sum_pns = Some(values.signed("fundingSumPNS")?);
        }
        "MakerOrderFilled" | "MakerOrderFilledV2" => {
            event.price_pns = Some(values.unsigned("pricePNS")?);
            event.lot_lns = Some(values.unsigned("lotLNS")?);
            event.fee_cns = Some(values.unsigned("feeCNS")?);
            event.amount_cns = Some(values.signed("amountCNS")?);
            event.balance_cns = Some(values.unsigned("balanceCNS")?);
        }
        "ContractAdded" | "ContractAddedV2" => {}
        other => {
            return Err(DataQualityError::msg(format!(
                "unsupported Exchange event projection: {other}"
            )));
        }
    }
    event.validate()?;
    Ok(event)
}

fn verify_subjects(
    abi_event_name: &str,
    payload: &Payload<'_>,
    account_id: Option<u64>,
    perpetual_id: Option<u32>,
    position_type: Option<u8>,
) -> Result<()> {
    let account_field = match abi_event_name {
        "AccountCreated" => Some("id"),
        "CollateralDeposit"
        | "CollateralWithdrawal"
        | "TransferAccountToProtocol"
        | "TransferProtocolToAccount" => Some("accountId"),
        "FundingEventCompleted" | "ContractAdded" | "ContractAddedV2" => None,
        "PositionLiquidated" => Some("posAccountId"),
        _ => Some("accountId"),
    };
    if let Some(field) = account_field {
        let payload_account = payload.u64(field)?;
        if account_id != Some(payload_account) {
            return Err(DataQualityError::msg(format!(
                "{abi_event_name} payload {field} {payload_account} does not match classified account {account_id:?}"
            )));
        }
    } else if account_id.is_some() {
        return Err(DataQualityError::msg(format!(
            "{abi_event_name} unexpectedly has a classified account"
        )));
    }

    let has_perpetual = !matches!(
        abi_event_name,
        "AccountCreated"
            | "CollateralDeposit"
            | "CollateralWithdrawal"
            | "TransferAccountToProtocol"
            | "TransferProtocolToAccount"
    );
    if has_perpetual {
        let payload_perpetual = payload.u32("perpId")?;
        if perpetual_id != Some(payload_perpetual) {
            return Err(DataQualityError::msg(format!(
                "{abi_event_name} payload perpId {payload_perpetual} does not match classified perpetual {perpetual_id:?}"
            )));
        }
    } else if perpetual_id.is_some() {
        return Err(DataQualityError::msg(format!(
            "{abi_event_name} unexpectedly has a classified perpetual"
        )));
    }

    let has_position_type = matches!(
        abi_event_name,
        "PositionCollateralDecreased"
            | "PositionOpened"
            | "PositionOpenedV2"
            | "PositionIncreased"
            | "PositionIncreasedV2"
            | "PositionDecreased"
            | "PositionClosed"
            | "PositionLiquidated"
            | "PositionDeleveraged"
            | "PositionDeleveragedV2"
            | "PositionInverted"
            | "PositionUnwound"
            | "PositionUnwoundV2"
            | "PositionUnwoundWithoutPayment"
            | "PositionUnwoundWithoutPaymentV2"
    );
    if has_position_type {
        let payload_position_type = payload.u8("positionType")?;
        if position_type != Some(payload_position_type) {
            return Err(DataQualityError::msg(format!(
                "{abi_event_name} payload positionType {payload_position_type} does not match classified position type {position_type:?}"
            )));
        }
    } else if position_type.is_some() {
        return Err(DataQualityError::msg(format!(
            "{abi_event_name} unexpectedly has a classified position type"
        )));
    }
    Ok(())
}

fn lifecycle_kind(abi_event_name: &str) -> Result<LifecycleKind> {
    match abi_event_name {
        "AccountCreated" => Ok(LifecycleKind::AccountCreated),
        "AccountLiquidationCredit" => Ok(LifecycleKind::AccountLiquidationCredit),
        "TransferAccountToProtocol" => Ok(LifecycleKind::AccountToProtocolTransfer),
        "CollateralDeposit" => Ok(LifecycleKind::CollateralDeposit),
        "CollateralWithdrawal" => Ok(LifecycleKind::CollateralWithdrawal),
        "IncreasePositionCollateral" => Ok(LifecycleKind::CollateralIncreased),
        "PositionCollateralDecreased" => Ok(LifecycleKind::CollateralDecreased),
        "PositionOpened" | "PositionOpenedV2" => Ok(LifecycleKind::PositionOpened),
        "PositionIncreased" | "PositionIncreasedV2" => Ok(LifecycleKind::PositionIncreased),
        "PositionDecreased" => Ok(LifecycleKind::PositionDecreased),
        "PositionClosed" => Ok(LifecycleKind::PositionClosed),
        "PositionLiquidated" => Ok(LifecycleKind::PositionLiquidated),
        "PositionLiquidationCredit" => Ok(LifecycleKind::PositionLiquidationCredit),
        "PositionDeleveraged" | "PositionDeleveragedV2" => Ok(LifecycleKind::PositionDeleveraged),
        "PositionInverted" => Ok(LifecycleKind::PositionInverted),
        "PositionUnwound"
        | "PositionUnwoundV2"
        | "PositionUnwoundWithoutPayment"
        | "PositionUnwoundWithoutPaymentV2" => Ok(LifecycleKind::PositionUnwound),
        "FundingEventCompleted" => Ok(LifecycleKind::MarketFunding),
        "MakerOrderFilled" | "MakerOrderFilledV2" => Ok(LifecycleKind::MakerFill),
        "ContractAdded" | "ContractAddedV2" => Ok(LifecycleKind::ContractAdded),
        "TransferProtocolToAccount" => Ok(LifecycleKind::ProtocolToAccountTransfer),
        other => Err(DataQualityError::msg(format!(
            "unsupported Exchange ABI event {other}"
        ))),
    }
}

fn envio_kind(kind: LifecycleKind) -> &'static str {
    match kind {
        LifecycleKind::AccountCreated => "ACCOUNT_CREATED",
        LifecycleKind::AccountLiquidationCredit => "ACCOUNT_LIQUIDATION_CREDIT",
        LifecycleKind::AccountToProtocolTransfer => "ACCOUNT_TO_PROTOCOL_TRANSFER",
        LifecycleKind::CollateralDeposit => "COLLATERAL_DEPOSIT",
        LifecycleKind::CollateralWithdrawal => "COLLATERAL_WITHDRAWAL",
        LifecycleKind::PositionOpened => "POSITION_OPENED",
        LifecycleKind::PositionIncreased => "POSITION_INCREASED",
        LifecycleKind::PositionDecreased => "POSITION_DECREASED",
        LifecycleKind::PositionClosed => "POSITION_CLOSED",
        LifecycleKind::PositionLiquidated => "POSITION_LIQUIDATED",
        LifecycleKind::PositionLiquidationCredit => "POSITION_LIQUIDATION_CREDIT",
        LifecycleKind::PositionDeleveraged => "POSITION_DELEVERAGED",
        LifecycleKind::PositionInverted => "POSITION_INVERTED",
        LifecycleKind::PositionUnwound => "POSITION_UNWOUND",
        LifecycleKind::CollateralIncreased => "COLLATERAL_INCREASED",
        LifecycleKind::CollateralDecreased => "COLLATERAL_DECREASED",
        LifecycleKind::MarketFunding => "MARKET_FUNDING",
        LifecycleKind::MakerFill => "MAKER_FILL",
        LifecycleKind::TakerFill => "TAKER_FILL",
        LifecycleKind::OrderRequest => "ORDER_REQUEST",
        LifecycleKind::ContractAdded => "CONTRACT_ADDED",
        LifecycleKind::ProtocolToAccountTransfer => "PROTOCOL_TO_ACCOUNT_TRANSFER",
    }
}

struct Payload<'a> {
    event_name: &'a str,
    values: &'a serde_json::Map<String, serde_json::Value>,
}

impl<'a> Payload<'a> {
    fn new(event_name: &'a str, values: &'a serde_json::Map<String, serde_json::Value>) -> Self {
        Self { event_name, values }
    }

    fn signed(&self, field: &str) -> Result<i128> {
        let value = self.values.get(field).ok_or_else(|| {
            DataQualityError::msg(format!("{}.{} is missing", self.event_name, field))
        })?;
        match value {
            serde_json::Value::String(text) => text.parse::<i128>().map_err(|_| {
                DataQualityError::msg(format!(
                    "{}.{} is outside the signed 128-bit range",
                    self.event_name, field
                ))
            }),
            serde_json::Value::Number(number) => number
                .as_i64()
                .map(i128::from)
                .or_else(|| number.as_u64().map(i128::from))
                .ok_or_else(|| {
                    DataQualityError::msg(format!(
                        "{}.{} must be an integer",
                        self.event_name, field
                    ))
                }),
            _ => Err(DataQualityError::msg(format!(
                "{}.{} must be an integer",
                self.event_name, field
            ))),
        }
    }

    fn unsigned(&self, field: &str) -> Result<i128> {
        let value = self.signed(field)?;
        if value < 0 {
            return Err(DataQualityError::msg(format!(
                "{}.{} must be non-negative",
                self.event_name, field
            )));
        }
        Ok(value)
    }

    fn u64(&self, field: &str) -> Result<u64> {
        u64::try_from(self.unsigned(field)?).map_err(|_| {
            DataQualityError::msg(format!(
                "{}.{} is outside the u64 range",
                self.event_name, field
            ))
        })
    }

    fn u32(&self, field: &str) -> Result<u32> {
        u32::try_from(self.unsigned(field)?).map_err(|_| {
            DataQualityError::msg(format!(
                "{}.{} is outside the u32 range",
                self.event_name, field
            ))
        })
    }

    fn u8(&self, field: &str) -> Result<u8> {
        u8::try_from(self.unsigned(field)?).map_err(|_| {
            DataQualityError::msg(format!(
                "{}.{} is outside the u8 range",
                self.event_name, field
            ))
        })
    }

    fn address(&self, field: &str) -> Result<String> {
        let value = self
            .values
            .get(field)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                DataQualityError::msg(format!(
                    "{}.{} must be an address string",
                    self.event_name, field
                ))
            })?;
        if value.len() != 42
            || !value.starts_with("0x")
            || !value[2..]
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(DataQualityError::msg(format!(
                "{}.{} is not a 20-byte address",
                self.event_name, field
            )));
        }
        Ok(value.to_string())
    }
}

fn require_version(actual: &str, expected: &str, label: &str) -> Result<()> {
    if actual != expected {
        return Err(DataQualityError::msg(format!(
            "unsupported {label} {actual}; expected {expected}"
        )));
    }
    Ok(())
}

fn validate_provenance_tuple(row: &RawCanonicalEvent) -> Result<()> {
    let (handler, classifier, profile, fingerprint) = match row.schema_version.as_str() {
        LEGACY_SCHEMA_VERSION => (
            LEGACY_HANDLER_VERSION,
            LEGACY_CLASSIFIER_VERSION,
            LEGACY_INGESTION_PROFILE,
            LEGACY_ABI_FINGERPRINT,
        ),
        CURRENT_SCHEMA_VERSION => (
            CURRENT_HANDLER_VERSION,
            CURRENT_CLASSIFIER_VERSION,
            CURRENT_INGESTION_PROFILE,
            CURRENT_ABI_FINGERPRINT,
        ),
        other => {
            return Err(DataQualityError::msg(format!(
                "unsupported schemaVersion {other}; expected {LEGACY_SCHEMA_VERSION} or {CURRENT_SCHEMA_VERSION}"
            )))
        }
    };
    require_version(&row.handler_version, handler, "handlerVersion")?;
    require_version(&row.classifier_version, classifier, "classifierVersion")?;
    require_version(&row.ingestion_profile, profile, "ingestionProfile")?;
    require_version(&row.abi_fingerprint, fingerprint, "abiFingerprint")
}

fn required_text(value: &str, label: &str) -> Result<String> {
    if value.trim().is_empty() {
        Err(DataQualityError::msg(format!("{label} is required")))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{load_fixture, repo_root};

    fn raw_event(payload_json: &str) -> RawCanonicalEvent {
        RawCanonicalEvent {
            id: "143:0xblock:0xtx:7".to_string(),
            chain_id: 143,
            block_number: NumericScalar::Text("102500000".to_string()),
            block_hash: "0xblock".to_string(),
            parent_hash: "0xparent".to_string(),
            tx_hash: "0xtx".to_string(),
            log_index: 7,
            timestamp_ms: NumericScalar::Text("1780000000000".to_string()),
            src_address: "0x34B6552d57a35a1D042CcAe1951BD1C370112a6F".to_string(),
            abi_event_name: "PositionLiquidated".to_string(),
            kind: "POSITION_LIQUIDATED".to_string(),
            account_id: Some(NumericScalar::Text("42".to_string())),
            perpetual_id: Some(1),
            position_type: Some(0),
            payload_json: payload_json.to_string(),
            schema_version: CURRENT_SCHEMA_VERSION.to_string(),
            handler_version: CURRENT_HANDLER_VERSION.to_string(),
            classifier_version: CURRENT_CLASSIFIER_VERSION.to_string(),
            ingestion_profile: CURRENT_INGESTION_PROFILE.to_string(),
            abi_fingerprint: CURRENT_ABI_FINGERPRINT.to_string(),
        }
    }

    #[test]
    fn liquidation_projection_uses_post_liquidation_position_values() {
        let event = parse_canonical_event(raw_event(
            r#"{"perpId":"1","posAccountId":"42","positionType":"0","markPricePNS":"700000","liqPricePNS":"625000","liqLotLNS":"50000","posLotLNS":"25000","deltaPnlCNS":"-7500000000","fundingCNS":"-2000000","posAmountCNS":"0","posDepositCNS":"2500000000","accAmountCNS":"0","accBalanceCNS":"9000000000","onOrderBook":false}"#,
        ))
        .expect("liquidation event");
        assert_eq!(event.liq_lot_lns, Some(50_000));
        assert_eq!(event.end_lot_lns, Some(25_000));
        assert_eq!(event.deposit_cns, Some(2_500_000_000));
        assert_eq!(event.delta_pnl_cns, Some(-7_500_000_000));
        assert_eq!(event.balance_cns, Some(9_000_000_000));
        assert_eq!(event.mark_price_pns, Some(700_000));
        assert_eq!(event.position_type, Some(0));
    }

    #[test]
    fn liquidation_credit_projection_preserves_transition_values() {
        let mut raw = raw_event(
            r#"{"perpId":"1","accountId":"42","startDepositCNS":"2500000000","endDepositCNS":"2750000000"}"#,
        );
        raw.abi_event_name = "PositionLiquidationCredit".to_string();
        raw.kind = "POSITION_LIQUIDATION_CREDIT".to_string();
        raw.position_type = None;

        let event = parse_canonical_event(raw).expect("liquidation credit event");
        assert_eq!(event.kind, LifecycleKind::PositionLiquidationCredit);
        assert_eq!(event.start_deposit_cns, Some(2_500_000_000));
        assert_eq!(event.end_deposit_cns, Some(2_750_000_000));
    }

    #[test]
    fn projection_rejects_subject_mismatch_and_unknown_provenance() {
        let payload = r#"{"perpId":"1","posAccountId":"99","positionType":"0","markPricePNS":"700000","liqPricePNS":"625000","liqLotLNS":"50000","posLotLNS":"0","deltaPnlCNS":"-1","fundingCNS":"0","posAmountCNS":"0","posDepositCNS":"0","accAmountCNS":"0","accBalanceCNS":"1","onOrderBook":false}"#;
        let error = parse_canonical_event(raw_event(payload)).expect_err("subject mismatch");
        assert!(error
            .to_string()
            .contains("does not match classified account"));

        let mut raw = raw_event(payload);
        raw.account_id = Some(NumericScalar::Text("99".to_string()));
        raw.schema_version = "canonical-event-v999".to_string();
        let error = parse_canonical_event(raw).expect_err("schema mismatch");
        assert!(error.to_string().contains("unsupported schemaVersion"));

        let mut obsolete = raw_event(payload);
        obsolete.account_id = Some(NumericScalar::Text("99".to_string()));
        obsolete.abi_fingerprint =
            "sha256:16b3a4812e63fd11d543879117f21c48976f8a4ea8c9aa487d7c2ac3fc397482".into();
        let error = parse_canonical_event(obsolete).expect_err("obsolete Windows ABI fingerprint");
        assert!(error.to_string().contains("abiFingerprint"));
    }

    #[test]
    fn client_limits_are_fail_closed() {
        assert!(EnvioClient::new("file:///tmp/data", None, 100, 100).is_err());
        assert!(EnvioClient::new("http://localhost", None, 0, 100).is_err());
        assert!(EnvioClient::new("http://localhost", None, 1001, 100).is_err());
        assert!(EnvioClient::new("http://localhost", None, 100, 0).is_err());
    }

    #[test]
    fn account_birth_is_eligible_only_under_a_complete_ingestion_profile() {
        let fixture = load_fixture(repo_root().join("fixtures/golden/open-position-as-of.json"))
            .expect("fixture");
        let mut events: Vec<CanonicalEvent> = fixture
            .events
            .iter()
            .filter(|event| event.account_id == Some(42))
            .cloned()
            .collect();
        for event in &mut events {
            event.provenance = Some(CanonicalProvenance {
                envio_id: event.event_id().unwrap().key(),
                parent_hash: "0xparent".to_string(),
                payload_json: "{}".to_string(),
                schema_version: LEGACY_SCHEMA_VERSION.to_string(),
                handler_version: LEGACY_HANDLER_VERSION.to_string(),
                classifier_version: LEGACY_CLASSIFIER_VERSION.to_string(),
                ingestion_profile: LEGACY_INGESTION_PROFILE.to_string(),
                abi_fingerprint: LEGACY_ABI_FINGERPRINT.to_string(),
            });
        }
        let latest = events.last().unwrap();
        let slice = AccountEventSlice {
            account_id: 42,
            coverage: EnvioCoverage {
                evidence: CoverageEvidence {
                    chain_id: 143,
                    start_block: fixture.registry.deployed_at_block + 5,
                    processed_block: fixture.as_of.block_number,
                },
                source_block: fixture.as_of.block_number,
                events_processed: events.len() as u64,
                is_ready: true,
                latest_event: IndexedPoint {
                    id: latest.event_id().unwrap().key(),
                    block_number: latest.block_number,
                    block_hash: latest.block_hash.clone(),
                    log_index: latest.log_index,
                    timestamp_ms: latest.timestamp_ms,
                },
            },
            as_of: fixture.as_of.clone(),
            events,
        };

        let blocked = slice.replay_eligibility(&fixture.registry).unwrap();
        assert!(!blocked.eligible);
        assert_eq!(blocked.basis, "ingestion-profile");

        let mut eligible = slice;
        for event in &mut eligible.events {
            let provenance = event.provenance.as_mut().unwrap();
            provenance.schema_version = CURRENT_SCHEMA_VERSION.to_string();
            provenance.handler_version = CURRENT_HANDLER_VERSION.to_string();
            provenance.classifier_version = CURRENT_CLASSIFIER_VERSION.to_string();
            provenance.ingestion_profile = LEDGER_ELIGIBLE_PROFILE.to_string();
            provenance.abi_fingerprint = CURRENT_ABI_FINGERPRINT.to_string();
        }
        let result = eligible.replay_eligibility(&fixture.registry).unwrap();
        assert!(result.eligible);
        assert_eq!(result.basis, "account-birth");
    }
}
