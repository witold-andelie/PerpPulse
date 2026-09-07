use rust_decimal::Decimal;

use crate::error::{DataQualityError, Result};
use crate::events::MarketMark;
use crate::identity::AsOf;
use crate::ledger::{Ledger, PositionState};
use crate::money::{from_native, margin_fraction, margin_rate, notional};
use crate::registry::{MarketSpec, ProtocolRegistry, SIDE_LONG, SIDE_SHORT};

#[derive(Clone, Debug)]
pub struct PositionSnapshot {
    pub account_id: u64,
    pub perpetual_id: u32,
    pub symbol: String,
    pub side: String,
    pub status: String,
    pub size: Decimal,
    pub entry: Decimal,
    pub mark: Option<Decimal>,
    pub deposit: Decimal,
    pub leverage: Option<Decimal>,
    pub unrealized_pnl: Option<Decimal>,
    pub realized_pnl: Decimal,
    pub realized_funding: Decimal,
    pub fees: Decimal,
    pub notional_value: Option<Decimal>,
    pub maintenance_margin: Option<Decimal>,
    pub fair_market_value: Option<Decimal>,
    pub liquidation_price: Option<Decimal>,
    pub liquidation_buffer: Option<Decimal>,
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
    pub unrealized_pnl: Decimal,
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
    for position in ledger.positions.values() {
        if position.position_id.account_id != account_id {
            continue;
        }
        let snap = position_snapshot(position, &ledger.registry, as_of, marks, require_marks)?;
        warnings.extend(snap.warnings.iter().cloned());
        realized_total += snap.realized_pnl;
        fees_total += snap.fees;
        funding_total += snap.realized_funding;
        if let Some(upnl) = snap.unrealized_pnl {
            unrealized_total += upnl;
        } else if position.is_open() {
            warnings.push(format!(
                "unrealized PnL unavailable for {} because mark data is missing",
                snap.symbol
            ));
        }
        snapshots.push(snap);
    }
    snapshots.sort_by_key(|item| item.perpetual_id);
    Ok(WalletSnapshot {
        account_id,
        owner: account.owner.clone(),
        free_balance: from_native(account.balance_cns, decimals, "free_balance")?,
        positions: snapshots,
        realized_pnl: realized_total,
        unrealized_pnl: unrealized_total,
        fees: fees_total,
        realized_funding: funding_total,
        warnings,
    })
}

pub fn position_snapshot(
    position: &PositionState,
    registry: &ProtocolRegistry,
    as_of: &AsOf,
    marks: &[MarketMark],
    require_marks: bool,
) -> Result<PositionSnapshot> {
    let market = registry.market(position.position_id.perpetual_id)?;
    let collateral_decimals = registry.collateral.decimals;
    let size = from_native(position.lot_lns, market.size_decimals, "size")?;
    let entry = from_native(position.entry_pns, market.price_decimals, "entry")?;
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
    let mut fmv = None;
    let mut liq = None;
    let mut buffer = None;

    if position.is_open() {
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
                if mark.block_number > as_of.block_number {
                    return Err(DataQualityError::msg(format!(
                        "mark for perpetual {} uses future block {}",
                        mark.perpetual_id, mark.block_number
                    )));
                }
                let mark_value = from_native(mark.mark_pns, market.price_decimals, "mark")?;
                let pnl = unrealized_pnl(position.side, entry, mark_value, size)?;
                let notional_value = notional(mark_value, size, "notional")?;
                let maintenance = notional_value
                    / margin_fraction(market.maint_margin_frac_hdths, "maint_margin_frac")?;
                let fair = deposit + pnl;
                mark_px = Some(mark_value);
                upnl = Some(pnl);
                notion = Some(notional_value);
                mmr = Some(maintenance);
                fmv = Some(fair);
                buffer = Some(fair - maintenance);
                liq = liquidation_price(position.side, entry, size, deposit, market)?;
                warnings.push(
                    "unrealized funding is not applied to open positions; only funding settled on lifecycle events is included"
                        .to_string(),
                );
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
        mark: mark_px,
        deposit,
        leverage,
        unrealized_pnl: upnl,
        realized_pnl: realized,
        realized_funding: funding,
        fees,
        notional_value: notion,
        maintenance_margin: mmr,
        fair_market_value: fmv,
        liquidation_price: liq,
        liquidation_buffer: buffer,
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
    let pnl = if side == SIDE_LONG {
        (mark - entry) * size
    } else {
        (entry - mark) * size
    };
    Ok(pnl)
}

fn liquidation_price(
    side: u8,
    entry: Decimal,
    size: Decimal,
    deposit: Decimal,
    market: &MarketSpec,
) -> Result<Option<Decimal>> {
    if size <= Decimal::ZERO {
        return Ok(None);
    }
    let mm_rate = margin_rate(market.maint_margin_frac_hdths, "maint_margin_frac")?;
    let price = if side == SIDE_LONG {
        let denominator = size * (Decimal::ONE - mm_rate);
        if denominator.is_zero() {
            return Err(DataQualityError::msg(
                "long liquidation denominator is zero",
            ));
        }
        (entry * size - deposit) / denominator
    } else {
        let denominator = size * (Decimal::ONE + mm_rate);
        if denominator.is_zero() {
            return Err(DataQualityError::msg(
                "short liquidation denominator is zero",
            ));
        }
        (entry * size + deposit) / denominator
    };
    if price <= Decimal::ZERO {
        Ok(None)
    } else {
        Ok(Some(price))
    }
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
