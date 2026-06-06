//! Scale constants. Choices match `openspec/changes/port-trading-to-soroban/design.md` Decision 3.

/// USDC native scale on Stellar (matches the SAC).
pub const USDC_SCALE: i128 = 10_000_000; // 1e7

/// Price scale, `PRECISION = 1e10`.
pub const PRICE_SCALE: i128 = 10_000_000_000; // 1e10

/// Percentage scale, used for fee rates, PnL caps, liq thresholds, etc.
pub const P_SCALE: i128 = 10_000_000_000; // 1e10

/// Default per-side group fee at `P_SCALE`: 0.08% = 8e7.
pub const DEFAULT_GROUP_FEE_P: i128 = 80_000_000;

/// `MAX_GAIN_P = 900` (= 900%, expressed as integer percent — NOT P_SCALE).
pub const MAX_GAIN_P: u32 = 900;

/// `EXCEPTION_PAIR_MAX_GAIN_P = 300` for flagged pairs (xBTC/xETH on indices 100/101).
pub const EXCEPTION_PAIR_MAX_GAIN_P: u32 = 300;

/// `MAX_SL_P = 75` (max stop-loss = -75% of collateral).
pub const MAX_SL_P: u32 = 75;

/// `LIQ_THRESHOLD_P = 90` (default liquidation threshold = -90% of collateral).
pub const LIQ_THRESHOLD_P: u32 = 90;

/// Reflector callback drift cap (1% at `P_SCALE`).
pub const MAX_CALLBACK_PRICE_DRIFT_P: i128 = 100_000_000; // 1e8

/// Per-ledger price deviation safety cap (10% at `P_SCALE`).
pub const MAX_PRICE_DEVIATION_PER_LEDGER_P: i128 = 1_000_000_000; // 1e9

/// Trusted-baseline reset window for the deviation cap (~100 minutes at 5s/ledger).
pub const MAX_TRUSTED_PRICE_AGE_LEDGERS: u32 = 1_200;

/// Default re-subscription gating window (~1 hour at 5s/ledger).
pub const RESUB_LEDGER_INTERVAL: u32 = 720;