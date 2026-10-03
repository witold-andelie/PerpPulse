use std::collections::{BTreeMap, BTreeSet};

use crate::error::{DataQualityError, Result};
use crate::events::{CanonicalEvent, LifecycleKind, MarketMark};
use crate::identity::{AsOf, EventId, PositionId};
use crate::registry::{ProtocolRegistry, SIDE_LONG, SIDE_SHORT};

#[derive(Clone, Debug)]
pub struct AccountState {
    pub account_id: u64,
    pub owner: Option<String>,
    pub balance_cns: i128,
    pub last_event_id: Option<EventId>,
}

#[derive(Clone, Debug)]
pub struct PositionState {
    pub position_id: PositionId,
    pub side: u8,
    pub status: String,
    pub lot_lns: i128,
    pub entry_pns: i128,
    pub entry_residue_pnsq16: u32,
    pub deposit_cns: i128,
    pub leverage_hdths: u32,
    pub realized_pnl_cns: i128,
    pub realized_funding_cns: i128,
    pub fees_cns: i128,
    pub opened_block: u64,
    pub closed_block: Option<u64>,
    pub last_event_id: Option<EventId>,
}

impl PositionState {
    pub fn is_open(&self) -> bool {
        self.status == "open" && self.lot_lns > 0
    }
}

#[derive(Clone, Debug)]
pub struct FillRecord {
    pub event_id: EventId,
    pub perpetual_id: Option<u32>,
    pub account_id: Option<u64>,
    pub liquidity: String,
    pub price_pns: i128,
    pub lot_lns: i128,
    pub fee_cns: i128,
    pub block_number: u64,
    pub timestamp_ms: i64,
}

#[derive(Clone, Debug)]
pub struct Ledger {
    pub registry: ProtocolRegistry,
    pub accounts: BTreeMap<u64, AccountState>,
    pub positions: BTreeMap<String, PositionState>,
    pub fills: Vec<FillRecord>,
    pub events: Vec<CanonicalEvent>,
    pub market_marks: BTreeMap<u32, MarketMark>,
    pub mark_event_ids: BTreeMap<u32, String>,
}

impl Ledger {
    pub fn open_positions(&self) -> Vec<&PositionState> {
        self.positions
            .values()
            .filter(|position| position.is_open())
            .collect()
    }
}

pub fn replay(
    events: &[CanonicalEvent],
    registry: &ProtocolRegistry,
    as_of: &AsOf,
) -> Result<Ledger> {
    registry.require_chain(as_of.chain_id)?;
    let mut ordered = events.to_vec();
    ordered.sort_by_key(|event| (event.block_number, event.log_index, event.tx_hash.clone()));

    let mut ledger = Ledger {
        registry: registry.clone(),
        accounts: BTreeMap::new(),
        positions: BTreeMap::new(),
        fills: Vec::new(),
        events: Vec::new(),
        market_marks: BTreeMap::new(),
        mark_event_ids: BTreeMap::new(),
    };
    let mut seen = BTreeSet::new();
    let mut previous: Option<(u64, u32)> = None;

    for event in ordered {
        event.validate()?;
        let event_id = event.event_id()?;
        if event_id.chain_id != as_of.chain_id {
            return Err(DataQualityError::msg(format!(
                "event {} chain_id {} != as-of {}",
                event_id.key(),
                event_id.chain_id,
                as_of.chain_id
            )));
        }
        if !as_of.includes(event.block_number, event.log_index) {
            continue;
        }
        if event.timestamp_ms > as_of.timestamp_ms {
            return Err(DataQualityError::msg(
                "event timestamp is after the as-of cutoff",
            ));
        }
        if !event
            .contract_address
            .eq_ignore_ascii_case(&registry.exchange_address)
        {
            return Err(DataQualityError::msg(
                "event contract does not match the Exchange registry",
            ));
        }
        if !seen.insert(event_id.key()) {
            return Err(DataQualityError::msg(format!(
                "duplicate event identity {}",
                event_id.key()
            )));
        }
        let current = (event.block_number, event.log_index);
        if previous.is_some_and(|prior| current < prior) {
            return Err(DataQualityError::msg(format!(
                "event {} is out of order",
                event_id.key()
            )));
        }
        if event.block_number < registry.deployed_at_block {
            return Err(DataQualityError::msg(format!(
                "event {} block {} is before deployment {}",
                event_id.key(),
                event.block_number,
                registry.deployed_at_block
            )));
        }
        apply(&mut ledger, &event)?;
        ledger.events.push(event);
        previous = Some(current);
    }
    if ledger.events.is_empty() {
        return Err(DataQualityError::msg(
            "no canonical events are included at the requested as-of cutoff",
        ));
    }
    Ok(ledger)
}

