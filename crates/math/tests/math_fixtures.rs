//! Math correctness tests against fixed reference fixtures.
//!
//! Scales used by the test fixtures:
//!   - PRECISION (P_SCALE) = 1e10
//!   - USDC = 1e7 (Soroban / Stellar SAC scale)
//! Price stays at 1e10 across all assertions. Each closed-form value is
//! computed from the formulas in `crates/math/src/{fees,liq}.rs` evaluated at
//! these scales.

use math::fees::{
    apply_spread_and_impact, clamp_funding_fee, current_percent_profit, funding_fee_for_trade,
    pending_acc_funding, pending_acc_rollover, price_impact, rollover_fee_for_trade,
};
use math::liq::{is_liquidatable, liquidation_price, settlement_payout};
use math::scale::{LIQ_THRESHOLD_P, MAX_GAIN_P, P_SCALE, PRICE_SCALE, USDC_SCALE};

const ONE_USDC: i128 = USDC_SCALE; // 1 USDC at Stellar scale

// ────────────────────────────────────────────────────────────────────────────
// rollover accumulator
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn rollover_accumulator_zero_delta_is_noop() {
    let acc = 12_345_678_i128;
    let out = pending_acc_rollover(acc, 0, 1_000_000, USDC_SCALE).unwrap();
    assert_eq!(out, acc);
}

#[test]
fn rollover_accumulator_zero_rate_is_noop() {
    let acc = 12_345_678_i128;
    let out = pending_acc_rollover(acc, 1000, 0, USDC_SCALE).unwrap();
    assert_eq!(out, acc);
}

#[test]
fn rollover_accumulator_linear_increment() {
    // formula:   acc' = acc + Δledgers * rate * USDC_SCALE / PRECISION / 100
    // pick Δ=100 ledgers, rate=1e6 (1e6/1e10 = 0.0001% per ledger ≈ 0.36%/hr at 5s/ledger)
    let delta = 100u64;
    let rate = 1_000_000_i128;
    let out = pending_acc_rollover(0, delta, rate, USDC_SCALE).unwrap();
    // expected = 100 * 1e6 * 1e7 / 1e10 / 100 = 1000
    let expected = (delta as i128 * rate * USDC_SCALE) / P_SCALE / 100;
    assert_eq!(out, expected);
    assert_eq!(out, 1000);
}

#[test]
fn rollover_accumulator_does_not_lose_small_rates() {
    // regression test: an earlier formulation did `delta*rate/P_SCALE` first, which
    // truncated to zero whenever `delta*rate < P_SCALE`. The implementation must
    // multiply by USDC_SCALE before dividing.
    let delta = 1u64;
    let rate = 1_000_000_i128; // 0.0001% per ledger
    let out = pending_acc_rollover(0, delta, rate, USDC_SCALE).unwrap();
    assert!(out > 0, "small-rate single-ledger advance must not truncate to zero");
}

#[test]
fn rollover_fee_for_trade_pure() {
    // (acc_now - acc_open) * collateral / USDC_SCALE
    let acc_open = 0;
    let acc_now = 100; // matches above
    let collateral = 1000 * ONE_USDC; // 1000 USDC
    let fee = rollover_fee_for_trade(acc_open, acc_now, collateral, USDC_SCALE).unwrap();
    let expected = ((acc_now - acc_open) * collateral) / USDC_SCALE;
    assert_eq!(fee, expected);
}

// ────────────────────────────────────────────────────────────────────────────
// funding accumulator
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn funding_accumulator_balanced_oi_no_funding_pressure() {
    // oi_long == oi_short → no funding pressure → both accumulators unchanged
    let oi = 1_000_000 * ONE_USDC;
    let (long, short) = pending_acc_funding(0, 0, oi, oi, 100, 1_000_000, USDC_SCALE).unwrap();
    assert_eq!(long, 0);
    assert_eq!(short, 0);
}

