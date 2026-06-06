//! Errors raised by the registry contract.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PairRegistryError {
    /// `init` called twice.
    AlreadyInitialized = 1,
    /// Entry point requires the registry to have been `init`'d.
    NotInitialized = 2,
    /// Caller is not the admin.
    NotAdmin = 3,
    /// Caller is not the pinned PositionManager (mutators only).
    NotPositionManager = 4,
    /// Pair index has no PairInfo.
    PairNotFound = 5,
    /// Pair already exists at the given index.
    PairAlreadyExists = 6,
    /// Group index has no Group.
    GroupNotFound = 7,
    /// Group already exists at the given index.
    GroupAlreadyExists = 8,
    /// Numeric input outside accepted range (negative fee, leverage 0, etc.).
    InvalidParam = 9,
    /// Underlying math overflowed or div-by-zero'd.
    MathFault = 10,
    /// `commit_*` called with a stale `at_ledger` (already past).
    StaleLedger = 11,
}