fn apply(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    if let Some(perpetual_id) = event.perpetual_id {
        if event.kind != LifecycleKind::ContractAdded {
            ledger.registry.market(perpetual_id)?;
        }
    }
    match event.kind {
        LifecycleKind::MarkUpdated => {
            let id = event
                .perpetual_id
                .ok_or_else(|| DataQualityError::msg("MarkUpdated perpetual_id is required"))?;
            ledger.market_marks.insert(
                id,
                MarketMark {
                    perpetual_id: id,
                    mark_pns: required(event.mark_price_pns, "mark_price_pns")?,
                    oracle_pns: None,
                    block_number: event.block_number,
                    timestamp_ms: event.timestamp_ms,
                    log_index: Some(event.log_index),
                    block_hash: Some(event.block_hash.clone()),
                },
            );
            ledger.mark_event_ids.insert(id, event.event_id()?.key());
        }
        LifecycleKind::AccountCreated => {
            let account = account(ledger, event)?;
            account.owner = event.owner.clone();
            account.balance_cns = 0;
        }
        LifecycleKind::CollateralDeposit | LifecycleKind::CollateralWithdrawal => {
            account(ledger, event)?.balance_cns = required(event.balance_cns, "balance_cns")?;
        }
        LifecycleKind::PositionOpened => open_position(ledger, event)?,
        LifecycleKind::PositionIncreased => increase(ledger, event)?,
        LifecycleKind::PositionDecreased => decrease(ledger, event)?,
        LifecycleKind::PositionClosed => close(ledger, event, "closed")?,
        LifecycleKind::PositionLiquidated => liquidate(ledger, event)?,
        LifecycleKind::PositionLiquidationCredit => liquidation_credit(ledger, event)?,
        LifecycleKind::PositionDeleveraged => delever(ledger, event)?,
        LifecycleKind::PositionInverted => invert(ledger, event)?,
        LifecycleKind::PositionUnwound => close(ledger, event, "unwound")?,
        LifecycleKind::CollateralIncreased => {
            let position = require_open(ledger, event)?;
            position.deposit_cns = required(event.deposit_cns, "deposit_cns")?;
        }
        LifecycleKind::CollateralDecreased => {
            let start_deposit = required(event.start_deposit_cns, "start_deposit_cns")?;
            let end_deposit = required(event.end_deposit_cns, "end_deposit_cns")?;
            let position = require_open(ledger, event)?;
            if position.deposit_cns != start_deposit {
                return Err(DataQualityError::msg(format!(
                    "collateral decrease {} start deposit {start_deposit} != ledger deposit {}",
                    event.event_id()?.key(),
                    position.deposit_cns
                )));
            }
            position.deposit_cns = end_deposit;
            if let Some(price_pns) = event.price_pns {
                position.entry_pns = price_pns;
                position.entry_residue_pnsq16 = 0;
            }
        }
        LifecycleKind::MakerFill => ledger.fills.push(fill(event, "maker")?),
        LifecycleKind::TakerFill => ledger.fills.push(fill(event, "taker")?),
        LifecycleKind::AccountLiquidationCredit
        | LifecycleKind::AccountToProtocolTransfer
        | LifecycleKind::ProtocolToAccountTransfer
        | LifecycleKind::MarketFunding
        | LifecycleKind::FundingScaleUpdated
        | LifecycleKind::OrderRequest
        | LifecycleKind::ContractAdded => {}
    }
    if event.account_id.is_some() {
        let account = account(ledger, event)?;
        if let Some(balance_cns) = event.balance_cns {
            account.balance_cns = balance_cns;
        }
        account.last_event_id = Some(event.event_id()?);
    }
    Ok(())
}

fn liquidation_credit(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    let start_deposit = required(event.start_deposit_cns, "start_deposit_cns")?;
    let end_deposit = required(event.end_deposit_cns, "end_deposit_cns")?;
    let position = require_open(ledger, event)?;
    if position.deposit_cns != start_deposit {
        return Err(DataQualityError::msg(format!(
            "liquidation credit {} start deposit {start_deposit} != ledger deposit {}",
            event.event_id()?.key(),
            position.deposit_cns
        )));
    }
    position.deposit_cns = end_deposit;
    position.last_event_id = Some(event.event_id()?);
    require_open_invariants(position, event)
}