#[test]
fn funding_accumulator_long_heavy_pays_short() {
    // longs pay → acc_long increases, acc_short decreases
    let oi_long = 1_500_000 * ONE_USDC;
    let oi_short = 500_000 * ONE_USDC;
    let delta = 100u64;
    let rate = 1_000_000_i128;

    let (long, short) = pending_acc_funding(0, 0, oi_long, oi_short, delta, rate, USDC_SCALE).unwrap();
    assert!(long > 0, "long acc should have grown");
    assert!(short < 0, "short acc should have decreased");
    // closed form: paid = (oi_long - oi_short) * Δ * rate / P_SCALE / 100
    let paid = (oi_long - oi_short) * delta as i128 * rate / P_SCALE / 100;
    let inc_long = (paid * USDC_SCALE) / oi_long;
    let inc_short = (paid * USDC_SCALE) / oi_short;
    assert_eq!(long, inc_long);
    assert_eq!(short, -inc_short);
}

#[test]
fn funding_fee_for_trade_pure() {
    // (acc_now - acc_open) * collateral * leverage / USDC_SCALE
    let acc_open = 0;
    let acc_now = 5_000_000_i128;
    let collateral = 1000 * ONE_USDC;
    let lev = 50;
    let fee = funding_fee_for_trade(acc_open, acc_now, collateral, lev, USDC_SCALE).unwrap();
    let expected = ((acc_now - acc_open) * collateral / USDC_SCALE) * lev as i128;
    assert_eq!(fee, expected);
}

#[test]
fn clamp_funding_fee_floors_at_min() {
    let min = -99 * ONE_USDC;
    assert_eq!(clamp_funding_fee(-1000 * ONE_USDC, min), min);
    assert_eq!(clamp_funding_fee(-50 * ONE_USDC, min), -50 * ONE_USDC);
    assert_eq!(clamp_funding_fee(0, min), 0);
    assert_eq!(clamp_funding_fee(100 * ONE_USDC, min), 100 * ONE_USDC);
}

// ────────────────────────────────────────────────────────────────────────────
// price impact + spread
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn price_impact_zero_depth_is_identity() {
    let p = 50_000 * PRICE_SCALE; // BTC at 50k
    let (impact_p, after) = price_impact(p, true, 0, 100 * ONE_USDC, 0, USDC_SCALE).unwrap();
    assert_eq!(impact_p, 0);
    assert_eq!(after, p);
}

#[test]
fn price_impact_long_pushes_price_up_short_pushes_down() {
    let p = 50_000 * PRICE_SCALE;
    let depth = 1_000_000 * ONE_USDC;
    let trade_oi = 100_000 * ONE_USDC;

    let (_, after_long) = price_impact(p, true, 500_000 * ONE_USDC, trade_oi, depth, USDC_SCALE).unwrap();
    let (_, after_short) = price_impact(p, false, 500_000 * ONE_USDC, trade_oi, depth, USDC_SCALE).unwrap();
    assert!(after_long > p, "long pushes price up");
    assert!(after_short < p, "short pushes price down");
}

#[test]
fn apply_spread_and_impact_long_round_trip() {
    let p = 50_000 * PRICE_SCALE;
    // 0.05% spread
    let spread = 5_000_000_i128;
    let depth = 1_000_000 * ONE_USDC;
    let (_, after_long) =
        apply_spread_and_impact(p, spread, true, 0, 100 * ONE_USDC, depth, USDC_SCALE).unwrap();
    let (_, after_short) =
        apply_spread_and_impact(p, spread, false, 0, 100 * ONE_USDC, depth, USDC_SCALE).unwrap();
    // long pays >price, short pays <price
    assert!(after_long > p);
    assert!(after_short < p);
}

// ────────────────────────────────────────────────────────────────────────────
// PnL %
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn pnl_long_in_profit() {
    // +10% price * 10x leverage = +100% pnl
    let open = 100 * PRICE_SCALE;
    let close = 110 * PRICE_SCALE;
    let pnl = current_percent_profit(open, close, true, 10, MAX_GAIN_P).unwrap();
    // expected = (10/100) * 10 * 100% at P_SCALE = 100% = P_SCALE * 100
    let expected = 100 * P_SCALE;
    assert_eq!(pnl, expected);
}

