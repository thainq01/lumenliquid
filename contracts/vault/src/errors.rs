//! Errors raised by the Vault contract.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    /// `init` called twice.
    AlreadyInitialized = 1,
    /// Entry point requires the vault to have been `init`'d.
    NotInitialized = 2,
    /// Caller is not the admin.
    NotAdmin = 3,
    /// Caller is not the pinned PositionManager (collateral path only).
    NotPositionManager = 4,
    /// `withdraw`/`redeem` called before `last_deposit_ledger + withdraw_lock_ledgers`.
    WithdrawLocked = 5,
    /// Share balance is too small for the requested burn/transfer.
    InsufficientShares = 6,
    /// Vault holds fewer assets than the operation requires.
    InsufficientAssets = 7,
    /// Allowance is too small for a `transfer_from`/`burn_from`.
    InsufficientAllowance = 8,
    /// Operation attempted while the vault is paused.
    Paused = 9,
    /// Numeric input outside accepted range (negative amount, zero shares minted).
    InvalidParam = 10,
    /// Underlying math overflowed or div-by-zero'd.
    MathFault = 11,
}