fn account<'a>(ledger: &'a mut Ledger, event: &CanonicalEvent) -> Result<&'a mut AccountState> {
    let account_id = event
        .account_id
        .ok_or_else(|| DataQualityError::msg("account_id is required"))?;
    Ok(ledger
        .accounts
        .entry(account_id)
        .or_insert_with(|| AccountState {
            account_id,
            owner: None,
            balance_cns: 0,
            last_event_id: None,
        }))
}

fn open_position(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    let position_id = require_position_id(event)?;
    if ledger
        .positions
        .get(&position_id.key())
        .is_some_and(PositionState::is_open)
    {
        return Err(DataQualityError::msg(format!(
            "cannot open an already-open position {}",
            position_id.key()
        )));
    }
    let previous = ledger.positions.get(&position_id.key());
    let position = PositionState {
        position_id: position_id.clone(),
        side: side(event)?,
        status: "open".to_string(),
        lot_lns: required(event.lot_lns, "lot_lns")?,
        entry_pns: required(event.price_pns, "price_pns")?,
        entry_residue_pnsq16: event.price_residue_pnsq16.unwrap_or(0),
        deposit_cns: required(event.deposit_cns, "deposit_cns")?,
        leverage_hdths: event.leverage_hdths.unwrap_or(0),
        realized_pnl_cns: previous.map_or(0, |position| position.realized_pnl_cns),
        realized_funding_cns: previous.map_or(0, |position| position.realized_funding_cns),
        fees_cns: previous
            .map_or(0, |position| position.fees_cns)
            .checked_add(fees(event)?)
            .ok_or_else(|| DataQualityError::msg("cumulative position fees overflow"))?,
        opened_block: event.block_number,
        closed_block: None,
        last_event_id: Some(event.event_id()?),
    };
    require_open_invariants(&position, event)?;
    ledger.positions.insert(position_id.key(), position);
    Ok(())
}

fn increase(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    let start_lot = required(event.start_lot_lns, "start_lot_lns")?;
    let start_deposit = required(event.start_deposit_cns, "start_deposit_cns")?;
    let end_lot = required(event.end_lot_lns, "end_lot_lns")?;
    let position = require_open(ledger, event)?;
    assert_transition(position, event, start_lot, start_deposit)?;
    if end_lot <= position.lot_lns {
        return Err(DataQualityError::msg(format!(
            "increase {} does not increase size",
            event.event_id()?.key()
        )));
    }
    position.lot_lns = end_lot;
    position.entry_pns = required(event.price_pns, "price_pns")?;
    position.entry_residue_pnsq16 = event.price_residue_pnsq16.unwrap_or(0);
    position.deposit_cns = required(event.end_deposit_cns, "end_deposit_cns")?;
    if let Some(leverage) = event.leverage_hdths {
        position.leverage_hdths = leverage;
    }
    position.fees_cns = accumulate(position.fees_cns, fees(event)?)?;
    if let Some(funding) = event.funding_cns {
        position.realized_funding_cns = accumulate(position.realized_funding_cns, funding)?;
    }
    position.last_event_id = Some(event.event_id()?);
    require_open_invariants(position, event)
}

fn decrease(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    let start_lot = required(event.start_lot_lns, "start_lot_lns")?;
    let start_deposit = required(event.start_deposit_cns, "start_deposit_cns")?;
    let end_lot = required(event.end_lot_lns, "end_lot_lns")?;
    let delta = required(event.delta_pnl_cns, "delta_pnl_cns")?;
    let funding = required(event.funding_cns, "funding_cns")?;
    let end_deposit = required(event.end_deposit_cns, "end_deposit_cns")?;
    let position = require_open(ledger, event)?;
    assert_transition(position, event, start_lot, start_deposit)?;
    if end_lot >= position.lot_lns {
        return Err(DataQualityError::msg(format!(
            "decrease {} does not reduce size",
            event.event_id()?.key()
        )));
    }
    position.lot_lns = end_lot;
    position.deposit_cns = end_deposit;
    position.realized_pnl_cns = accumulate(position.realized_pnl_cns, delta)?;
    position.realized_funding_cns = accumulate(position.realized_funding_cns, funding)?;
    position.last_event_id = Some(event.event_id()?);
    if end_lot == 0 {
        position.status = "closed".to_string();
        position.closed_block = Some(event.block_number);
        position.deposit_cns = 0;
        Ok(())
    } else {
        require_open_invariants(position, event)
    }
}

