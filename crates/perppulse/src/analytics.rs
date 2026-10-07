//! Versioned protocol analytics over one canonical event set.
//!
//! Window completeness is proven from indexed coverage, never inferred from the
//! first observed business event. Point-in-time open interest, collateral and
//! skew require position history from Exchange deployment; a bounded index
//! reports them unavailable instead of presenting a partial book as complete.

use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::accounting::{mark_is_eligible, position_snapshot};
use crate::error::{DataQualityError, Result};
use crate::events::{CanonicalEvent, LifecycleKind, MarketMark};
use crate::identity::AsOf;
use crate::ledger::Ledger;
use crate::money::{from_native, notional};
use crate::registry::{ProtocolRegistry, SIDE_LONG};

pub const ANALYTICS_VERSION: &str = "protocol-analytics-v1";
/// Ratios are truncated toward zero at this scale before display or comparison.
pub const RATIO_DECIMALS: u32 = 6;

pub struct WindowSpec {
    pub id: &'static str,
    pub duration_ms: Option<i64>,
}

pub const WINDOWS: [WindowSpec; 4] = [
    WindowSpec {
        id: "24h",
        duration_ms: Some(86_400_000),
    },
    WindowSpec {
        id: "7d",
        duration_ms: Some(604_800_000),
    },
    WindowSpec {
        id: "30d",
        duration_ms: Some(2_592_000_000),
    },
    WindowSpec {
        id: "coverage",
        duration_ms: None,
    },
];

/// Coverage facts that decide whether a window or state metric is complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageBasis {
    pub start_block: u64,
    pub deployed_at_block: u64,
    /// Independently observed timestamp of the first covered block, if any.
    pub start_timestamp_ms: Option<i64>,
}

