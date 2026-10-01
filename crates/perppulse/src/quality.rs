use crate::coverage::CoverageEvidence;
use crate::error::{DataQualityError, Result};
use crate::identity::AsOf;
use crate::ledger::Ledger;
use crate::metrics::ProtocolMetrics;

#[derive(Clone, Debug)]
pub struct QualityReport {
    pub status: String,
    pub as_of_block: u64,
    pub processed_block: u64,
    pub last_event_block: u64,
    pub coverage_lag_blocks: u64,
    pub event_silence_blocks: u64,
    pub notes: Vec<String>,
}

pub fn gate_ledger(
    ledger: &Ledger,
    as_of: &AsOf,
    coverage: &CoverageEvidence,
    max_lag_blocks: Option<u64>,
) -> Result<QualityReport> {
    coverage.validate()?;
    if coverage.chain_id != as_of.chain_id {
        return Err(DataQualityError::msg(format!(
            "coverage chain {} does not match as-of chain {}",
            coverage.chain_id, as_of.chain_id
        )));
    }
    let last = ledger
        .events
        .last()
        .ok_or_else(|| DataQualityError::msg("quality gate rejected an empty ledger"))?;
    if last.block_number > as_of.block_number {
        return Err(DataQualityError::msg(
            "ledger contains events after the as-of block",
        ));
    }
    if last.block_number > coverage.processed_block {
        return Err(DataQualityError::msg(format!(
            "last event block {} exceeds processed coverage {}",
            last.block_number, coverage.processed_block
        )));
    }
    if ledger
        .events
        .iter()
        .any(|event| event.block_number < coverage.start_block)
    {
        return Err(DataQualityError::msg(
            "ledger contains events before processed coverage begins",
        ));
    }
    let coverage_lag = as_of.block_number.saturating_sub(coverage.processed_block);
    if let Some(max_lag) = max_lag_blocks {
        if coverage_lag > max_lag {
            return Err(DataQualityError::msg(format!(
                "processed coverage is stale: block {} lags as-of {} by {coverage_lag} blocks (max {max_lag})",
                coverage.processed_block, as_of.block_number
            )));
        }
    }
    let event_silence = coverage.processed_block.saturating_sub(last.block_number);
    let mut notes = Vec::new();
    if coverage_lag > 0 {
        notes.push(format!("processed coverage lag is {coverage_lag} blocks"));
    }
    if event_silence > 0 {
        notes.push(format!(
            "no matching canonical event was observed in the last {event_silence} processed blocks"
        ));
    }
    Ok(QualityReport {
        status: "eligible".to_string(),
        as_of_block: as_of.block_number,
        processed_block: coverage.processed_block,
        last_event_block: last.block_number,
        coverage_lag_blocks: coverage_lag,
        event_silence_blocks: event_silence,
        notes,
    })
}

pub fn gate_metrics(metrics: &ProtocolMetrics) -> Result<()> {
    if metrics.markets.is_empty() {
        return Err(DataQualityError::msg(
            "quality gate rejected protocol metrics with no markets",
        ));
    }
    for market in &metrics.markets {
        if market.taker_volume.is_sign_negative() {
            return Err(DataQualityError::msg(format!(
                "{} taker volume is negative",
                market.symbol
            )));
        }
        if market.open_interest.is_sign_negative() || market.tvl.is_sign_negative() {
            return Err(DataQualityError::msg(format!(
                "{} open interest or TVL is negative",
                market.symbol
            )));
        }
    }
    Ok(())
}