fn close(ledger: &mut Ledger, event: &CanonicalEvent, status: &str) -> Result<()> {
    let position = require_open(ledger, event)?;
    if let Some(delta) = event.delta_pnl_cns {
        position.realized_pnl_cns = accumulate(position.realized_pnl_cns, delta)?;
    }
    if let Some(funding) = event.funding_cns {
        position.realized_funding_cns = accumulate(position.realized_funding_cns, funding)?;
    }
    position.lot_lns = 0;
    position.deposit_cns = 0;
    position.status = status.to_string();
    position.closed_block = Some(event.block_number);
    position.last_event_id = Some(event.event_id()?);
    Ok(())
}

fn liquidate(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    let remaining = required(event.end_lot_lns, "end_lot_lns")?;
    let delta = required(event.delta_pnl_cns, "delta_pnl_cns")?;
    let funding = required(event.funding_cns, "funding_cns")?;
    let deposit = required(event.deposit_cns, "deposit_cns")?;
    let position = require_open(ledger, event)?;
    position.realized_pnl_cns = accumulate(position.realized_pnl_cns, delta)?;
    position.realized_funding_cns = accumulate(position.realized_funding_cns, funding)?;
    position.lot_lns = remaining;
    position.deposit_cns = if remaining > 0 { deposit } else { 0 };
    position.last_event_id = Some(event.event_id()?);
    if remaining == 0 {
        position.status = "liquidated".to_string();
        position.closed_block = Some(event.block_number);
        Ok(())
    } else {
        position.status = "open".to_string();
        require_open_invariants(position, event)
    }
}

fn delever(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    let start_lot = required(event.start_lot_lns, "start_lot_lns")?;
    let start_deposit = required(event.start_deposit_cns, "start_deposit_cns")?;
    let remaining = required(event.end_lot_lns, "end_lot_lns")?;
    let delta = required(event.delta_pnl_cns, "delta_pnl_cns")?;
    let funding = required(event.funding_cns, "funding_cns")?;
    let end_deposit = required(event.end_deposit_cns, "end_deposit_cns")?;
    let position = require_open(ledger, event)?;
    assert_transition(position, event, start_lot, start_deposit)?;
    position.realized_pnl_cns = accumulate(position.realized_pnl_cns, delta)?;
    position.realized_funding_cns = accumulate(position.realized_funding_cns, funding)?;
    position.lot_lns = remaining;
    position.deposit_cns = if remaining > 0 { end_deposit } else { 0 };
    position.last_event_id = Some(event.event_id()?);
    if remaining == 0 {
        position.status = "deleveraged".to_string();
        position.closed_block = Some(event.block_number);
        Ok(())
    } else {
        require_open_invariants(position, event)
    }
}

fn invert(ledger: &mut Ledger, event: &CanonicalEvent) -> Result<()> {
    let start_lot = required(event.start_lot_lns, "start_lot_lns")?;
    let start_deposit = required(event.start_deposit_cns, "start_deposit_cns")?;
    let end_lot = required(event.end_lot_lns, "end_lot_lns")?;
    let price = required(event.price_pns, "price_pns")?;
    let end_deposit = required(event.end_deposit_cns, "end_deposit_cns")?;
    let delta = required(event.delta_pnl_cns, "delta_pnl_cns")?;
    let funding = required(event.funding_cns, "funding_cns")?;
    let next_side = side(event)?;
    let extra_fees = fees(event)?;
    let leverage = event.leverage_hdths;
    let position = require_open_position(ledger, event)?;
    assert_transition(position, event, start_lot, start_deposit)?;
    if position.side == next_side {
        return Err(DataQualityError::msg(format!(
            "inversion {} does not change position side",
            event.event_id()?.key()
        )));
    }
    position.side = next_side;
    position.lot_lns = end_lot;
    position.entry_pns = price;
    position.entry_residue_pnsq16 = 0;
    position.deposit_cns = end_deposit;
    position.realized_pnl_cns = accumulate(position.realized_pnl_cns, delta)?;
    position.realized_funding_cns = accumulate(position.realized_funding_cns, funding)?;
    position.fees_cns = accumulate(position.fees_cns, extra_fees)?;
    if let Some(value) = leverage {
        position.leverage_hdths = value;
    }
    position.last_event_id = Some(event.event_id()?);
    require_open_invariants(position, event)
}