impl CoverageBasis {
    pub fn starts_at_deployment(&self) -> bool {
        self.start_block <= self.deployed_at_block
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketFlow {
    pub perpetual_id: u32,
    pub symbol: String,
    pub taker_volume: Decimal,
    pub trades: u64,
    pub fill_fees: Decimal,
    pub protocol_fees: Decimal,
    pub insurance_fees: Decimal,
    pub liquidations: u64,
    pub liquidation_notional: Decimal,
    pub active_accounts: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowTotals {
    pub taker_volume: Decimal,
    pub trades: u64,
    pub fill_fees: Decimal,
    pub protocol_fees: Decimal,
    pub insurance_fees: Decimal,
    pub liquidations: u64,
    pub liquidation_notional: Decimal,
    pub collateral_deposits: Decimal,
    pub collateral_withdrawals: Decimal,
    pub net_collateral_flow: Decimal,
    pub active_accounts: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowReport {
    pub id: String,
    pub start_timestamp_ms: Option<i64>,
    pub end_block: u64,
    pub end_log_index: Option<u32>,
    pub end_timestamp_ms: i64,
    pub status: String,
    pub basis: String,
    pub reason: String,
    pub event_count: usize,
    pub first_event_id: Option<String>,
    pub last_event_id: Option<String>,
    pub event_ids_hash: String,
    pub totals: Option<FlowTotals>,
    pub markets: Vec<MarketFlow>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketState {
    pub perpetual_id: u32,
    pub symbol: String,
    pub open_positions: usize,
    pub long_open_interest: Option<Decimal>,
    pub short_open_interest: Option<Decimal>,
    pub open_interest: Option<Decimal>,
    pub position_collateral: Decimal,
    pub skew: Option<Decimal>,
    pub mark: Option<Decimal>,
    pub mark_event_id: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateReport {
    pub status: String,
    pub basis: String,
    pub reason: String,
    pub open_positions: Option<usize>,
    pub long_open_interest: Option<Decimal>,
    pub short_open_interest: Option<Decimal>,
    pub open_interest: Option<Decimal>,
    pub position_collateral: Option<Decimal>,
    pub skew: Option<Decimal>,
    pub markets: Vec<MarketState>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsReport {
    pub version: String,
    pub scope: String,
    pub as_of_block: u64,
    pub as_of_log_index: Option<u32>,
    pub as_of_timestamp_ms: i64,
    pub coverage_start_block: u64,
    pub coverage_start_timestamp_ms: Option<i64>,
    pub history_basis: String,
    pub windows: Vec<WindowReport>,
    pub state: StateReport,
}

impl AnalyticsReport {
    pub fn window(&self, id: &str) -> Option<&WindowReport> {
        self.windows.iter().find(|window| window.id == id)
    }
}

/// Truncated ratio; `None` for a zero denominator instead of a fabricated value.
pub fn ratio(numerator: Decimal, denominator: Decimal) -> Option<Decimal> {
    if denominator.is_zero() {
        return None;
    }
    numerator
        .checked_div(denominator)
        .map(|value| value.round_dp_with_strategy(RATIO_DECIMALS, RoundingStrategy::ToZero))
}

fn add(left: Decimal, right: Decimal, name: &str) -> Result<Decimal> {
    left.checked_add(right)
        .ok_or_else(|| DataQualityError::msg(format!("{name} aggregate overflow")))
}

fn sub(left: Decimal, right: Decimal, name: &str) -> Result<Decimal> {
    left.checked_sub(right)
        .ok_or_else(|| DataQualityError::msg(format!("{name} aggregate overflow")))
}

/// Unknown stays unknown; two known values add with overflow rejection.
fn add_known(left: Option<Decimal>, right: Option<Decimal>, name: &str) -> Result<Option<Decimal>> {
    match (left, right) {
        (Some(a), Some(b)) => add(a, b, name).map(Some),
        _ => Ok(None),
    }
}

fn skew(long: Option<Decimal>, short: Option<Decimal>) -> Result<Option<Decimal>> {
    match (long, short) {
        (Some(l), Some(s)) => Ok(ratio(sub(l, s, "skew")?, add(l, s, "skew")?)),
        _ => Ok(None),
    }
}

/// Analyze one bounded canonical event set at a selected cutoff.
pub fn analyze(
    events: &[CanonicalEvent],
    registry: &ProtocolRegistry,
    as_of: &AsOf,
    basis: &CoverageBasis,
    state: Option<(&Ledger, &[MarketMark])>,
    scope: &str,
) -> Result<AnalyticsReport> {
    registry.require_chain(as_of.chain_id)?;
    if basis.deployed_at_block != registry.deployed_at_block {
        return Err(DataQualityError::msg(
            "analytics coverage basis does not match the registry deployment block",
        ));
    }
    if basis
        .start_timestamp_ms
        .is_some_and(|start| start < 0 || start > as_of.timestamp_ms)
    {
        return Err(DataQualityError::msg(
            "coverage start timestamp is negative or after the selected cutoff",
        ));
    }
    let mut ordered: Vec<&CanonicalEvent> = Vec::with_capacity(events.len());
    for event in events {
        if event.chain_id != as_of.chain_id {
            return Err(DataQualityError::msg(
                "analytics input chain differs from the selected cutoff",
            ));
        }
        if !as_of.includes(event.block_number, event.log_index) {
            continue;
        }
        if event.timestamp_ms > as_of.timestamp_ms {
            return Err(DataQualityError::msg(
                "analytics input timestamp is after the selected cutoff",
            ));
        }
        if event.block_number < basis.start_block {
            return Err(DataQualityError::msg(
                "analytics input precedes indexed coverage",
            ));
        }
        ordered.push(event);
    }
    ordered.sort_by_key(|event| (event.block_number, event.log_index));
    if ordered.windows(2).any(|pair| {
        (pair[0].block_number, pair[0].log_index) == (pair[1].block_number, pair[1].log_index)
    }) {
        return Err(DataQualityError::msg(
            "analytics input contains duplicate block/log positions",
        ));
    }

    let mut windows = Vec::with_capacity(WINDOWS.len());
    for spec in &WINDOWS {
        windows.push(window_report(spec, &ordered, registry, as_of, basis)?);
    }
    let state = match state {
        Some((ledger, marks)) => state_report(ledger, marks, registry, as_of, basis)?,
        None if !basis.starts_at_deployment() => bounded_state(basis),
        None => unavailable_state(
            "no-ledger",
            "No canonical position ledger was supplied for this scope.".to_string(),
        ),
    };
    Ok(AnalyticsReport {
        version: ANALYTICS_VERSION.to_string(),
        scope: scope.to_string(),
        as_of_block: as_of.block_number,
        as_of_log_index: as_of.log_index,
        as_of_timestamp_ms: as_of.timestamp_ms,
        coverage_start_block: basis.start_block,
        coverage_start_timestamp_ms: basis.start_timestamp_ms,
        history_basis: if basis.starts_at_deployment() {
            "deployment-history"
        } else {
            "bounded-coverage"
        }
        .to_string(),
        windows,
        state,
    })
}

fn window_report(
    spec: &WindowSpec,
    ordered: &[&CanonicalEvent],
    registry: &ProtocolRegistry,
    as_of: &AsOf,
    basis: &CoverageBasis,
) -> Result<WindowReport> {
    let start_ms = match spec.duration_ms {
        None => None,
        Some(duration) => Some(
            as_of
                .timestamp_ms
                .checked_sub(duration)
                .ok_or_else(|| DataQualityError::msg("window start underflow"))?,
        ),
    };
    let (complete, basis_name, reason) = match start_ms {
        None => (
            true,
            "covered-range",
            format!(
                "All canonical events from coverage block {} through the selected cutoff.",
                basis.start_block
            ),
        ),
        Some(_) if basis.starts_at_deployment() => (
            true,
            "deployment-history",
            "Coverage begins at Exchange deployment, so no earlier events exist.".to_string(),
        ),
        Some(start) => match basis.start_timestamp_ms {
            Some(covered) if covered <= start => (
                true,
                "observed-coverage-start",
                format!(
                    "Independently observed coverage start time {covered} precedes the window start."
                ),
            ),
            Some(covered) => (
                false,
                "coverage-after-window-start",
                format!(
                    "Coverage starts at block {} (time {covered}) after the window start {start}; totals would be partial.",
                    basis.start_block
                ),
            ),
            None => (
                false,
                "coverage-start-unobserved",
                format!(
                    "The timestamp of coverage start block {} was not independently observed; window completeness is unproven.",
                    basis.start_block
                ),
            ),
        },
    };
    let selected: Vec<&CanonicalEvent> = ordered
        .iter()
        .copied()
        .filter(|event| start_ms.is_none_or(|start| event.timestamp_ms >= start))
        .collect();
    let ids = selected
        .iter()
        .map(|event| event.event_id().map(|id| id.key()))
        .collect::<Result<Vec<_>>>()?;
    let (totals, markets) = if complete {
        let (totals, markets) = flows(&selected, registry)?;
        (Some(totals), markets)
    } else {
        (None, Vec::new())
    };
    Ok(WindowReport {
        id: spec.id.to_string(),
        start_timestamp_ms: start_ms,
        end_block: as_of.block_number,
        end_log_index: as_of.log_index,
        end_timestamp_ms: as_of.timestamp_ms,
        status: if complete { "complete" } else { "incomplete" }.to_string(),
        basis: basis_name.to_string(),
        reason,
        event_count: ids.len(),
        first_event_id: ids.first().cloned(),
        last_event_id: ids.last().cloned(),
        event_ids_hash: crate::evidence::digest(&ids)?,
        totals,
        markets,
    })
}

#[derive(Default)]
struct MarketBucket {
    flow: MarketFlow,
    accounts: BTreeSet<u64>,
}

fn flows(
    events: &[&CanonicalEvent],
    registry: &ProtocolRegistry,
) -> Result<(FlowTotals, Vec<MarketFlow>)> {
    let decimals = registry.collateral.decimals;
    let mut totals = FlowTotals::default();
    let mut accounts = BTreeSet::new();
    let mut markets: BTreeMap<u32, MarketBucket> = BTreeMap::new();
    for event in events {
        let event_id = || event.event_id().map(|id| id.key());
        let market_id = || {
            event.perpetual_id.ok_or_else(|| {
                DataQualityError::msg(format!(
                    "event {} is missing perpetual_id",
                    event_id().unwrap_or_default()
                ))
            })
        };
        match event.kind {
            LifecycleKind::MakerFill => {
                let id = market_id()?;
                let market = registry.market(id)?;
                let size = from_native(
                    event.lot_lns.unwrap_or(0),
                    market.size_decimals,
                    "fill.size",
                )?;
                let price = from_native(
                    event.price_pns.unwrap_or(0),
                    market.price_decimals,
                    "fill.price",
                )?;
                let volume = notional(price, size, "fill.volume")?;
                let fee = from_native(event.fee_cns.unwrap_or(0), decimals, "fill.fee")?;
                let bucket = markets.entry(id).or_default();
                bucket.flow.taker_volume = add(bucket.flow.taker_volume, volume, "volume")?;
                bucket.flow.trades += 1;
                bucket.flow.fill_fees = add(bucket.flow.fill_fees, fee, "fill fees")?;
                totals.taker_volume = add(totals.taker_volume, volume, "volume")?;
                totals.trades += 1;
                totals.fill_fees = add(totals.fill_fees, fee, "fill fees")?;
            }
            LifecycleKind::PositionOpened
            | LifecycleKind::PositionIncreased
            | LifecycleKind::PositionInverted => {
                let id = market_id()?;
                let insurance =
                    from_native(event.ins_fee_cns.unwrap_or(0), decimals, "insurance fee")?;
                let protocol = add(
                    insurance,
                    from_native(event.prot_fee_cns.unwrap_or(0), decimals, "protocol fee")?,
                    "protocol fees",
                )?;
                let bucket = markets.entry(id).or_default();
                bucket.flow.protocol_fees = add(bucket.flow.protocol_fees, protocol, "fees")?;
                bucket.flow.insurance_fees =
                    add(bucket.flow.insurance_fees, insurance, "insurance fees")?;
                totals.protocol_fees = add(totals.protocol_fees, protocol, "fees")?;
                totals.insurance_fees = add(totals.insurance_fees, insurance, "insurance fees")?;
            }
            LifecycleKind::PositionLiquidated => {
                let id = market_id()?;
                let market = registry.market(id)?;
                let size = from_native(
                    event.liq_lot_lns.unwrap_or(0),
                    market.size_decimals,
                    "liq.size",
                )?;
                let price = from_native(
                    event.liq_price_pns.unwrap_or(0),
                    market.price_decimals,
                    "liq.price",
                )?;
                let value = notional(price, size, "liq.notional")?;
                let bucket = markets.entry(id).or_default();
                bucket.flow.liquidations += 1;
                bucket.flow.liquidation_notional =
                    add(bucket.flow.liquidation_notional, value, "liquidations")?;
                totals.liquidations += 1;
                totals.liquidation_notional =
                    add(totals.liquidation_notional, value, "liquidations")?;
            }
            LifecycleKind::CollateralDeposit | LifecycleKind::CollateralWithdrawal => {
                let amount = event.amount_cns.unwrap_or(0);
                if amount < 0 {
                    return Err(DataQualityError::msg(format!(
                        "collateral flow {} has a negative amount",
                        event_id()?
                    )));
                }
                let value = from_native(amount, decimals, "collateral flow")?;
                if event.kind == LifecycleKind::CollateralDeposit {
                    totals.collateral_deposits =
                        add(totals.collateral_deposits, value, "deposits")?;
                } else {
                    totals.collateral_withdrawals =
                        add(totals.collateral_withdrawals, value, "withdrawals")?;
                }
                continue;
            }
            _ => {}
        }
        if is_trading_activity(event.kind) {
            if let Some(account) = event.account_id {
                accounts.insert(account);
                if let Some(id) = event.perpetual_id {
                    markets.entry(id).or_default().accounts.insert(account);
                }
            }
        }
    }
    totals.net_collateral_flow = sub(
        totals.collateral_deposits,
        totals.collateral_withdrawals,
        "net collateral flow",
    )?;
    totals.active_accounts = accounts.len();
    let rows = markets
        .into_iter()
        .map(|(id, bucket)| {
            let mut flow = bucket.flow;
            flow.perpetual_id = id;
            flow.symbol = registry.market(id)?.symbol.clone();
            flow.active_accounts = bucket.accounts.len();
            Ok(flow)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((totals, rows))
}

/// Accounts are active when they trade or change a position; deposits,
/// withdrawals and account creation alone do not count.
fn is_trading_activity(kind: LifecycleKind) -> bool {
    matches!(
        kind,
        LifecycleKind::MakerFill
            | LifecycleKind::PositionOpened
            | LifecycleKind::PositionIncreased
            | LifecycleKind::PositionDecreased
            | LifecycleKind::PositionClosed
            | LifecycleKind::PositionLiquidated
            | LifecycleKind::PositionDeleveraged
            | LifecycleKind::PositionInverted
            | LifecycleKind::PositionUnwound
            | LifecycleKind::CollateralIncreased
            | LifecycleKind::CollateralDecreased
            | LifecycleKind::PositionLiquidationCredit
    )
}

fn unavailable_state(basis: &str, reason: String) -> StateReport {
    StateReport {
        status: "unavailable".to_string(),
        basis: basis.to_string(),
        reason,
        open_positions: None,
        long_open_interest: None,
        short_open_interest: None,
        open_interest: None,
        position_collateral: None,
        skew: None,
        markets: Vec::new(),
    }
}

fn bounded_state(basis: &CoverageBasis) -> StateReport {
    unavailable_state(
        "bounded-coverage",
        format!(
            "Open interest, collateral and skew require position history from Exchange deployment block {}; indexed coverage starts at block {}.",
            basis.deployed_at_block, basis.start_block
        ),
    )
}

struct StateBucket {
    open_positions: usize,
    long: Option<Decimal>,
    short: Option<Decimal>,
    collateral: Decimal,
    mark: Option<Decimal>,
}

fn state_report(
    ledger: &Ledger,
    marks: &[MarketMark],
    registry: &ProtocolRegistry,
    as_of: &AsOf,
    basis: &CoverageBasis,
) -> Result<StateReport> {
    if !basis.starts_at_deployment() {
        return Ok(bounded_state(basis));
    }
    let mut buckets: BTreeMap<u32, StateBucket> = BTreeMap::new();
    for position in ledger.open_positions() {
        let id = position.position_id.perpetual_id;
        let market = registry.market(id)?;
        let mark = marks
            .iter()
            .find(|mark| mark.perpetual_id == id && mark_is_eligible(mark, as_of, market));
        let selected: Vec<MarketMark> = mark.cloned().into_iter().collect();
        let snap = position_snapshot(position, registry, as_of, &selected, false)?;
        let bucket = buckets.entry(id).or_insert(StateBucket {
            open_positions: 0,
            long: Some(Decimal::ZERO),
            short: Some(Decimal::ZERO),
            collateral: Decimal::ZERO,
            mark: snap.mark,
        });
        bucket.open_positions += 1;
        bucket.collateral = add(bucket.collateral, snap.deposit, "position collateral")?;
        match snap.notional_value {
            Some(value) if position.side == SIDE_LONG => {
                bucket.long = add_known(bucket.long, Some(value), "long open interest")?;
            }
            Some(value) => {
                bucket.short = add_known(bucket.short, Some(value), "short open interest")?;
            }
            None => {
                bucket.long = None;
                bucket.short = None;
            }
        }
    }
    let mut markets = Vec::new();
    let mut total_long = Some(Decimal::ZERO);
    let mut total_short = Some(Decimal::ZERO);
    let mut collateral = Decimal::ZERO;
    let mut open_positions = 0usize;
    for (id, bucket) in buckets {
        let open_interest = add_known(bucket.long, bucket.short, "open interest")?;
        let market_skew = skew(bucket.long, bucket.short)?;
        total_long = add_known(total_long, bucket.long, "long open interest")?;
        total_short = add_known(total_short, bucket.short, "short open interest")?;
        collateral = add(collateral, bucket.collateral, "position collateral")?;
        open_positions += bucket.open_positions;
        markets.push(MarketState {
            perpetual_id: id,
            symbol: registry.market(id)?.symbol.clone(),
            open_positions: bucket.open_positions,
            long_open_interest: bucket.long,
            short_open_interest: bucket.short,
            open_interest,
            position_collateral: bucket.collateral,
            skew: market_skew,
            mark: bucket.mark,
            mark_event_id: bucket
                .mark
                .and_then(|_| ledger.mark_event_ids.get(&id).cloned()),
            status: if open_interest.is_some() {
                "complete"
            } else {
                "mark-unavailable"
            }
            .to_string(),
        });
    }
    let open_interest = add_known(total_long, total_short, "open interest")?;
    let complete = markets.iter().all(|market| market.status == "complete");
    Ok(StateReport {
        status: if complete { "complete" } else { "partial" }.to_string(),
        basis: "deployment-history".to_string(),
        reason: if complete {
            "Every open position has an eligible as-of mark.".to_string()
        } else {
            "At least one market lacks an eligible as-of mark; its open interest and skew, and protocol totals, are unavailable.".to_string()
        },
        open_positions: Some(open_positions),
        long_open_interest: total_long,
        short_open_interest: total_short,
        open_interest,
        position_collateral: Some(collateral),
        skew: skew(total_long, total_short)?,
        markets,
    })
}
