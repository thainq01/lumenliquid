//! Funding-fee and rollover-fee math, and the spread+impact + PnL helpers.
//!
//! All `a * b / denom` chains route through [`crate::mul_div_floor`], which
//! checked-multiplies on i128 and floor-divides. Call-site magnitudes (USDC 1e7,
//! price 1e10, P 1e10, leverage ≤ ~150, OI ≤ ~1e13) all fit in i128 — no i256
//! widening required.
//!
//! Storage is i128 throughout (matches `design.md` Decision 3).

use crate::errors::MathError;
use crate::mul_div_floor;
use crate::scale::P_SCALE;

/// `acc_per_collateral` is a per-pair monotonically increasing accumulator (USDC at `USDC_SCALE`)
/// representing fee-per-unit-collateral since pair inception.
///
/// Mirrors `getPendingAccRolloverFees`:
///   acc' = acc + (Δledger * rolloverFeePerLedgerP * USDC_SCALE) / P_SCALE / 100
///
/// The unit scales with `USDC_SCALE` (Stellar USDC is 1e7). Result is in USDC (1e7) per unit
/// collateral (1e7) — i.e. dimensionless after dividing by collateral (see `rollover_fee_for_trade`).
pub fn pending_acc_rollover(
    acc_per_collateral: i128,
    delta_ledgers: u64,
    rollover_fee_per_ledger_p: i128,
    usdc_scale: i128,
) -> Result<i128, MathError> {
    if delta_ledgers == 0 || rollover_fee_per_ledger_p == 0 {
        return Ok(acc_per_collateral);
    }
    // increment = delta * rate * USDC_SCALE / P_SCALE / 100
    // — precompute (rate * USDC_SCALE) and (P_SCALE * 100) so we don't lose precision when
    //   the rate is small relative to P_SCALE.
    let delta = delta_ledgers as i128;
    let num = rollover_fee_per_ledger_p
        .checked_mul(usdc_scale)
        .ok_or(MathError::Overflow)?;
    let denom = P_SCALE.checked_mul(100).ok_or(MathError::Overflow)?;
    let increment = mul_div_floor(delta, num, denom)?;
    acc_per_collateral.checked_add(increment).ok_or(MathError::Overflow)
}

/// Per-trade rollover fee in USDC.
///
/// Mirrors `getTradeRolloverFeePure`:
///   fee = (acc_now - acc_open) * collateral / USDC_SCALE
pub fn rollover_fee_for_trade(
    acc_open: i128,
    acc_now: i128,
    collateral: i128,
    usdc_scale: i128,
) -> Result<i128, MathError> {
    let delta = acc_now.checked_sub(acc_open).ok_or(MathError::Overflow)?;
    if delta == 0 {
        return Ok(0);
    }
    mul_div_floor(delta, collateral, usdc_scale)
}

/// Asymmetric pending funding accumulators (long, short).
///
/// Mirrors `getPendingAccFundingFees`:
///   paidByLongs = (oiLong - oiShort) * Δledger * fundingPerLedgerP / P_SCALE / 100
///   accLong'  = accLong  + paidByLongs * USDC_SCALE / oiLong   (if oiLong > 0)
///   accShort' = accShort - paidByLongs * USDC_SCALE / oiShort  (if oiShort > 0)
///
/// Returns (acc_long_new, acc_short_new). Both can be negative.
pub fn pending_acc_funding(
    acc_long: i128,
    acc_short: i128,
    oi_long: i128,
    oi_short: i128,
    delta_ledgers: u64,
    funding_fee_per_ledger_p: i128,
    usdc_scale: i128,
) -> Result<(i128, i128), MathError> {
    if delta_ledgers == 0 || funding_fee_per_ledger_p == 0 {
        return Ok((acc_long, acc_short));
    }
    let delta = delta_ledgers as i128;
    let oi_diff = oi_long.checked_sub(oi_short).ok_or(MathError::Overflow)?;
    // paid_by_longs = oi_diff * delta * rate / P_SCALE / 100
    // group denominator before dividing to preserve precision when the rate is small.
    let denom = P_SCALE.checked_mul(100).ok_or(MathError::Overflow)?;
    let stage = oi_diff
        .checked_mul(delta)
        .ok_or(MathError::Overflow)?;
    let paid_by_longs = mul_div_floor(stage, funding_fee_per_ledger_p, denom)?;

    let new_long = if oi_long > 0 {
        // inc = paid_by_longs * USDC_SCALE / oi_long
        let inc = mul_div_floor(paid_by_longs, usdc_scale, oi_long)?;
        acc_long.checked_add(inc).ok_or(MathError::Overflow)?
    } else {
        acc_long
    };

    let new_short = if oi_short > 0 {
        // negate: shorts receive when longs pay
        let inc = mul_div_floor(paid_by_longs, usdc_scale, oi_short)?;
        acc_short.checked_sub(inc).ok_or(MathError::Overflow)?
    } else {
        acc_short
    };

    Ok((new_long, new_short))
}

/// Per-trade funding fee in USDC. Positive = trader pays, negative = trader receives.
///
/// Mirrors `getTradeFundingFeePure`:
///   fee = (acc_now - acc_open) * collateral * leverage / USDC_SCALE
pub fn funding_fee_for_trade(
    acc_open: i128,
    acc_now: i128,
    collateral: i128,
    leverage: u32,
    usdc_scale: i128,
) -> Result<i128, MathError> {
    let delta = acc_now.checked_sub(acc_open).ok_or(MathError::Overflow)?;
    if delta == 0 {
        return Ok(0);
    }
    let staged = mul_div_floor(delta, collateral, usdc_scale)?;
    staged.checked_mul(leverage as i128).ok_or(MathError::Overflow)
}

