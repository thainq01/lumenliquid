//! Typed accessors over Soroban storage for the Vault. Mirrors the structure
//! of `pair-registry/src/storage.rs`: instance singletons up top, persistent
//! per-key records below.

use soroban_sdk::{Address, Env};

use crate::errors::VaultError;
use crate::types::{AllowanceKey, AllowanceValue, DataKey};

// ───────────────────────── instance ─────────────────────────

pub fn read_admin(env: &Env) -> Result<Address, VaultError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(VaultError::NotInitialized)
}

pub fn write_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_position_manager(env: &Env) -> Result<Address, VaultError> {
    env.storage()
        .instance()
        .get(&DataKey::PositionManager)
        .ok_or(VaultError::NotInitialized)
}

pub fn write_position_manager(env: &Env, pm: &Address) {
    env.storage().instance().set(&DataKey::PositionManager, pm);
}

pub fn read_usdc_token(env: &Env) -> Result<Address, VaultError> {
    env.storage()
        .instance()
        .get(&DataKey::UsdcToken)
        .ok_or(VaultError::NotInitialized)
}

pub fn write_usdc_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::UsdcToken, token);
}

pub fn read_total_assets(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalAssets)
        .unwrap_or(0)
}

pub fn write_total_assets(env: &Env, value: i128) {
    env.storage().instance().set(&DataKey::TotalAssets, &value);
}

pub fn read_total_shares(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalShares)
        .unwrap_or(0)
}

pub fn write_total_shares(env: &Env, value: i128) {
    env.storage().instance().set(&DataKey::TotalShares, &value);
}

pub fn read_withdraw_lock_ledgers(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::WithdrawLockLedgers)
        .unwrap_or(0)
}

pub fn write_withdraw_lock_ledgers(env: &Env, value: u32) {
    env.storage()
        .instance()
        .set(&DataKey::WithdrawLockLedgers, &value);
}

pub fn read_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn write_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

// ───────────────────────── persistent: shares ─────────────────────────

pub fn read_balance(env: &Env, holder: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(holder.clone()))
        .unwrap_or(0)
}

pub fn write_balance(env: &Env, holder: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Balance(holder.clone()), &amount);
}

// ───────────────────────── persistent: allowances ─────────────────────────

pub fn read_allowance(env: &Env, from: &Address, spender: &Address) -> AllowanceValue {
    let key = DataKey::Allowance(AllowanceKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    match env.storage().persistent().get::<_, AllowanceValue>(&key) {
        Some(v) if v.expiration_ledger >= env.ledger().sequence() => v,
        // Missing or expired allowances read as zero (SEP-41 semantics).
        _ => AllowanceValue {
            amount: 0,
            expiration_ledger: 0,
        },
    }
}

pub fn write_allowance(env: &Env, from: &Address, spender: &Address, value: &AllowanceValue) {
    let key = DataKey::Allowance(AllowanceKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    env.storage().persistent().set(&key, value);
}

// ───────────────────────── persistent: lock / bad debt ─────────────────────────

pub fn read_last_deposit_ledger(env: &Env, holder: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::LastDepositLedger(holder.clone()))
        .unwrap_or(0)
}

pub fn write_last_deposit_ledger(env: &Env, holder: &Address, ledger: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::LastDepositLedger(holder.clone()), &ledger);
}

pub fn read_bad_debt_pool(env: &Env, pair_index: u32) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::BadDebtPool(pair_index))
        .unwrap_or(0)
}

pub fn write_bad_debt_pool(env: &Env, pair_index: u32, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::BadDebtPool(pair_index), &amount);
}
