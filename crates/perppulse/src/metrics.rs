use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;

use crate::accounting::position_snapshot;
use crate::error::{DataQualityError, Result};
use crate::events::{CanonicalEvent, LifecycleKind, MarketMark};
use crate::identity::AsOf;
use crate::ledger::Ledger;
use crate::money::{from_native, notional};

#[derive(Clone, Debug)]
pub struct MarketMetrics {
    pub perpetual_id: u32,
    pub symbol: String,
    pub taker_volume: Decimal,
    pub open_interest: Decimal,
    pub tvl: Decimal,
    pub protocol_fees: Decimal,
    pub insurance_fees: Decimal,
    pub fill_fees: Decimal,
    pub liquidations: u64,
    pub liquidation_notional: Decimal,
    pub active_accounts: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ProtocolMetrics {
    pub as_of_block: u64,
    pub as_of_timestamp_ms: i64,
    pub last_event_block: u64,
    pub last_event_timestamp_ms: i64,
    pub taker_volume: Decimal,
    pub open_interest: Decimal,
    pub tvl: Decimal,
    pub protocol_fees: Decimal,
    pub liquidations: u64,
    pub active_accounts: usize,
    pub markets: Vec<MarketMetrics>,
    pub warnings: Vec<String>,
}

#[derive(Default)]
struct Bucket {
    volume: Decimal,
    open_interest: Decimal,
    tvl: Decimal,
    protocol_fees: Decimal,
    insurance_fees: Decimal,
    fill_fees: Decimal,
    liquidations: u64,
    liquidation_notional: Decimal,
    accounts: BTreeSet<u64>,
}

pub fn protocol_metrics(
    ledger: &Ledger,
    as_of: &AsOf,
    marks: &[MarketMark],
    events: &[CanonicalEvent],
    window_start_ms: Option<i64>,
) -> Result<ProtocolMetrics> {
    if let Some(start) = window_start_ms {
        if start > as_of.timestamp_ms {
            return Err(DataQualityError::msg("metric window starts after the as-of timestamp"));
        }
    }
    let window_events: Vec<&CanonicalEvent> = events
        .iter()
        .filter(|event| {
            as_of.includes(event.block_number, event.log_index)
                && window_start_ms.is_none_or(|start| event.timestamp_ms >= start)
        })
        .collect();
    if window_events.is_empty() {
        return Err(DataQualityError::msg("metric window contains no canonical events"));
    }

    let mut by_market: BTreeMap<u32, Bucket> = BTreeMap::new();
    let collateral_decimals = ledger.registry.collateral.decimals;
    for event in &window_events {
        match event.kind {
            LifecycleKind::MakerFill => {
                let event_id = event.event_id()?.key();
                let perpetual_id = event.perpetual_id.ok_or_else(|| {
                    DataQualityError::msg(format!("maker fill {event_id} is missing perpetual_id"))
                })?;
                let market = ledger.registry.market(perpetual_id)?;
                let bucket = by_market.entry(perpetual_id).or_default();
                let size = from_native(event.lot_lns.unwrap_or(0), market.size_decimals, "fill.size")?;
                let price = from_native(event.price_pns.unwrap_or(0), market.price_decimals, "fill.price")?;
                bucket.volume += notional(price, size, "fill.volume")?;
                bucket.fill_fees += from_native(event.fee_cns.unwrap_or(0), collateral_decimals, "fill.fee")?;
                if let Some(account_id) = event.account_id {
                    bucket.accounts.insert(account_id);
                }
            }
            LifecycleKind::PositionOpened | LifecycleKind::PositionIncreased | LifecycleKind::PositionInverted => {
                let perpetual_id = market_id(event)?;
                let bucket = by_market.entry(perpetual_id).or_default();
                bucket.protocol_fees += from_native(
                    event.ins_fee_cns.unwrap_or(0) + event.prot_fee_cns.unwrap_or(0),
                    collateral_decimals,
                    "protocol_fees",
                )?;
                bucket.insurance_fees +=
                    from_native(event.ins_fee_cns.unwrap_or(0), collateral_decimals, "insurance_fees")?;
                if let Some(account_id) = event.account_id {
                    bucket.accounts.insert(account_id);
                }
            }
            LifecycleKind::PositionLiquidated => {
                let perpetual_id = market_id(event)?;
                let market = ledger.registry.market(perpetual_id)?;
                let bucket = by_market.entry(perpetual_id).or_default();
                bucket.liquidations += 1;
                let size = from_native(event.liq_lot_lns.unwrap_or(0), market.size_decimals, "liq.size")?;
                let price = from_native(event.liq_price_pns.unwrap_or(0), market.price_decimals, "liq.price")?;
                bucket.liquidation_notional += notional(price, size, "liq.notional")?;
                if let Some(account_id) = event.account_id {
                    bucket.accounts.insert(account_id);
                }
            }
            _ => {
                if let (Some(account_id), Some(perpetual_id)) = (event.account_id, event.perpetual_id) {
                    by_market.entry(perpetual_id).or_default().accounts.insert(account_id);
                }
            }
        }
    }

    for position in ledger.open_positions() {
        let snap = position_snapshot(position, &ledger.registry, as_of, marks, true)?;
        let notional_value = snap
            .notional_value
            .ok_or_else(|| DataQualityError::msg(format!("open interest for {} is missing mark-derived notional", snap.symbol)))?;
        let bucket = by_market.entry(snap.perpetual_id).or_default();
        bucket.open_interest += notional_value;
        bucket.tvl += snap.deposit;
        bucket.accounts.insert(snap.account_id);
    }

    let mut warnings = Vec::new();
    let mut markets = Vec::new();
    let mut total_volume = Decimal::ZERO;
    let mut total_oi = Decimal::ZERO;
    let mut total_tvl = Decimal::ZERO;
    let mut total_fees = Decimal::ZERO;
    let mut total_liqs = 0u64;
    let mut accounts = BTreeSet::new();
    for (perpetual_id, bucket) in by_market {
        let symbol = ledger.registry.market(perpetual_id)?.symbol.clone();
        let mut row_warnings = Vec::new();
        if bucket.volume.is_zero() {
            row_warnings.push(
                "taker volume is zero because no maker fills with perpetual_id were present; maker fills are used so volume is not double-counted"
                    .to_string(),
            );
        }
        warnings.extend(row_warnings.iter().cloned());
        markets.push(MarketMetrics {
            perpetual_id,
            symbol,
            taker_volume: bucket.volume,
            open_interest: bucket.open_interest,
            tvl: bucket.tvl,
            protocol_fees: bucket.protocol_fees,
            insurance_fees: bucket.insurance_fees,
            fill_fees: bucket.fill_fees,
            liquidations: bucket.liquidations,
            liquidation_notional: bucket.liquidation_notional,
            active_accounts: bucket.accounts.len(),
            warnings: row_warnings,
        });
        total_volume += bucket.volume;
        total_oi += bucket.open_interest;
        total_tvl += bucket.tvl;
        total_fees += bucket.protocol_fees;
        total_liqs += bucket.liquidations;
        accounts.extend(bucket.accounts);
    }

    let last_event = ledger
        .events
        .last()
        .ok_or_else(|| DataQualityError::msg("ledger has no events"))?;
    Ok(ProtocolMetrics {
        as_of_block: as_of.block_number,
        as_of_timestamp_ms: as_of.timestamp_ms,
        last_event_block: last_event.block_number,
        last_event_timestamp_ms: last_event.timestamp_ms,
        taker_volume: total_volume,
        open_interest: total_oi,
        tvl: total_tvl,
        protocol_fees: total_fees,
        liquidations: total_liqs,
        active_accounts: accounts.len(),
        markets,
        warnings,
    })
}

fn market_id(event: &CanonicalEvent) -> Result<u32> {
    let event_id = event.event_id()?.key();
    event
        .perpetual_id
        .ok_or_else(|| DataQualityError::msg(format!("event {event_id} is missing perpetual_id")))
}