/// Floor `funding_fee` at `min_funding_fee` (mirrors the `minFundingFee` clamp). Both at USDC scale.
pub fn clamp_funding_fee(funding_fee: i128, min_funding_fee: i128) -> i128 {
    if funding_fee < min_funding_fee { min_funding_fee } else { funding_fee }
}

/// Dynamic price impact `(impact_p, price_after_impact)`.
///
/// Mirrors `getTradePriceImpactPure`:
///   impact_p     = (start_oi + trade_oi/2) * P_SCALE / USDC_SCALE / one_percent_depth
///   price_impact = impact_p * open_price / P_SCALE / 100
///   price_after  = open_price ± price_impact   (long: +, short: -)
///
/// `start_oi` and `trade_oi` are USDC at `USDC_SCALE`. `one_percent_depth` is also USDC at `USDC_SCALE`.
pub fn price_impact(
    open_price: i128,
    is_long: bool,
    start_oi: i128,
    trade_oi: i128,
    one_percent_depth: i128,
    usdc_scale: i128,
) -> Result<(i128, i128), MathError> {
    if one_percent_depth == 0 {
        return Ok((0, open_price));
    }
    // half = trade_oi / 2 (floor)
    let half = trade_oi / 2;
    let oi_term = start_oi.checked_add(half).ok_or(MathError::Overflow)?;
    // impact_p = oi_term * P_SCALE / USDC_SCALE / depth
    //         = (oi_term * P_SCALE) / (USDC_SCALE * depth)
    let denom = usdc_scale.checked_mul(one_percent_depth).ok_or(MathError::Overflow)?;
    let impact_p = mul_div_floor(oi_term, P_SCALE, denom)?;

    // price_impact = impact_p * open_price / P_SCALE / 100
    let stage = mul_div_floor(impact_p, open_price, P_SCALE)?;
    let price_impact = stage / 100;

    let price_after = if is_long {
        open_price.checked_add(price_impact).ok_or(MathError::Overflow)?
    } else {
        open_price.checked_sub(price_impact).ok_or(MathError::Overflow)?
    };
    Ok((impact_p, price_after))
}

/// Apply spread on top of price-after-impact (the `marketExecutionPrice` step).
///
/// `spread_p` is expressed at `P_SCALE` (e.g. `5e7` for 0.05%).
///   spread_amount = price * spread_p / P_SCALE / 100
///   long  → price + spread
///   short → price - spread
pub fn apply_spread(
    price: i128,
    spread_p: i128,
    is_long: bool,
) -> Result<i128, MathError> {
    if spread_p == 0 {
        return Ok(price);
    }
    let stage = mul_div_floor(price, spread_p, P_SCALE)?;
    let spread_amount = stage / 100;
    if is_long {
        price.checked_add(spread_amount).ok_or(MathError::Overflow)
    } else {
        price.checked_sub(spread_amount).ok_or(MathError::Overflow)
    }
}

/// Combined market execution price = spread+impact applied in the order
/// (impact first, then spread). Returns `(impact_p, market_price)`.
pub fn apply_spread_and_impact(
    open_price: i128,
    spread_p: i128,
    is_long: bool,
    start_oi: i128,
    trade_oi: i128,
    one_percent_depth: i128,
    usdc_scale: i128,
) -> Result<(i128, i128), MathError> {
    let (impact_p, price_after_impact) = price_impact(
        open_price, is_long, start_oi, trade_oi, one_percent_depth, usdc_scale,
    )?;
    let market_price = apply_spread(price_after_impact, spread_p, is_long)?;
    Ok((impact_p, market_price))
}

/// Signed PnL percentage at `P_SCALE`. Positive = profit, negative = loss.
///
/// Mirrors `currentPercentProfit`:
///   raw_p = (close - open) / open * leverage * 100   (long)
///   raw_p = (open - close) / open * leverage * 100   (short)
/// then capped at `±max_gain_p` (an integer percent like 900).
pub fn current_percent_profit(
    open_price: i128,
    close_price: i128,
    is_long: bool,
    leverage: u32,
    max_gain_p: u32,
) -> Result<i128, MathError> {
    if open_price <= 0 {
        return Err(MathError::DivByZero);
    }
    let diff = if is_long {
        close_price.checked_sub(open_price).ok_or(MathError::Overflow)?
    } else {
        open_price.checked_sub(close_price).ok_or(MathError::Overflow)?
    };
    // p = diff * P_SCALE / open * leverage * 100  -- but compose to avoid blowup:
    // step = diff * P_SCALE / open
    let step = mul_div_floor(diff, P_SCALE, open_price)?;
    // raw = step * leverage * 100
    let raw = step
        .checked_mul(leverage as i128)
        .ok_or(MathError::Overflow)?
        .checked_mul(100)
        .ok_or(MathError::Overflow)?;

    // cap at ±max_gain_p * P_SCALE   (max_gain_p is integer-percent like 900)
    let cap_pos = (max_gain_p as i128).checked_mul(P_SCALE).ok_or(MathError::Overflow)?;
    let cap_neg = -cap_pos;
    Ok(raw.clamp(cap_neg, cap_pos))
}
