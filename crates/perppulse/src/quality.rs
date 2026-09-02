use crate::error::{DataQualityError, Result};
use crate::identity::AsOf;
use crate::ledger::Ledger;
use crate::metrics::ProtocolMetrics;

#[derive(Clone, Debug)]
pub struct QualityReport {
    pub status: String,
    pub as_of_block: u64,
    pub last_event_block: u64,
    pub lag_blocks: u64,
    pub notes: Vec<String>,
}

pub fn gate_ledger(ledger: &Ledger, as_of: &AsOf, max_lag_blocks: Option<u64>) -> Result<QualityReport> {
    let last = ledger
        .events
        .last()
        .ok_or_else(|| DataQualityError::msg("quality gate rejected an empty ledger"))?;
    if last.block_number > as_of.block_number {
        return Err(DataQualityError::msg("ledger contains events after the as-of block"));
    }
    let lag = as_of.block_number - last.block_number;
    if let Some(max_lag) = max_lag_blocks {
        if lag > max_lag {
            return Err(DataQualityError::msg(format!(
                "ledger is stale: last event block {} lags as-of {} by {lag} blocks (max {max_lag})",
                last.block_number, as_of.block_number
            )));
        }
    }
    let mut notes = Vec::new();
    if lag > 0 {
        notes.push(format!("ledger lag is {lag} blocks"));
    }
    Ok(QualityReport {
        status: "eligible".to_string(),
        as_of_block: as_of.block_number,
        last_event_block: last.block_number,
        lag_blocks: lag,
        notes,
    })
}

pub fn gate_metrics(metrics: &ProtocolMetrics) -> Result<QualityReport> {
    if metrics.markets.is_empty() {
        return Err(DataQualityError::msg("quality gate rejected protocol metrics with no markets"));
    }
    for market in &metrics.markets {
        if market.taker_volume.is_sign_negative() {
            return Err(DataQualityError::msg(format!("{} taker volume is negative", market.symbol)));
        }
        if market.open_interest.is_sign_negative() || market.tvl.is_sign_negative() {
            return Err(DataQualityError::msg(format!("{} open interest or TVL is negative", market.symbol)));
        }
    }
    Ok(QualityReport {
        status: "eligible".to_string(),
        as_of_block: metrics.as_of_block,
        last_event_block: metrics.last_event_block,
        lag_blocks: metrics.as_of_block.saturating_sub(metrics.last_event_block),
        notes: metrics.warnings.clone(),
    })
}
