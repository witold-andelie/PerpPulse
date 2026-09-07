use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::coverage::CoverageEvidence;
use crate::error::{DataQualityError, Result};
use crate::identity::{AsOf, EventId, PositionId};
use crate::registry::{load_registry, parse_registry_value, ProtocolRegistry};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleKind {
    AccountCreated,
    AccountLiquidationCredit,
    AccountToProtocolTransfer,
    CollateralDeposit,
    CollateralWithdrawal,
    PositionOpened,
    PositionIncreased,
    PositionDecreased,
    PositionClosed,
    PositionLiquidated,
    PositionLiquidationCredit,
    PositionDeleveraged,
    PositionInverted,
    PositionUnwound,
    CollateralIncreased,
    CollateralDecreased,
    MarketFunding,
    MakerFill,
    TakerFill,
    OrderRequest,
    ContractAdded,
    ProtocolToAccountTransfer,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CanonicalProvenance {
    pub envio_id: String,
    pub parent_hash: String,
    pub payload_json: String,
    pub schema_version: String,
    pub handler_version: String,
    pub classifier_version: String,
    pub ingestion_profile: String,
    pub abi_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CanonicalEvent {
    pub chain_id: u64,
    pub block_hash: String,
    pub tx_hash: String,
    pub log_index: u32,
    pub block_number: u64,
    pub timestamp_ms: i64,
    pub contract_address: String,
    pub abi_event_name: String,
    pub kind: LifecycleKind,
    #[serde(default)]
    pub account_id: Option<u64>,
    #[serde(default)]
    pub perpetual_id: Option<u32>,
    #[serde(default)]
    pub position_type: Option<u8>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub leverage_hdths: Option<u32>,
    #[serde(default)]
    pub lot_lns: Option<i128>,
    #[serde(default)]
    pub start_lot_lns: Option<i128>,
    #[serde(default)]
    pub end_lot_lns: Option<i128>,
    #[serde(default)]
    pub liq_lot_lns: Option<i128>,
    #[serde(default)]
    pub price_pns: Option<i128>,
    #[serde(default)]
    pub mark_price_pns: Option<i128>,
    #[serde(default)]
    pub liq_price_pns: Option<i128>,
    #[serde(default)]
    pub amount_cns: Option<i128>,
    #[serde(default)]
    pub balance_cns: Option<i128>,
    #[serde(default)]
    pub start_balance_cns: Option<i128>,
    #[serde(default)]
    pub deposit_cns: Option<i128>,
    #[serde(default)]
    pub start_deposit_cns: Option<i128>,
    #[serde(default)]
    pub end_deposit_cns: Option<i128>,
    #[serde(default)]
    pub delta_pnl_cns: Option<i128>,
    #[serde(default)]
    pub funding_cns: Option<i128>,
    #[serde(default)]
    pub ins_fee_cns: Option<i128>,
    #[serde(default)]
    pub prot_fee_cns: Option<i128>,
    #[serde(default)]
    pub fee_cns: Option<i128>,
    #[serde(default)]
    pub funding_rate_pct100k: Option<i128>,
    #[serde(default)]
    pub funding_price_pns: Option<i128>,
    #[serde(default)]
    pub funding_payment_pns: Option<i128>,
    #[serde(default)]
    pub funding_sum_pns: Option<i128>,
    #[serde(default)]
    pub position_fmv_cns: Option<i128>,
    #[serde(default)]
    pub payment_cns: Option<i128>,
    #[serde(default)]
    pub amount_owed_cns: Option<i128>,
    #[serde(default)]
    pub provenance: Option<CanonicalProvenance>,
}

impl CanonicalEvent {
    pub fn event_id(&self) -> Result<EventId> {
        EventId::new(
            self.chain_id,
            self.block_hash.clone(),
            self.tx_hash.clone(),
            self.log_index,
        )
    }

    pub fn position_id(&self) -> Result<Option<PositionId>> {
        match (self.account_id, self.perpetual_id) {
            (Some(account_id), Some(perpetual_id)) => Ok(Some(PositionId::new(
                self.chain_id,
                account_id,
                perpetual_id,
            )?)),
            _ => Ok(None),
        }
    }

    pub fn sort_key(&self) -> Result<(u64, u32, String)> {
        Ok((self.block_number, self.log_index, self.event_id()?.key()))
    }

    pub fn validate(&self) -> Result<()> {
        let event_id = self.event_id()?;
        if self.timestamp_ms < 0 {
            return Err(DataQualityError::msg("timestamp_ms must be >= 0"));
        }
        if self.contract_address.trim().is_empty() {
            return Err(DataQualityError::msg("contract_address is required"));
        }
        if let Some(position_type) = self.position_type {
            if position_type > 2 {
                return Err(DataQualityError::msg(format!(
                    "event {} has invalid position_type {position_type}",
                    event_id.key()
                )));
            }
        }
        let required: &[&str] = match self.kind {
            LifecycleKind::AccountCreated => &["account_id", "owner"],
            LifecycleKind::AccountLiquidationCredit => &[
                "account_id",
                "perpetual_id",
                "start_balance_cns",
                "balance_cns",
            ],
            LifecycleKind::AccountToProtocolTransfer | LifecycleKind::ProtocolToAccountTransfer => {
                &["account_id", "amount_cns", "balance_cns"]
            }
            LifecycleKind::CollateralDeposit | LifecycleKind::CollateralWithdrawal => {
                &["account_id", "amount_cns", "balance_cns"]
            }
            LifecycleKind::PositionOpened => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "price_pns",
                "lot_lns",
                "deposit_cns",
            ],
            LifecycleKind::PositionIncreased => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "price_pns",
                "start_lot_lns",
                "end_lot_lns",
                "start_deposit_cns",
                "end_deposit_cns",
            ],
            LifecycleKind::PositionDecreased => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "start_lot_lns",
                "end_lot_lns",
                "start_deposit_cns",
                "end_deposit_cns",
                "delta_pnl_cns",
                "funding_cns",
            ],
            LifecycleKind::PositionClosed => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "price_pns",
                "delta_pnl_cns",
                "funding_cns",
            ],
            LifecycleKind::PositionLiquidated => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "liq_price_pns",
                "liq_lot_lns",
                "end_lot_lns",
                "deposit_cns",
                "delta_pnl_cns",
                "funding_cns",
            ],
            LifecycleKind::PositionLiquidationCredit => &[
                "account_id",
                "perpetual_id",
                "start_deposit_cns",
                "end_deposit_cns",
            ],
            LifecycleKind::PositionDeleveraged => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "start_lot_lns",
                "end_lot_lns",
                "start_deposit_cns",
                "end_deposit_cns",
                "delta_pnl_cns",
                "funding_cns",
            ],
            LifecycleKind::PositionInverted => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "price_pns",
                "start_lot_lns",
                "end_lot_lns",
                "start_deposit_cns",
                "end_deposit_cns",
                "delta_pnl_cns",
                "funding_cns",
            ],
            LifecycleKind::PositionUnwound => &[
                "account_id",
                "perpetual_id",
                "position_type",
                "price_pns",
                "lot_lns",
                "deposit_cns",
            ],
            LifecycleKind::CollateralIncreased => {
                &["account_id", "perpetual_id", "deposit_cns", "amount_cns"]
            }
            LifecycleKind::CollateralDecreased => &[
                "account_id",
                "perpetual_id",
                "start_deposit_cns",
                "end_deposit_cns",
            ],
            LifecycleKind::MarketFunding => &["perpetual_id", "funding_rate_pct100k"],
            LifecycleKind::MakerFill => &[
                "account_id",
                "perpetual_id",
                "price_pns",
                "lot_lns",
                "fee_cns",
            ],
            LifecycleKind::TakerFill => &["price_pns", "lot_lns", "fee_cns"],
            LifecycleKind::OrderRequest => &["account_id", "perpetual_id"],
            LifecycleKind::ContractAdded => &["perpetual_id"],
        };
        for field in required {
            if !self.has_field(field) {
                return Err(DataQualityError::msg(format!(
                    "{:?} event {} is missing {field}",
                    self.kind,
                    self.event_id()?.key()
                )));
            }
        }
        if matches!(
            self.kind,
            LifecycleKind::PositionOpened | LifecycleKind::PositionInverted
        ) && !matches!(self.position_type, Some(1 | 2))
        {
            return Err(DataQualityError::msg(format!(
                "event {} requires an explicit long or short position_type",
                event_id.key()
            )));
        }
        if let Some(provenance) = &self.provenance {
            provenance.validate(&event_id.key())?;
        }
        Ok(())
    }

    fn has_field(&self, name: &str) -> bool {
        match name {
            "account_id" => self.account_id.is_some(),
            "perpetual_id" => self.perpetual_id.is_some(),
            "position_type" => self.position_type.is_some(),
            "owner" => self
                .owner
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty()),
            "lot_lns" => self.lot_lns.is_some(),
            "start_lot_lns" => self.start_lot_lns.is_some(),
            "end_lot_lns" => self.end_lot_lns.is_some(),
            "liq_lot_lns" => self.liq_lot_lns.is_some(),
            "price_pns" => self.price_pns.is_some(),
            "mark_price_pns" => self.mark_price_pns.is_some(),
            "liq_price_pns" => self.liq_price_pns.is_some(),
            "amount_cns" => self.amount_cns.is_some(),
            "balance_cns" => self.balance_cns.is_some(),
            "start_balance_cns" => self.start_balance_cns.is_some(),
            "deposit_cns" => self.deposit_cns.is_some(),
            "start_deposit_cns" => self.start_deposit_cns.is_some(),
            "end_deposit_cns" => self.end_deposit_cns.is_some(),
            "delta_pnl_cns" => self.delta_pnl_cns.is_some(),
            "funding_cns" => self.funding_cns.is_some(),
            "fee_cns" => self.fee_cns.is_some(),
            "funding_rate_pct100k" => self.funding_rate_pct100k.is_some(),
            "funding_price_pns" => self.funding_price_pns.is_some(),
            "funding_payment_pns" => self.funding_payment_pns.is_some(),
            "funding_sum_pns" => self.funding_sum_pns.is_some(),
            "position_fmv_cns" => self.position_fmv_cns.is_some(),
            "payment_cns" => self.payment_cns.is_some(),
            "amount_owed_cns" => self.amount_owed_cns.is_some(),
            _ => false,
        }
    }
}

