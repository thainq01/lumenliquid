//! Strongly-typed storage records and storage-key enum for the Vault.
//!
//! The Vault is a SEP-0056 tokenized vault whose shares are a hand-rolled
//! SEP-41 token (the "gToken"). Storage is split:
//!
//! * **Instance**: admin, position_manager, usdc_token, total_assets,
//!   total_shares, withdraw_lock_ledgers, paused. Touched by nearly every
//!   entry point — one bumped instance read covers them all.
//! * **Persistent**: per-holder share balance, SEP-41 allowances, per-LP
//!   `last_deposit_ledger`, and the per-pair `bad_debt_pool`.

use soroban_sdk::{contracttype, Address};

/// SEP-41 allowance lookup key.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceKey {
    pub from: Address,
    pub spender: Address,
}

/// SEP-41 allowance value — amount plus the ledger at which it expires.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}

/// All instance/persistent storage keys for the vault.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // ── instance ──
    Admin,
    PositionManager,
    /// USDC SAC address — the underlying asset of the vault.
    UsdcToken,
    /// Total USDC backing the vault (LP principal + retained PnL − bad debt).
    TotalAssets,
    /// Total gToken shares outstanding.
    TotalShares,
    /// Withdraw lock window in ledgers (deposit → first allowed withdraw).
    WithdrawLockLedgers,
    /// Emergency pause flag.
    Paused,
    // ── persistent ──
    /// Per-holder gToken share balance.
    Balance(Address),
    /// SEP-41 allowance.
    Allowance(AllowanceKey),
    /// Ledger at which `holder` last deposited (gates withdraw lock).
    LastDepositLedger(Address),
    /// Per-pair accumulated bad debt (USDC at USDC_SCALE).
    BadDebtPool(u32),
}
