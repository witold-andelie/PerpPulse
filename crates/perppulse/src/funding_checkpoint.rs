//! Funding is applied before size changes in its effective block. Lifecycle
//! settlement/reset semantics follow the pinned MIT Perpl SDK (see NOTICE).
//! A caller must supply complete market-event coverage; a bare event list never
//! proves that missing funding publications or scaling anchors equal zero.
use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;
use serde::Serialize;

use crate::ledger::Ledger;
use crate::registry::{ProtocolRegistry, SIDE_LONG};
use crate::{AsOf, CanonicalEvent, DataQualityError, LifecycleKind, Result};

#[derive(Clone, Debug)]
pub struct FundingCoverage {
    pub start_block: u64,
    pub end_block: u64,
    pub market_ids: Vec<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingCheckpoint {
    pub unsettled_pnl: Option<Decimal>,
    pub reset_event_id: Option<String>,
    pub baseline_event_id: Option<String>,
    pub baseline_effective_block: Option<u64>,
    pub coverage_start_block: Option<u64>,
    pub through_block: Option<u64>,
    pub payment_event_ids: Vec<String>,
    pub settlement_event_ids: Vec<String>,
    pub reason: Option<String>,
}

impl Default for FundingCheckpoint {
    fn default() -> Self {
        Self {
            unsettled_pnl: None,
            reset_event_id: None,
            baseline_event_id: None,
            baseline_effective_block: None,
            coverage_start_block: None,
            through_block: None,
            payment_event_ids: Vec::new(),
            settlement_event_ids: Vec::new(),
            reason: Some("Complete funding publication coverage is unavailable.".into()),
        }
    }
}

struct Payment {
    market: u32,
    effective_block: u64,
    native: i128,
    decimals: Option<u32>,
    event_id: String,
    scaling_event_id: Option<String>,
}

pub(crate) struct FundingEngine {
    coverage: Option<FundingCoverage>,
    /// Coverage from Exchange deployment observes every funding publication,
    /// so no unobserved pre-window schedule can be pending at a reset.
    from_deployment: bool,
    baselines: BTreeMap<u32, (u64, String)>,
    payments: Vec<Payment>,
    next: usize,
}

impl FundingEngine {
    pub fn prepare(
        events: &[CanonicalEvent],
        registry: &ProtocolRegistry,
        as_of: &AsOf,
        coverage: Option<&FundingCoverage>,
    ) -> Result<Self> {
        let mut engine = Self {
            coverage: coverage.cloned(),
            from_deployment: coverage.is_some_and(|c| c.start_block <= registry.deployed_at_block),
            baselines: BTreeMap::new(),
            payments: Vec::new(),
            next: 0,
        };
        let Some(coverage) = coverage else {
            return Ok(engine);
        };
        if coverage.start_block < registry.deployed_at_block
            || coverage.start_block > as_of.block_number
            || coverage.end_block != as_of.block_number
            || coverage.market_ids.is_empty()
            || coverage.market_ids.len() > 20
            || coverage.market_ids.iter().collect::<BTreeSet<_>>().len()
                != coverage.market_ids.len()
        {
            return Err(DataQualityError::msg(
                "Invalid complete funding coverage scope or cutoff",
            ));
        }
        for market in &coverage.market_ids {
            let spec = registry.market(*market)?;
            // This validates overwrites, cumulative continuity, time and identity.
            crate::funding::timeline(events, *market, as_of)?;
            let mut scale = None;
            let mut scaling_event_id = None;
            let mut schedules = BTreeMap::new();
            let mut ordered: Vec<_> = events
                .iter()
                .filter(|e| {
                    e.perpetual_id == Some(*market)
                        && matches!(
                            e.kind,
                            LifecycleKind::MarketFunding | LifecycleKind::FundingScaleUpdated
                        )
                        && as_of.includes(e.block_number, e.log_index)
                })
                .collect();
            ordered.sort_by_key(|e| (e.block_number, e.log_index));
            for event in ordered {
                if event.block_number < coverage.start_block {
                    return Err(DataQualityError::msg(
                        "Funding input precedes its declared coverage",
                    ));
                }
                if event.kind == LifecycleKind::FundingScaleUpdated {
                    scale = event.funding_scaling_exp;
                    scaling_event_id = Some(event.event_id()?.key());
                } else {
                    let target = event
                        .funding_event_block
                        .ok_or_else(|| DataQualityError::msg("Incomplete funding schedule"))?;
                    schedules.insert(
                        target,
                        Payment {
                            market: *market,
                            effective_block: target,
                            native: event.funding_payment_pns.ok_or_else(|| {
                                DataQualityError::msg("Incomplete funding payment")
                            })?,
                            // Freeze units at publication, including for pending payments.
                            decimals: scale.and_then(|exp| exp.checked_add(spec.price_decimals)),
                            event_id: event.event_id()?.key(),
                            scaling_event_id: scaling_event_id.clone(),
                        },
                    );
                }
            }
            if let Some((target, payment)) = schedules.first_key_value() {
                // Perpl has one pending schedule. After this first observed target
                // takes effect, a reset is independent of the pre-window payment.
                engine
                    .baselines
                    .insert(*market, (*target, payment.event_id.clone()));
            }
            engine.payments.extend(schedules.into_values());
        }
        engine
            .payments
            .sort_by_key(|p| (p.effective_block, p.market));
        Ok(engine)
    }

