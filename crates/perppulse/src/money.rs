use rust_decimal::Decimal;

use crate::error::{DataQualityError, Result};

pub const FEE_SCALE_DECIMALS: u32 = 5;

pub fn from_native(amount: i128, decimals: u32, name: &str) -> Result<Decimal> {
    if decimals > 18 {
        return Err(DataQualityError::msg(format!("{name} decimals {decimals} exceed 18")));
    }
    Decimal::try_from_i128_with_scale(amount, decimals)
        .map_err(|_| DataQualityError::msg(format!("{name} {amount} is not a finite decimal")))
}

pub fn require_finite(value: Decimal, _name: &str) -> Result<Decimal> {
    Ok(value)
}

pub fn margin_fraction(hdths: u32, name: &str) -> Result<Decimal> {
    if hdths == 0 {
        return Err(DataQualityError::msg(format!("{name} must be positive hundredths")));
    }
    Ok(Decimal::from(hdths) / Decimal::from(100u32))
}

pub fn margin_rate(hdths: u32, name: &str) -> Result<Decimal> {
    let fraction = margin_fraction(hdths, name)?;
    Ok(Decimal::ONE / fraction)
}

pub fn notional(price: Decimal, size: Decimal, name: &str) -> Result<Decimal> {
    require_finite(price * size, name)
}
