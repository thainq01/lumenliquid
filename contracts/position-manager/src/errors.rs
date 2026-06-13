//! Errors raised by the PositionManager contract. Names mirror the spec's
//! revert tags (`openspec/.../spec.md`).

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PositionManagerError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAdmin = 3,
    Paused = 4,
    PairDisabled = 5,
    LeverageIncorrect = 6,
    AboveMaxPos = 7,
    BelowMinPos = 8,
    MaxTradesReached = 9,
    InvalidPriceProof = 10,
    PriceImpactTooHigh = 11,
    WrongTp = 12,
    WrongSl = 13,
    OiCapExceeded = 14,
    GroupCollateralCapExceeded = 15,
    InsufficientSubscriptionReserve = 16,
    TradeNotFound = 17,
    UnauthorizedCallback = 18,
    SubNotFound = 19,
    SubOrphaned = 20,
    PriceMismatch = 21,
    OracleDeviationTooHigh = 22,
    NotLiquidatable = 23,
    InsufficientAccruedFees = 24,
    MathFault = 25,
    InvalidParam = 26,
    OracleUnavailable = 27,
    LimitNotFound = 28,
    SubscriptionNotConfigured = 29,
}
