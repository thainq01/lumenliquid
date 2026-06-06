#![no_std]

//! Shared math primitives for the Soroban perp DEX.
//!
//! Storage is i128 (compact under Soroban's read-bytes pricing). Intermediate
//! `a * b / denom` chains use [`mul_div_floor`] which performs a checked i128
//! multiply and then divides. All call-site magnitudes in the perp DEX
//! (USDC 1e7, price 1e10, percent 1e10, leverage ≤ ~150, OI ≤ ~1e13) fit in
//! i128 without i256 widening.

pub mod scale;
pub mod types;
pub mod fees;
pub mod liq;
pub mod errors;

pub use errors::MathError;
pub use scale::*;
pub use types::*;

/// Floor-rounded `(a * b) / denom` with checked overflow on the multiply.
/// Returns `Err(Overflow)` if the i128 product would wrap.
/// `denom` MUST be non-zero — callers ensure this; we return `Err(DivByZero)`
/// defensively.
#[inline]
pub fn mul_div_floor(a: i128, b: i128, denom: i128) -> Result<i128, MathError> {
    if denom == 0 {
        return Err(MathError::DivByZero);
    }
    let prod = a.checked_mul(b).ok_or(MathError::Overflow)?;
    // i128 division truncates toward zero. For non-negative `prod` this is
    // already floor; for negative `prod` we adjust to floor (Rust's `/`
    // would otherwise round toward zero, giving the wrong sign of remainder
    // for accumulator math).
    let q = prod / denom;
    let r = prod % denom;
    if (r != 0) && ((r < 0) != (denom < 0)) {
        // signs differ → result rounded toward zero, so subtract 1 to floor.
        Ok(q.checked_sub(1).ok_or(MathError::Overflow)?)
    } else {
        Ok(q)
    }
}