#[test]
fn pnl_short_in_loss_when_price_rises() {
    // short, +5% price, 4x leverage → -20% pnl
    let open = 100 * PRICE_SCALE;
    let close = 105 * PRICE_SCALE;
    let pnl = current_percent_profit(open, close, false, 4, MAX_GAIN_P).unwrap();
    let expected = -20 * P_SCALE;
    assert_eq!(pnl, expected);
}

#[test]
fn pnl_capped_by_max_gain_p() {
    // +100% price * 50x = +5000%, but max_gain = 900% → cap
    let open = 100 * PRICE_SCALE;
    let close = 200 * PRICE_SCALE;
    let pnl = current_percent_profit(open, close, true, 50, MAX_GAIN_P).unwrap();
    let cap = (MAX_GAIN_P as i128) * P_SCALE;
    assert_eq!(pnl, cap);
}

#[test]
fn pnl_capped_by_negative_max() {
    // long with price collapse and high leverage → -∞ in raw, capped at -900%
    let open = 100 * PRICE_SCALE;
    let close = 1 * PRICE_SCALE;
    let pnl = current_percent_profit(open, close, true, 50, MAX_GAIN_P).unwrap();
    let cap_neg = -((MAX_GAIN_P as i128) * P_SCALE);
    assert_eq!(pnl, cap_neg);
}

// ────────────────────────────────────────────────────────────────────────────
// liquidation price
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn liq_price_long_no_fees_matches_closed_form() {
    // long, open=100, collateral=1000, leverage=10, no fees, threshold=90
    // distance = 100 * (1000*90/100 - 0 - 0) / 1000 / 10 = 100 * 900 / 1000 / 10 = 9
    // liq = 100 - 9 = 91
    let open = 100 * PRICE_SCALE;
    let collateral = 1000 * ONE_USDC;
    let leverage = 10;
    let liq = liquidation_price(open, true, collateral, leverage, 0, 0, LIQ_THRESHOLD_P).unwrap();
    let expected = 91 * PRICE_SCALE;
    assert_eq!(liq, expected);
}

#[test]
fn liq_price_short_no_fees() {
    // short, open=100, collateral=1000, leverage=10, no fees → liq = 109
    let open = 100 * PRICE_SCALE;
    let collateral = 1000 * ONE_USDC;
    let leverage = 10;
    let liq = liquidation_price(open, false, collateral, leverage, 0, 0, LIQ_THRESHOLD_P).unwrap();
    let expected = 109 * PRICE_SCALE;
    assert_eq!(liq, expected);
}

#[test]
fn liq_price_with_fees_tightens_long() {
    // fees eat into collateral threshold → liq price gets closer to open price (higher for longs)
    let open = 100 * PRICE_SCALE;
    let collateral = 1000 * ONE_USDC;
    let leverage = 10;
    let no_fee =
        liquidation_price(open, true, collateral, leverage, 0, 0, LIQ_THRESHOLD_P).unwrap();
    let with_fee = liquidation_price(
        open,
        true,
        collateral,
        leverage,
        50 * ONE_USDC, // rollover
        25 * ONE_USDC, // funding
        LIQ_THRESHOLD_P,
    )
    .unwrap();
    assert!(with_fee > no_fee, "fees should raise the long liq price");
}

#[test]
fn liq_price_floored_at_zero() {
    // wildly hostile fees push distance > open_price → result clamps to 0
    let open = 100 * PRICE_SCALE;
    let collateral = 1000 * ONE_USDC;
    let leverage = 1;
    // distance ≈ 100 * (900 - 100_000) / 1000 / 1   → very negative for long subtraction → result negative → clamp 0
    let liq = liquidation_price(
        open,
        true,
        collateral,
        leverage,
        100_000 * ONE_USDC,
        0,
        LIQ_THRESHOLD_P,
    )
    .unwrap();
    assert!(liq >= 0);
}

#[test]
fn is_liquidatable_long_threshold() {
    let liq = 91 * PRICE_SCALE;
    assert!(is_liquidatable(91 * PRICE_SCALE, liq, true));
    assert!(is_liquidatable(90 * PRICE_SCALE, liq, true));
    assert!(!is_liquidatable(92 * PRICE_SCALE, liq, true));
}

