//! Strongly-typed storage records and storage-key enum for the registry.

use soroban_sdk::{contracttype, Address, Symbol};

use reflector_adapter::ReflectorAsset;

/// Per-pair configuration as defined in `design.md` Decision 4.
///
/// Scales:
/// * `spread_p` and `max_neg_pnl_p` are at `P_SCALE = 1e10` (e.g. `5e7` = 0.05%).
/// * `min_lev_pos_usdc` and `max_oi_usdc` are USDC at `USDC_SCALE = 1e7`.
/// * `liq_threshold_p` and `max_gain_p` are integer percent (NOT P_SCALE),
///   e.g. `90`, `900`.
///
/// `min_leverage`/`max_leverage` are integer leverages (e.g. `2`, `5`, `50`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairInfo {
    pub symbol: Symbol,
    pub reflector_asset: ReflectorAsset,
    pub group_index: u32,
    pub spread_p: i128,
    pub min_leverage: u32,
    pub max_leverage: u32,
    pub min_lev_pos_usdc: i128,
    pub max_oi_usdc: i128,
    pub max_neg_pnl_p: i128,
    pub liq_threshold_p: u32,
    pub max_gain_p: u32,
    pub disabled: bool,
}

/// Group configuration (e.g. all crypto pairs share one group).
///
/// `open_fee_p` / `close_fee_p` are at `P_SCALE = 1e10` (default `8e7` = 0.08%).
/// `max_collateral_usdc` is USDC at `USDC_SCALE = 1e7`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    pub name: Symbol,
    pub max_collateral_usdc: i128,
    pub open_fee_p: i128,
    pub close_fee_p: i128,
}

/// Rollover-fee accumulator state for a pair. Tracks the per-pair rollover
/// fields used by the position manager.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloverState {
    /// `accPerCollateral` — monotonic accumulator (USDC at USDC_SCALE per
    /// unit collateral; i.e. dimensionless after dividing by collateral).
    pub acc_per_collateral: i128,
    /// `rolloverFeePerLedgerP` — rate at `P_SCALE`.
    pub fee_per_ledger_p: i128,
    /// Last ledger sequence at which the accumulator was committed.
    pub last_update_ledger: u32,
}

impl Default for RolloverState {
    fn default() -> Self {
        Self {
            acc_per_collateral: 0,
            fee_per_ledger_p: 0,
            last_update_ledger: 0,
        }
    }
}

/// Funding-fee accumulator state for a pair (asymmetric long/short).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingState {
    /// `accPerOiLong` — accumulator on long side (USDC per OI, can be negative).
    pub acc_long: i128,
    /// `accPerOiShort` — accumulator on short side (USDC per OI, can be negative).
    pub acc_short: i128,
    /// `fundingFeePerLedgerP` — rate at `P_SCALE`.
    pub fee_per_ledger_p: i128,
    /// Last ledger sequence at which the accumulator was committed.
    pub last_update_ledger: u32,
}

impl Default for FundingState {
    fn default() -> Self {
        Self {
            acc_long: 0,
            acc_short: 0,
            fee_per_ledger_p: 0,
            last_update_ledger: 0,
        }
    }
}

/// Per-side open interest in USDC at `USDC_SCALE`.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct PairOi {
    pub long: i128,
    pub short: i128,
}

/// Trade snapshot used by liquidation/PnL views — supplied by callers (not
/// stored here, lives on PositionManager).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TradeMeta {
    pub pair_index: u32,
    pub is_long: bool,
    pub leverage: u32,
    pub open_price: i128,
    /// Effective collateral (post-open-fee) at USDC_SCALE.
    pub collateral: i128,
    /// `acc_per_collateral` snapshot at trade open.
    pub acc_rollover_open: i128,
    /// Funding accumulator snapshot for this trade's side at open.
    pub acc_funding_open: i128,
}

/// All instance/persistent storage keys for the registry.
///
/// Keep variants tightly scoped — every read costs read-bytes pricing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    PositionManager,
    MaxPosUsdc,
    PairsCount,
    Pair(u32),
    Group(u32),
    Rollover(u32),
    Funding(u32),
    OI(u32),
    /// One-percent depth used by price-impact math, USDC at `USDC_SCALE`.
    Depth(u32),
}
