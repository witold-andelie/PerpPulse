use rust_decimal::Decimal;

use crate::error::{DataQualityError, Result};
use crate::events::MarketMark;
use crate::identity::AsOf;
use crate::ledger::{Ledger, PositionState};
use crate::money::{from_native, margin_fraction, notional};
use crate::registry::{MarketSpec, ProtocolRegistry, SIDE_LONG, SIDE_SHORT};

/// Conservative application freshness limit; live protocol limits still need
/// exact-cutoff verification. Latest REST observations are never accepted here.
pub const MAX_MARK_AGE_MS: i64 = 60_000;

fn add(a: Decimal, b: Decimal, name: &str) -> Result<Decimal> {
    a.checked_add(b)
        .ok_or_else(|| DataQualityError::msg(format!("{name} overflow")))
}

fn sub(a: Decimal, b: Decimal, name: &str) -> Result<Decimal> {
    a.checked_sub(b)
        .ok_or_else(|| DataQualityError::msg(format!("{name} overflow")))
}

fn div(a: Decimal, b: Decimal, name: &str) -> Result<Decimal> {
    a.checked_div(b)
        .ok_or_else(|| DataQualityError::msg(format!("{name} overflow or zero denominator")))
}

fn validate_mark(mark: &MarketMark, as_of: &AsOf, market: &MarketSpec) -> Result<()> {
    if mark.timestamp_ms < 0
        || mark.timestamp_ms > as_of.timestamp_ms
        || mark.block_number > as_of.block_number
    {
        return Err(DataQualityError::msg(
            "mark uses a negative or future timestamp or future block",
        ));
    }
    if mark.block_number == as_of.block_number {
        if let Some(cutoff) = as_of.log_index {
            if mark.log_index.is_none_or(|log| log > cutoff)
                || mark.block_hash.as_deref() != Some(&as_of.block_hash)
            {
                return Err(DataQualityError::msg(
                    "same-block mark requires the matching block hash and an eligible log cutoff",
                ));
            }
        } else if mark
            .block_hash
            .as_ref()
            .is_some_and(|hash| hash != &as_of.block_hash)
        {
            return Err(DataQualityError::msg(
                "mark block hash differs from the as-of block",
            ));
        }
    }
    if as_of.timestamp_ms - mark.timestamp_ms >= MAX_MARK_AGE_MS {
        return Err(DataQualityError::msg(
            "as-of mark is stale (60-second application limit)",
        ));
    }
    if mark.mark_pns <= 0 || mark.oracle_pns.is_some_and(|p| p <= 0) {
        return Err(DataQualityError::msg(
            "mark and supplied oracle prices must be positive",
        ));
    }
    from_native(mark.mark_pns, market.price_decimals, "mark")?;
    if let Some(oracle) = mark.oracle_pns {
        from_native(oracle, market.price_decimals, "oracle")?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct PositionSnapshot {
    pub account_id: u64,
    pub perpetual_id: u32,
    pub symbol: String,
    pub side: String,
    pub status: String,
    pub size: Decimal,
    pub entry: Decimal,
    pub stored_entry_pns: i128,
    pub entry_residue_pnsq16: u32,
    pub mark: Option<Decimal>,
    pub mark_event_id: Option<String>,
    pub deposit: Decimal,
    pub leverage: Option<Decimal>,
    pub unrealized_pnl: Option<Decimal>,
    pub unrealized_price_pnl: Option<Decimal>,
    pub unrealized_funding: Option<Decimal>,
    pub realized_pnl: Decimal,
    pub realized_funding: Decimal,
    pub fees: Decimal,
    pub notional_value: Option<Decimal>,
    pub maintenance_margin: Option<Decimal>,
    pub fair_market_value: Option<Decimal>,
    pub liquidation_price: Option<Decimal>,
    pub liquidation_buffer: Option<Decimal>,
    pub zero_funding_equity: Option<Decimal>,
    pub zero_funding_liquidation_price: Option<Decimal>,
    pub zero_funding_liquidation_buffer: Option<Decimal>,
    pub last_event_id: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct WalletSnapshot {
    pub account_id: u64,
    pub owner: Option<String>,
    pub free_balance: Decimal,
    pub positions: Vec<PositionSnapshot>,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Option<Decimal>,
    pub unrealized_price_pnl: Option<Decimal>,
    pub unrealized_funding: Option<Decimal>,
    pub fees: Decimal,
    pub realized_funding: Decimal,
    pub warnings: Vec<String>,
}

pub fn account_wallet(
    ledger: &Ledger,
    account_id: u64,
    as_of: &AsOf,
    marks: &[MarketMark],
    require_marks: bool,
) -> Result<WalletSnapshot> {
    let account = ledger.accounts.get(&account_id).ok_or_else(|| {
        DataQualityError::msg(format!(
            "account {account_id} is not present in the canonical ledger"
        ))
    })?;
    let decimals = ledger.registry.collateral.decimals;
    let mut snapshots = Vec::new();
    let mut warnings = Vec::new();
    let mut unrealized_total = Decimal::ZERO;
    let mut realized_total = Decimal::ZERO;
    let mut fees_total = Decimal::ZERO;
    let mut funding_total = Decimal::ZERO;
    let mut missing_price_pnl = false;
    let mut open_position = false;
    for position in ledger.positions.values() {
        if position.position_id.account_id != account_id {
            continue;
        }
        let mut snap = position_snapshot(position, &ledger.registry, as_of, marks, require_marks)?;
        if snap.mark.is_some()
            && ledger
                .market_marks
                .get(&snap.perpetual_id)
                .is_some_and(|canonical| marks.iter().any(|mark| mark == canonical))
        {
            snap.mark_event_id = ledger.mark_event_ids.get(&snap.perpetual_id).cloned();
        }
        warnings.extend(snap.warnings.iter().cloned());
        realized_total = add(realized_total, snap.realized_pnl, "wallet realized PnL")?;
        fees_total = add(fees_total, snap.fees, "wallet fees")?;
        funding_total = add(funding_total, snap.realized_funding, "wallet funding")?;
        if let Some(upnl) = snap.unrealized_price_pnl {
            unrealized_total = add(unrealized_total, upnl, "wallet unrealized PnL")?;
        } else if position.is_open() {
            missing_price_pnl = true;
            warnings.push(format!(
                "unrealized PnL unavailable for {} because mark data is missing",
                snap.symbol
            ));
        }
        open_position |= position.is_open();
        snapshots.push(snap);
    }
    snapshots.sort_by_key(|item| item.perpetual_id);
    Ok(WalletSnapshot {
        account_id,
        owner: account.owner.clone(),
        free_balance: from_native(account.balance_cns, decimals, "free_balance")?,
        positions: snapshots,
        realized_pnl: realized_total,
        unrealized_pnl: (!open_position).then_some(Decimal::ZERO),
        unrealized_price_pnl: (!missing_price_pnl).then_some(unrealized_total),
        unrealized_funding: (!open_position).then_some(Decimal::ZERO),
        fees: fees_total,
        realized_funding: funding_total,
        warnings,
    })
}

fn effective_entry(position: &PositionState, decimals: u32) -> Result<Decimal> {
    let residue = position.entry_residue_pnsq16;
    if residue == 0 {
        return from_native(position.entry_pns, decimals, "entry");
    }
    if residue >= 65_536 || decimals > 18 {
        return Err(DataQualityError::msg("invalid effective entry precision"));
    }
    let base = if position.side == SIDE_LONG {
        position.entry_pns.checked_sub(1)
    } else if position.side == SIDE_SHORT {
        Some(position.entry_pns)
    } else {
        None
    }
    .ok_or_else(|| DataQualityError::msg("invalid effective entry side or price"))?;
    // 1 / 65536 = 5^16 / 10^16. Integer construction keeps every Q16 bit;
    // reject values that Decimal cannot represent exactly instead of rounding.
    let mut coefficient = base
        .checked_mul(65_536)
        .and_then(|n| n.checked_add(i128::from(residue)))
        .and_then(|n| n.checked_mul(152_587_890_625))
        .ok_or_else(|| DataQualityError::msg("effective entry coefficient overflow"))?;
    let mut scale = decimals + 16;
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    if scale > 28 {
        return Err(DataQualityError::msg(
            "effective entry exceeds exact decimal precision",
        ));
    }
    Decimal::try_from_i128_with_scale(coefficient, scale)
        .map_err(|_| DataQualityError::msg("effective entry exceeds exact decimal range"))
}

pub fn position_snapshot(
    position: &PositionState,
    registry: &ProtocolRegistry,
    as_of: &AsOf,
    marks: &[MarketMark],
    require_marks: bool,
) -> Result<PositionSnapshot> {
    let market = registry.market(position.position_id.perpetual_id)?;
    if marks
        .iter()
        .filter(|m| m.perpetual_id == market.perpetual_id)
        .count()
        > 1
    {
        return Err(DataQualityError::msg(
            "duplicate as-of marks for one perpetual",
        ));
    }
    let collateral_decimals = registry.collateral.decimals;
    let size = from_native(position.lot_lns, market.size_decimals, "size")?;
    let entry = effective_entry(position, market.price_decimals)?;
    let deposit = from_native(position.deposit_cns, collateral_decimals, "deposit")?;
    let realized = from_native(
        position.realized_pnl_cns,
        collateral_decimals,
        "realized_pnl",
    )?;
    let funding = from_native(
        position.realized_funding_cns,
        collateral_decimals,
        "realized_funding",
    )?;
    let fees = from_native(position.fees_cns, collateral_decimals, "fees")?;
    let leverage = if position.leverage_hdths == 0 {
        None
    } else {
        Some(from_native(
            i128::from(position.leverage_hdths),
            2,
            "leverage",
        )?)
    };

    let mut warnings = Vec::new();
    let mut mark_px = None;
    let mut upnl = None;
    let mut notion = None;
    let mut mmr = None;
    let mut zero_equity = None;
    let mut zero_liq = None;
    let mut zero_buffer = None;

    if position.is_open() {
        warnings.push("Unsettled funding is unverified; total unrealized PnL, equity and actual liquidation risk are unavailable. Zero-funding fields are conditional scenarios.".into());
        let maintenance_inverse =
            margin_fraction(market.maint_margin_frac_hdths, "maint_margin_frac")?;
        mmr = Some(div(
            notional(entry, size, "entry notional")?,
            maintenance_inverse,
            "maintenance margin",
        )?);
        zero_liq = Some(isolated_liquidation_price(
            position.side,
            entry,
            size,
            deposit,
            maintenance_inverse,
            Decimal::ZERO,
        )?);
        match marks
            .iter()
            .find(|mark| mark.perpetual_id == position.position_id.perpetual_id)
        {
            None => {
                let message = format!(
                    "missing as-of mark for perpetual {} at block {}",
                    position.position_id.perpetual_id, as_of.block_number
                );
                if require_marks {
                    return Err(DataQualityError::msg(message));
                }
                warnings.push(message);
            }
            Some(mark) => {
                validate_mark(mark, as_of, market)?;
                let mark_value = from_native(mark.mark_pns, market.price_decimals, "mark")?;
                let pnl = unrealized_pnl(position.side, entry, mark_value, size)?;
                let notional_value = notional(mark_value, size, "notional")?;
                let maintenance = mmr.expect("open position maintenance calculated above");
                let fair = add(deposit, pnl, "fair market value")?;
                mark_px = Some(mark_value);
                upnl = Some(pnl);
                notion = Some(notional_value);
                zero_equity = Some(fair);
                zero_buffer = Some(sub(fair, maintenance, "zero-funding liquidation buffer")?);
            }
        }
    } else {
        upnl = Some(Decimal::ZERO);
    }

    Ok(PositionSnapshot {
        account_id: position.position_id.account_id,
        perpetual_id: position.position_id.perpetual_id,
        symbol: market.symbol.clone(),
        side: side_name(position.side)?,
        status: position.status.clone(),
        size,
        entry,
        stored_entry_pns: position.entry_pns,
        entry_residue_pnsq16: position.entry_residue_pnsq16,
        mark: mark_px,
        mark_event_id: None,
        deposit,
        leverage,
        unrealized_pnl: (!position.is_open()).then_some(Decimal::ZERO),
        unrealized_price_pnl: upnl,
        unrealized_funding: (!position.is_open()).then_some(Decimal::ZERO),
        realized_pnl: realized,
        realized_funding: funding,
        fees,
        notional_value: notion,
        maintenance_margin: mmr,
        fair_market_value: None,
        liquidation_price: None,
        liquidation_buffer: None,
        zero_funding_equity: zero_equity,
        zero_funding_liquidation_price: zero_liq,
        zero_funding_liquidation_buffer: zero_buffer,
        last_event_id: position
            .last_event_id
            .as_ref()
            .map(EventIdExt::key)
            .unwrap_or_default(),
        warnings,
    })
}

trait EventIdExt {
    fn key(&self) -> String;
}

impl EventIdExt for crate::identity::EventId {
    fn key(&self) -> String {
        crate::identity::EventId::key(self)
    }
}

fn unrealized_pnl(side: u8, entry: Decimal, mark: Decimal, size: Decimal) -> Result<Decimal> {
    let difference = if side == SIDE_LONG {
        sub(mark, entry, "mark minus entry")?
    } else {
        sub(entry, mark, "entry minus mark")?
    };
    notional(difference, size, "unrealized PnL")
}

/// Isolated liquidation scenario with an explicitly supplied, signed funding PnL.
/// The caller must prove funding independently before presenting an actual risk fact.
pub fn isolated_liquidation_price(
    side: u8,
    entry: Decimal,
    size: Decimal,
    deposit: Decimal,
    maintenance_inverse: Decimal,
    premium_pnl: Decimal,
) -> Result<Decimal> {
    if !matches!(side, SIDE_LONG | SIDE_SHORT)
        || entry <= Decimal::ZERO
        || size <= Decimal::ZERO
        || deposit < Decimal::ZERO
        || maintenance_inverse <= Decimal::ONE
    {
        return Err(DataQualityError::msg("invalid isolated liquidation inputs"));
    }
    let maintenance = div(
        notional(entry, size, "entry notional")?,
        maintenance_inverse,
        "maintenance margin",
    )?;
    let gap = div(
        sub(
            sub(maintenance, deposit, "margin less deposit")?,
            premium_pnl,
            "margin less funding",
        )?,
        size,
        "liquidation distance",
    )?;
    let price = if side == SIDE_LONG {
        add(entry, gap, "long liquidation price")?
    } else {
        sub(entry, gap, "short liquidation price")?
    };
    Ok(price.max(Decimal::ZERO))
}

fn side_name(side: u8) -> Result<String> {
    match side {
        SIDE_LONG => Ok("long".to_string()),
        SIDE_SHORT => Ok("short".to_string()),
        other => Err(DataQualityError::msg(format!(
            "invalid position side {other}"
        ))),
    }
}