#[test]
fn is_liquidatable_short_threshold() {
    let liq = 109 * PRICE_SCALE;
    assert!(is_liquidatable(109 * PRICE_SCALE, liq, false));
    assert!(is_liquidatable(110 * PRICE_SCALE, liq, false));
    assert!(!is_liquidatable(108 * PRICE_SCALE, liq, false));
}

// ────────────────────────────────────────────────────────────────────────────
// settlement payout
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn settlement_break_even_no_fees_returns_collateral_minus_close_fee() {
    let collateral = 1000 * ONE_USDC;
    let close_fee = 8 * ONE_USDC;
    let (payout, charged) = settlement_payout(collateral, 0, 0, 0, close_fee, LIQ_THRESHOLD_P).unwrap();
    assert_eq!(payout, collateral - close_fee);
    assert_eq!(charged, close_fee);
}

#[test]
fn settlement_profit_payout() {
    // +10% PnL on 1000 collateral → value = 1100; minus 8 close fee → 1092
    let collateral = 1000 * ONE_USDC;
    let pnl = 10 * P_SCALE; // +10%
    let close_fee = 8 * ONE_USDC;
    let (payout, charged) = settlement_payout(collateral, pnl, 0, 0, close_fee, LIQ_THRESHOLD_P).unwrap();
    assert_eq!(payout, 1100 * ONE_USDC - close_fee);
    assert_eq!(charged, close_fee);
}

#[test]
fn settlement_insolvency_waives_close_fee() {
    // -95% PnL on 1000 → value = 50; cutoff (100-90)/100 * 1000 = 100; value < cutoff → 0/0
    let collateral = 1000 * ONE_USDC;
    let pnl = -95 * P_SCALE;
    let close_fee = 8 * ONE_USDC;
    let (payout, charged) = settlement_payout(collateral, pnl, 0, 0, close_fee, LIQ_THRESHOLD_P).unwrap();
    assert_eq!(payout, 0);
    assert_eq!(charged, 0);
}

#[test]
fn settlement_loss_just_above_cutoff_charges_close_fee() {
    // -85% PnL on 1000 → value = 150; cutoff = 100 → not insolvent → payout = 150 - 8 = 142
    let collateral = 1000 * ONE_USDC;
    let pnl = -85 * P_SCALE;
    let close_fee = 8 * ONE_USDC;
    let (payout, charged) = settlement_payout(collateral, pnl, 0, 0, close_fee, LIQ_THRESHOLD_P).unwrap();
    assert_eq!(payout, 150 * ONE_USDC - close_fee);
    assert_eq!(charged, close_fee);
}

#[test]
fn settlement_close_fee_pushes_negative_clamps_to_zero() {
    // tiny value, close_fee bigger → net would be negative → 0 (still charged)
    let collateral = 1000 * ONE_USDC;
    // value before fee = collateral + 0 - rollover - funding = 5
    let (payout, charged) = settlement_payout(
        collateral,
        0,
        995 * ONE_USDC, // rollover
        0,
        100 * ONE_USDC, // close_fee bigger than residue
        LIQ_THRESHOLD_P,
    )
    .unwrap();
    // value = 5; cutoff = 100; 5 ≤ 100 → insolvent → (0, 0)
    assert_eq!(payout, 0);
    assert_eq!(charged, 0);
}

#[test]
fn settlement_capital_preserved_invariant_under_loss() {
    // when not insolvent, gross_payout + close_fee_charged ≤ collateral + pnl_amount - rollover - funding
    let collateral = 1000 * ONE_USDC;
    let pnl = -50 * P_SCALE; // -50%
    let close_fee = 8 * ONE_USDC;
    let (payout, charged) = settlement_payout(collateral, pnl, 0, 0, close_fee, LIQ_THRESHOLD_P).unwrap();
    let expected_value = 500 * ONE_USDC; // collateral + pnl
    assert_eq!(payout + charged, expected_value);
}