fn fill(event: &CanonicalEvent, liquidity: &str) -> Result<FillRecord> {
    Ok(FillRecord {
        event_id: event.event_id()?,
        perpetual_id: event.perpetual_id,
        account_id: event.account_id,
        liquidity: liquidity.to_string(),
        price_pns: required(event.price_pns, "price_pns")?,
        lot_lns: required(event.lot_lns, "lot_lns")?,
        fee_cns: required(event.fee_cns, "fee_cns")?,
        block_number: event.block_number,
        timestamp_ms: event.timestamp_ms,
    })
}

fn require_position_id(event: &CanonicalEvent) -> Result<PositionId> {
    let event_id = event.event_id()?.key();
    event.position_id()?.ok_or_else(|| {
        DataQualityError::msg(format!("event {event_id} is missing position identity"))
    })
}

fn require_open<'a>(
    ledger: &'a mut Ledger,
    event: &CanonicalEvent,
) -> Result<&'a mut PositionState> {
    let position = require_open_position(ledger, event)?;
    if let Some(side) = event.position_type {
        if side != 0 && side != position.side {
            return Err(DataQualityError::msg(format!(
                "event {} side {side} != position side {}",
                event.event_id()?.key(),
                position.side
            )));
        }
    }
    Ok(position)
}

fn require_open_position<'a>(
    ledger: &'a mut Ledger,
    event: &CanonicalEvent,
) -> Result<&'a mut PositionState> {
    let position_id = require_position_id(event)?;
    let key = position_id.key();
    let position = ledger.positions.get_mut(&key).ok_or_else(|| {
        DataQualityError::msg(format!(
            "event {} targets a missing open position",
            event.event_id().unwrap().key()
        ))
    })?;
    if !position.is_open() {
        return Err(DataQualityError::msg(format!(
            "event {} targets a missing open position",
            event.event_id()?.key()
        )));
    }
    Ok(position)
}

fn assert_transition(
    position: &PositionState,
    event: &CanonicalEvent,
    start_lot: i128,
    start_deposit: i128,
) -> Result<()> {
    if start_lot != position.lot_lns {
        return Err(DataQualityError::msg(format!(
            "event {} start lot {start_lot} != ledger lot {}",
            event.event_id()?.key(),
            position.lot_lns
        )));
    }
    if start_deposit != position.deposit_cns {
        return Err(DataQualityError::msg(format!(
            "event {} start deposit {start_deposit} != ledger deposit {}",
            event.event_id()?.key(),
            position.deposit_cns
        )));
    }
    Ok(())
}

fn require_open_invariants(position: &PositionState, event: &CanonicalEvent) -> Result<()> {
    if position.lot_lns <= 0 {
        return Err(DataQualityError::msg(format!(
            "open position {} has non-positive size after {}",
            position.position_id.key(),
            event.event_id()?.key()
        )));
    }
    if position.deposit_cns < 0 {
        return Err(DataQualityError::msg(format!(
            "open position {} has negative deposit after {}",
            position.position_id.key(),
            event.event_id()?.key()
        )));
    }
    if position.entry_pns <= 0 {
        return Err(DataQualityError::msg(format!(
            "open position {} has non-positive entry after {}",
            position.position_id.key(),
            event.event_id()?.key()
        )));
    }
    Ok(())
}

fn side(event: &CanonicalEvent) -> Result<u8> {
    let side = event
        .position_type
        .ok_or_else(|| DataQualityError::msg("position_type is required"))?;
    if side != SIDE_LONG && side != SIDE_SHORT {
        return Err(DataQualityError::msg(format!(
            "event {} has invalid position_type {side}",
            event.event_id()?.key()
        )));
    }
    Ok(side)
}

fn fees(event: &CanonicalEvent) -> Result<i128> {
    accumulate(
        event.ins_fee_cns.unwrap_or(0),
        event.prot_fee_cns.unwrap_or(0),
    )
}

fn accumulate(left: i128, right: i128) -> Result<i128> {
    left.checked_add(right)
        .ok_or_else(|| DataQualityError::msg("native-scale accounting overflow"))
}

fn required(value: Option<i128>, name: &str) -> Result<i128> {
    value.ok_or_else(|| DataQualityError::msg(format!("{name} is required")))
}