impl CanonicalProvenance {
    fn validate(&self, expected_event_id: &str) -> Result<()> {
        if self.envio_id != expected_event_id {
            return Err(DataQualityError::msg(format!(
                "Envio id {} does not match canonical identity {expected_event_id}",
                self.envio_id
            )));
        }
        for (label, value) in [
            ("parent_hash", &self.parent_hash),
            ("schema_version", &self.schema_version),
            ("handler_version", &self.handler_version),
            ("classifier_version", &self.classifier_version),
            ("ingestion_profile", &self.ingestion_profile),
            ("abi_fingerprint", &self.abi_fingerprint),
        ] {
            if value.trim().is_empty() {
                return Err(DataQualityError::msg(format!(
                    "provenance {label} is required"
                )));
            }
        }
        let payload: serde_json::Value =
            serde_json::from_str(&self.payload_json).map_err(|err| {
                DataQualityError::msg(format!("canonical payload is not valid JSON: {err}"))
            })?;
        if !payload.is_object() {
            return Err(DataQualityError::msg(
                "canonical payload must be a JSON object",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MarketMark {
    pub perpetual_id: u32,
    pub mark_pns: i128,
    #[serde(default)]
    pub oracle_pns: Option<i128>,
    pub block_number: u64,
    pub timestamp_ms: i64,
}

#[derive(Clone, Debug)]
pub struct Fixture {
    pub name: String,
    pub path: PathBuf,
    pub registry: ProtocolRegistry,
    pub as_of: AsOf,
    pub coverage: CoverageEvidence,
    pub events: Vec<CanonicalEvent>,
    pub marks: Vec<MarketMark>,
    pub window_start_ms: Option<i64>,
}

#[derive(Deserialize)]
struct FixtureFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    registry_file: Option<String>,
    #[serde(default)]
    registry: Option<serde_json::Value>,
    as_of: AsOfFile,
    coverage: CoverageEvidence,
    events: Vec<CanonicalEvent>,
    #[serde(default)]
    market_state: Vec<MarketMark>,
    #[serde(default)]
    window_start_ms: Option<i64>,
}

#[derive(Deserialize)]
struct AsOfFile {
    chain_id: u64,
    block_number: u64,
    block_hash: String,
    timestamp_ms: i64,
    #[serde(default)]
    log_index: Option<u32>,
}

pub fn load_fixture(path: impl AsRef<Path>) -> Result<Fixture> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|err| {
        DataQualityError::msg(format!("cannot read fixture {}: {err}", path.display()))
    })?;
    let parsed: FixtureFile = serde_json::from_str(&text).map_err(|err| {
        DataQualityError::msg(format!(
            "fixture {} is not valid JSON: {err}",
            path.display()
        ))
    })?;
    let registry = if let Some(relative) = parsed.registry_file {
        let registry_path = resolve_relative(path, &relative);
        load_registry(registry_path)?
    } else if let Some(value) = parsed.registry {
        parse_registry_value(&value, &path.display().to_string())?
    } else {
        return Err(DataQualityError::msg(format!(
            "fixture {} is missing registry or registry_file",
            path.display()
        )));
    };
    if parsed.events.is_empty() {
        return Err(DataQualityError::msg(format!(
            "fixture {} has no events",
            path.display()
        )));
    }
    for event in &parsed.events {
        event.validate()?;
    }
    parsed.coverage.validate()?;
    if parsed.coverage.chain_id != parsed.as_of.chain_id {
        return Err(DataQualityError::msg(format!(
            "fixture {} coverage chain {} does not match as-of chain {}",
            path.display(),
            parsed.coverage.chain_id,
            parsed.as_of.chain_id
        )));
    }
    Ok(Fixture {
        name: parsed.name.unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("fixture")
                .to_string()
        }),
        path: path.to_path_buf(),
        registry,
        as_of: AsOf::new(
            parsed.as_of.chain_id,
            parsed.as_of.block_number,
            parsed.as_of.block_hash,
            parsed.as_of.timestamp_ms,
            parsed.as_of.log_index,
        )?,
        coverage: parsed.coverage,
        events: parsed.events,
        marks: parsed.market_state,
        window_start_ms: parsed.window_start_ms,
    })
}

fn resolve_relative(fixture: &Path, relative: &str) -> PathBuf {
    let candidate = fixture.parent().unwrap_or(Path::new(".")).join(relative);
    if candidate.exists() {
        candidate
    } else {
        repo_root().join(relative)
    }
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}