    pub fn advance(&mut self, ledger: &mut Ledger, block: u64) -> Result<()> {
        while let Some(payment) = self
            .payments
            .get(self.next)
            .filter(|p| p.effective_block <= block)
        {
            let size_decimals = ledger.registry.market(payment.market)?.size_decimals;
            for position in ledger
                .positions
                .values_mut()
                .filter(|p| p.is_open() && p.position_id.perpetual_id == payment.market)
            {
                let checkpoint = &mut position.funding_checkpoint;
                checkpoint.payment_event_ids.push(payment.event_id.clone());
                if let Some(id) = &payment.scaling_event_id {
                    if !checkpoint.payment_event_ids.contains(id) {
                        checkpoint.payment_event_ids.push(id.clone());
                    }
                }
                if let Some(prior) = checkpoint.unsettled_pnl {
                    let amount = if payment.native == 0 {
                        Some(Decimal::ZERO)
                    } else {
                        payment
                            .decimals
                            .map(|decimals| {
                                let native = payment
                                    .native
                                    .checked_mul(position.lot_lns)
                                    .and_then(|n| {
                                        if position.side == SIDE_LONG {
                                            n.checked_neg()
                                        } else {
                                            Some(n)
                                        }
                                    })
                                    .ok_or_else(|| {
                                        DataQualityError::msg("Funding native product overflow")
                                    })?;
                                exact_decimal(native, decimals + size_decimals)
                            })
                            .transpose()?
                    };
                    match amount {
                        Some(amount) => checkpoint.unsettled_pnl = Some(exact_add(prior, amount)?),
                        None => {
                            checkpoint.unsettled_pnl = None;
                            checkpoint.reason = Some(
                                "Funding scaling anchor at publication is unavailable.".into(),
                            );
                        }
                    }
                }
            }
            self.next += 1;
        }
        Ok(())
    }

    pub fn after_lifecycle(&self, ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
        let Some(id) = event.position_id()? else {
            return Ok(());
        };
        let Some(position) = ledger.positions.get_mut(&id.key()) else {
            return Ok(());
        };
        match event.kind {
            LifecycleKind::PositionOpened
            | LifecycleKind::PositionIncreased
            | LifecycleKind::PositionInverted => {
                let mut checkpoint = FundingCheckpoint {
                    reset_event_id: Some(event.event_id()?.key()),
                    ..FundingCheckpoint::default()
                };
                if let Some(coverage) = &self.coverage {
                    checkpoint.coverage_start_block = Some(coverage.start_block);
                    checkpoint.through_block = Some(coverage.end_block);
                    let baseline = self.baselines.get(&id.perpetual_id);
                    if let Some((block, source)) = baseline {
                        checkpoint.baseline_effective_block = Some(*block);
                        checkpoint.baseline_event_id = Some(source.clone());
                    }
                    if self.from_deployment && coverage.market_ids.contains(&id.perpetual_id) {
                        checkpoint.unsettled_pnl = Some(Decimal::ZERO);
                        checkpoint.reason = None;
                    } else if let Some((baseline, _)) = baseline {
                        if event.block_number >= *baseline {
                            checkpoint.unsettled_pnl = Some(Decimal::ZERO);
                            checkpoint.reason = None;
                        } else {
                            checkpoint.reason = Some("Reset precedes the first effective funding schedule; pre-window pending funding is unverified.".into());
                        }
                    } else {
                        checkpoint.reason = Some(
                            "No effective funding baseline proves the pre-window pending schedule."
                                .into(),
                        );
                    }
                }
                position.funding_checkpoint = checkpoint;
            }
            LifecycleKind::PositionDecreased
            | LifecycleKind::PositionLiquidated
            | LifecycleKind::PositionDeleveraged => {
                let checkpoint = &mut position.funding_checkpoint;
                checkpoint
                    .settlement_event_ids
                    .push(event.event_id()?.key());
                if let Some(prior) = checkpoint.unsettled_pnl {
                    let settled = crate::money::from_native(
                        event
                            .funding_cns
                            .ok_or_else(|| DataQualityError::msg("Missing funding settlement"))?,
                        ledger.registry.collateral.decimals,
                        "settled funding",
                    )?;
                    checkpoint.unsettled_pnl = Some(exact_add(prior, -settled)?);
                }
            }
            _ => {}
        }
        if !position.is_open() {
            position.funding_checkpoint.unsettled_pnl = Some(Decimal::ZERO);
            position.funding_checkpoint.reason = None;
        }
        Ok(())
    }
}

fn exact_add(a: Decimal, b: Decimal) -> Result<Decimal> {
    let scale = a.scale().max(b.scale());
    let aligned = |n: Decimal| n.mantissa().checked_mul(10i128.pow(scale - n.scale()));
    let native = aligned(a)
        .and_then(|a| aligned(b).and_then(|b| a.checked_add(b)))
        .ok_or_else(|| DataQualityError::msg("Exact funding accumulation overflow"))?;
    exact_decimal(native, scale)
}

fn exact_decimal(mut native: i128, mut decimals: u32) -> Result<Decimal> {
    while decimals > 0 && native % 10 == 0 {
        native /= 10;
        decimals -= 1;
    }
    if decimals > 28 {
        return Err(DataQualityError::msg(
            "Funding exceeds exact decimal precision",
        ));
    }
    Decimal::try_from_i128_with_scale(native, decimals)
        .map_err(|_| DataQualityError::msg("Funding exceeds exact decimal range"))
}
