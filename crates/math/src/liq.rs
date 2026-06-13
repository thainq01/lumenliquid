//! Liquidation price + payout-with-insolvency-waiver math.
//!
//! Implements `getTradeLiquidationPricePure` and `getTradeValuePure`. Time
//! advancement happens at the call site via `ledger.sequence`; the formulas
//! here are time-agnostic — they consume already-computed rollover/funding fees.

use crate::errors::MathError;
use crate::mul_div_floor;
use crate::scale::P_SCALE;

/// Liquidation price.
///
/// Mirrors `getTradeLiquidationPricePure`:
///   distance = open * (collateral * liq_threshold_p / 100 - rollover - funding) / collateral / leverage
///   long:  liq = open - distance
///   short: liq = open + distance
///
/// `liq_threshold_p` is integer percent (e.g. 90), NOT P_SCALE.
/// All USDC inputs at `USDC_SCALE`. Price at `PRICE_SCALE`. Returns price at `PRICE_SCALE`,
/// floored at zero.
pub fn liquidation_price(
    open_price: i128,
    is_long: bool,
    collateral: i128,
    leverage: u32,
    rollover_fee: i128,
    funding_fee: i128,
    liq_threshold_p: u32,
) -> Result<i128, MathError> {
    if collateral == 0 {
        return Err(MathError::DivByZero);
    }
    if leverage == 0 {
        return Err(MathError::DivByZero);
    }

    // collateral_threshold = collateral * liq_threshold_p / 100
    let collateral_threshold = collateral
        .checked_mul(liq_threshold_p as i128)
        .ok_or(MathError::Overflow)?
        / 100;

    // numer_inner = collateral_threshold - rollover_fee - funding_fee   (signed)
    let numer_inner = collateral_threshold
        .checked_sub(rollover_fee)
        .ok_or(MathError::Overflow)?
        .checked_sub(funding_fee)
        .ok_or(MathError::Overflow)?;

    // distance = open_price * numer_inner / collateral / leverage
    // Compose via i128 fixed-point: keep open_price as the multiplicand, divide by collateral first.
    let stage = mul_div_floor(open_price, numer_inner, collateral)?;
    let distance = stage / leverage as i128;

    let liq = if is_long {
        open_price.checked_sub(distance).ok_or(MathError::Overflow)?
    } else {
        open_price.checked_add(distance).ok_or(MathError::Overflow)?
    };
    Ok(if liq > 0 { liq } else { 0 })
}

/// Whether `observed_price` has crossed the liquidation threshold for this side.
pub fn is_liquidatable(
    observed_price: i128,
    liq_price: i128,
    is_long: bool,
) -> bool {
    if liq_price == 0 {
        return false;
    }
    if is_long { observed_price <= liq_price } else { observed_price >= liq_price }
}

/// Payout decision with insolvency waiver, mirroring `getTradeValuePure`.
///
/// Inputs:
///   - `collateral` is `effective_collateral` (already net of open fee)
///   - `pnl_p` is signed percent at `P_SCALE * 100` (PRECISION × percentage)
///   - `rollover_fee` and `funding_fee` are USDC at `USDC_SCALE`
///   - `close_fee` is USDC at `USDC_SCALE`
///   - `liq_threshold_p` is integer percent (e.g. 90)
///
/// Output: `(gross_payout, close_fee_charged)`.
///   - On insolvency (value_before_close_fee ≤ collateral * (100 - liq_threshold_p) / 100):
///     `gross_payout = 0`, `close_fee_charged = 0` (close fee waived; vault keeps everything).
///   - Otherwise: `gross_payout = max(0, value_before_close_fee - close_fee)`,
///     `close_fee_charged = close_fee`.
pub fn settlement_payout(
    collateral: i128,
    pnl_p_at_p_scale_times_100: i128,
    rollover_fee: i128,
    funding_fee: i128,
    close_fee: i128,
    liq_threshold_p: u32,
) -> Result<(i128, i128), MathError> {
    if collateral <= 0 {
        return Err(MathError::DivByZero);
    }
    // pnl_amount = collateral * pnl_p / P_SCALE / 100
    let stage = mul_div_floor(collateral, pnl_p_at_p_scale_times_100, P_SCALE)?;
    let pnl_amount = stage / 100;

    // value_before_close_fee = collateral + pnl_amount - rollover - funding
    let value_before_close_fee = collateral
        .checked_add(pnl_amount).ok_or(MathError::Overflow)?
        .checked_sub(rollover_fee).ok_or(MathError::Overflow)?
        .checked_sub(funding_fee).ok_or(MathError::Overflow)?;

    // insolvency cutoff = collateral * (100 - liq_threshold_p) / 100
    let liq_remainder_p = (100u32).saturating_sub(liq_threshold_p) as i128;
    let cutoff = collateral
        .checked_mul(liq_remainder_p).ok_or(MathError::Overflow)?
        / 100;

    if value_before_close_fee <= cutoff {
        // insolvent — vault keeps everything, close fee waived
        return Ok((0, 0));
    }

    let net = value_before_close_fee.checked_sub(close_fee).ok_or(MathError::Overflow)?;
    let gross_payout = if net > 0 { net } else { 0 };
    Ok((gross_payout, close_fee))
}
