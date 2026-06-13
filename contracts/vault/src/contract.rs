//! `VaultContract` — SEP-0056 tokenized vault whose shares are a hand-rolled
//! SEP-41 token ("gToken"). Holds LP USDC that backs trader PnL.
//!
//! Custody model (per `openspec/.../spec.md`, authoritative): trader collateral
//! is custodied by the PositionManager during a trade's life. The Vault only
//! moves USDC on LP flows (`deposit`/`withdraw`/`redeem`/`mint`), close
//! settlement (`return_collateral_with_pnl`), and bad-debt recording. At close
//! the PositionManager transfers `effective_collateral` into the Vault *before*
//! calling `return_collateral_with_pnl`; the Vault then pays `gross_payout` to
//! the trader and adjusts `total_assets` by `-net_pnl_for_vault`.

use soroban_sdk::{
    contract, contractimpl, token::TokenClient, Address, BytesN, Env, MuxedAddress, String, Symbol,
};

use crate::errors::VaultError;
use crate::storage;
use crate::types::AllowanceValue;

const DECIMALS: u32 = 7;

#[contract]
pub struct VaultContract;

// ───────────────────────────── helpers ─────────────────────────────

fn require_admin(env: &Env) -> Result<(), VaultError> {
    let admin = storage::read_admin(env)?;
    admin.require_auth();
    Ok(())
}

fn require_position_manager(env: &Env) -> Result<(), VaultError> {
    let pm = storage::read_position_manager(env)?;
    pm.require_auth();
    Ok(())
}

fn require_not_paused(env: &Env) -> Result<(), VaultError> {
    if storage::read_paused(env) {
        return Err(VaultError::Paused);
    }
    Ok(())
}

fn usdc_client(env: &Env) -> Result<TokenClient<'_>, VaultError> {
    let token = storage::read_usdc_token(env)?;
    Ok(TokenClient::new(env, &token))
}

/// Explicit `Address → MuxedAddress` to disambiguate `.into()` at the SEP-41
/// `transfer` call sites (multiple `From<Address>` impls exist in the SDK).
fn mux(addr: &Address) -> MuxedAddress {
    addr.into()
}

/// shares = assets * total_shares / total_assets (1:1 bootstrap when empty).
fn assets_to_shares(assets: i128, total_assets: i128, total_shares: i128) -> Result<i128, VaultError> {
    if total_shares == 0 || total_assets == 0 {
        return Ok(assets);
    }
    math::mul_div_floor(assets, total_shares, total_assets).map_err(|_| VaultError::MathFault)
}

/// assets = shares * total_assets / total_shares (1:1 bootstrap when empty).
fn shares_to_assets(shares: i128, total_assets: i128, total_shares: i128) -> Result<i128, VaultError> {
    if total_shares == 0 {
        return Ok(shares);
    }
    math::mul_div_floor(shares, total_assets, total_shares).map_err(|_| VaultError::MathFault)
}

/// Mint `amount` shares to `to`, bumping `total_shares`.
fn mint_shares(env: &Env, to: &Address, amount: i128) -> Result<(), VaultError> {
    let bal = storage::read_balance(env, to);
    let new_bal = bal.checked_add(amount).ok_or(VaultError::MathFault)?;
    storage::write_balance(env, to, new_bal);
    let total = storage::read_total_shares(env);
    storage::write_total_shares(env, total.checked_add(amount).ok_or(VaultError::MathFault)?);
    Ok(())
}

/// Burn `amount` shares from `from`, lowering `total_shares`.
fn burn_shares(env: &Env, from: &Address, amount: i128) -> Result<(), VaultError> {
    let bal = storage::read_balance(env, from);
    if bal < amount {
        return Err(VaultError::InsufficientShares);
    }
    storage::write_balance(env, from, bal - amount);
    let total = storage::read_total_shares(env);
    storage::write_total_shares(env, total.checked_sub(amount).ok_or(VaultError::MathFault)?);
    Ok(())
}

/// Move `amount` shares between two holders (SEP-41 transfer body).
fn move_shares(env: &Env, from: &Address, to: &Address, amount: i128) -> Result<(), VaultError> {
    if amount < 0 {
        return Err(VaultError::InvalidParam);
    }
    let from_bal = storage::read_balance(env, from);
    if from_bal < amount {
        return Err(VaultError::InsufficientShares);
    }
    storage::write_balance(env, from, from_bal - amount);
    let to_bal = storage::read_balance(env, to);
    storage::write_balance(env, to, to_bal.checked_add(amount).ok_or(VaultError::MathFault)?);
    Ok(())
}

/// Consume `amount` from the allowance `from → spender` (reverts if short).
fn spend_allowance(env: &Env, from: &Address, spender: &Address, amount: i128) -> Result<(), VaultError> {
    let allow = storage::read_allowance(env, from, spender);
    if allow.amount < amount {
        return Err(VaultError::InsufficientAllowance);
    }
    storage::write_allowance(
        env,
        from,
        spender,
        &AllowanceValue {
            amount: allow.amount - amount,
            expiration_ledger: allow.expiration_ledger,
        },
    );
    Ok(())
}

#[contractimpl]
impl VaultContract {
    /// One-shot initialization. `admin` owns admin entry points;
    /// `position_manager` is the only address allowed on the collateral path;
    /// `usdc_token` is the underlying SEP-41 asset; `withdraw_lock_ledgers`
    /// gates how long after a deposit an LP must wait to withdraw.
    pub fn init(
        env: Env,
        admin: Address,
        position_manager: Address,
        usdc_token: Address,
        withdraw_lock_ledgers: u32,
    ) -> Result<(), VaultError> {
        if storage::is_initialized(&env) {
            return Err(VaultError::AlreadyInitialized);
        }
        admin.require_auth();
        storage::write_admin(&env, &admin);
        storage::write_position_manager(&env, &position_manager);
        storage::write_usdc_token(&env, &usdc_token);
        storage::write_withdraw_lock_ledgers(&env, withdraw_lock_ledgers);
        storage::write_total_assets(&env, 0);
        storage::write_total_shares(&env, 0);
        storage::write_paused(&env, false);
        env.events().publish(
            (Symbol::new(&env, "init"),),
            (admin, position_manager, usdc_token, withdraw_lock_ledgers),
        );
        Ok(())
    }

    // ─────────────── SEP-41 token surface (gToken shares) ───────────────

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        storage::read_allowance(&env, &from, &spender).amount
    }

    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) -> Result<(), VaultError> {
        from.require_auth();
        if amount < 0 {
            return Err(VaultError::InvalidParam);
        }
        storage::write_allowance(
            &env,
            &from,
            &spender,
            &AllowanceValue {
                amount,
                expiration_ledger,
            },
        );
        env.events().publish(
            (Symbol::new(&env, "approve"), from, spender),
            (amount, expiration_ledger),
        );
        Ok(())
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        storage::read_balance(&env, &id)
    }

    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) -> Result<(), VaultError> {
        from.require_auth();
        let to_addr = to.address();
        move_shares(&env, &from, &to_addr, amount)?;
        env.events()
            .publish((Symbol::new(&env, "transfer"), from, to_addr), amount);
        Ok(())
    }

    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), VaultError> {
        spender.require_auth();
        spend_allowance(&env, &from, &spender, amount)?;
        move_shares(&env, &from, &to, amount)?;
        env.events()
            .publish((Symbol::new(&env, "transfer"), from, to), amount);
        Ok(())
    }

    pub fn burn(env: Env, from: Address, amount: i128) -> Result<(), VaultError> {
        from.require_auth();
        if amount < 0 {
            return Err(VaultError::InvalidParam);
        }
        burn_shares(&env, &from, amount)?;
        env.events()
            .publish((Symbol::new(&env, "burn"), from), amount);
        Ok(())
    }

    pub fn burn_from(
        env: Env,
        spender: Address,
        from: Address,
        amount: i128,
    ) -> Result<(), VaultError> {
        spender.require_auth();
        if amount < 0 {
            return Err(VaultError::InvalidParam);
        }
        spend_allowance(&env, &from, &spender, amount)?;
        burn_shares(&env, &from, amount)?;
        env.events()
            .publish((Symbol::new(&env, "burn"), from), amount);
        Ok(())
    }

    pub fn decimals(_env: Env) -> u32 {
        DECIMALS
    }

    pub fn name(env: Env) -> String {
        String::from_str(&env, "LumenLiquid gToken")
    }

    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "gUSDC")
    }

    // ─────────────── SEP-0056 vault ops ───────────────

    /// LP deposits `assets` USDC; mints shares to `receiver` at the current
    /// share price. Records `receiver`'s deposit ledger for the withdraw lock.
    pub fn deposit(
        env: Env,
        from: Address,
        assets: i128,
        receiver: Address,
    ) -> Result<i128, VaultError> {
        require_not_paused(&env)?;
        from.require_auth();
        if assets <= 0 {
            return Err(VaultError::InvalidParam);
        }
        let total_assets = storage::read_total_assets(&env);
        let total_shares = storage::read_total_shares(&env);
        let shares = assets_to_shares(assets, total_assets, total_shares)?;
        if shares <= 0 {
            return Err(VaultError::InvalidParam);
        }
        // Pull USDC from the LP into the vault.
        usdc_client(&env)?.transfer(&from, &mux(&env.current_contract_address()), &assets);
        storage::write_total_assets(&env, total_assets.checked_add(assets).ok_or(VaultError::MathFault)?);
        mint_shares(&env, &receiver, shares)?;
        storage::write_last_deposit_ledger(&env, &receiver, env.ledger().sequence());
        env.events()
            .publish((Symbol::new(&env, "deposit"), from, receiver), (assets, shares));
        Ok(shares)
    }

    /// LP mints exactly `shares` to `receiver`, paying the required USDC.
    pub fn mint(
        env: Env,
        from: Address,
        shares: i128,
        receiver: Address,
    ) -> Result<i128, VaultError> {
        require_not_paused(&env)?;
        from.require_auth();
        if shares <= 0 {
            return Err(VaultError::InvalidParam);
        }
        let total_assets = storage::read_total_assets(&env);
        let total_shares = storage::read_total_shares(&env);
        let assets = shares_to_assets(shares, total_assets, total_shares)?;
        if assets <= 0 {
            return Err(VaultError::InvalidParam);
        }
        usdc_client(&env)?.transfer(&from, &mux(&env.current_contract_address()), &assets);
        storage::write_total_assets(&env, total_assets.checked_add(assets).ok_or(VaultError::MathFault)?);
        mint_shares(&env, &receiver, shares)?;
        storage::write_last_deposit_ledger(&env, &receiver, env.ledger().sequence());
        env.events()
            .publish((Symbol::new(&env, "deposit"), from, receiver), (assets, shares));
        Ok(assets)
    }

    /// Burn `shares` from `owner`, sending the equivalent USDC to `receiver`.
    /// Reverts `WithdrawLocked` until `last_deposit_ledger + withdraw_lock`.
    pub fn redeem(
        env: Env,
        owner: Address,
        shares: i128,
        receiver: Address,
    ) -> Result<i128, VaultError> {
        require_not_paused(&env)?;
        owner.require_auth();
        if shares <= 0 {
            return Err(VaultError::InvalidParam);
        }
        Self::check_withdraw_lock(&env, &owner)?;
        let total_assets = storage::read_total_assets(&env);
        let total_shares = storage::read_total_shares(&env);
        let assets = shares_to_assets(shares, total_assets, total_shares)?;
        if assets > total_assets {
            return Err(VaultError::InsufficientAssets);
        }
        burn_shares(&env, &owner, shares)?;
        storage::write_total_assets(&env, total_assets - assets);
        usdc_client(&env)?.transfer(&env.current_contract_address(), &mux(&receiver), &assets);
        env.events()
            .publish((Symbol::new(&env, "withdraw"), owner, receiver), (assets, shares));
        Ok(assets)
    }

    /// Withdraw exactly `assets` USDC to `receiver`, burning the required
    /// shares from `owner`. Subject to the same withdraw lock as `redeem`.
    pub fn withdraw(
        env: Env,
        owner: Address,
        assets: i128,
        receiver: Address,
    ) -> Result<i128, VaultError> {
        require_not_paused(&env)?;
        owner.require_auth();
        if assets <= 0 {
            return Err(VaultError::InvalidParam);
        }
        Self::check_withdraw_lock(&env, &owner)?;
        let total_assets = storage::read_total_assets(&env);
        let total_shares = storage::read_total_shares(&env);
        if assets > total_assets {
            return Err(VaultError::InsufficientAssets);
        }
        // Ceil the shares so the vault is never short-changed by flooring.
        let shares = {
            let floored = assets_to_shares(assets, total_assets, total_shares)?;
            let round_trip = shares_to_assets(floored, total_assets, total_shares)?;
            if round_trip < assets {
                floored.checked_add(1).ok_or(VaultError::MathFault)?
            } else {
                floored
            }
        };
        burn_shares(&env, &owner, shares)?;
        storage::write_total_assets(&env, total_assets - assets);
        usdc_client(&env)?.transfer(&env.current_contract_address(), &mux(&receiver), &assets);
        env.events()
            .publish((Symbol::new(&env, "withdraw"), owner, receiver), (assets, shares));
        Ok(shares)
    }

    fn check_withdraw_lock(env: &Env, owner: &Address) -> Result<(), VaultError> {
        let lock = storage::read_withdraw_lock_ledgers(env);
        if lock == 0 {
            return Ok(());
        }
        let last = storage::read_last_deposit_ledger(env, owner);
        let unlock_at = last.saturating_add(lock);
        if env.ledger().sequence() < unlock_at {
            return Err(VaultError::WithdrawLocked);
        }
        Ok(())
    }

    // ─────────────── PositionManager-only collateral path ───────────────

    /// Move `amount` USDC out of the vault to the PositionManager as working
    /// capital. PM-only. Debits `total_assets`. Present for the auth surface;
    /// on the spec.md custody model the open path does NOT call this (the PM
    /// custodies effective_collateral directly), but it is the primitive the
    /// PM uses if it ever needs vault-held principal.
    pub fn take_collateral(env: Env, amount: i128) -> Result<(), VaultError> {
        require_position_manager(&env)?;
        if amount <= 0 {
            return Err(VaultError::InvalidParam);
        }
        let total_assets = storage::read_total_assets(&env);
        if amount > total_assets {
            return Err(VaultError::InsufficientAssets);
        }
        let pm = storage::read_position_manager(&env)?;
        storage::write_total_assets(&env, total_assets - amount);
        usdc_client(&env)?.transfer(&env.current_contract_address(), &mux(&pm), &amount);
        env.events()
            .publish((Symbol::new(&env, "take_collat"),), (pm, amount));
        Ok(())
    }

    /// Settle a closing trade. PM-only.
    ///
    /// Precondition (spec.md custody model): the PositionManager has ALREADY
    /// transferred `effective_collateral` USDC into this vault before calling.
    /// The vault then pays `gross_payout = effective_collateral + net_pnl_for_vault`
    /// to the trader and adjusts equity by `-net_pnl_for_vault`:
    ///   * trader profit  → net_pnl_for_vault > 0 → vault pays principal + profit, equity drops
    ///   * trader loss     → net_pnl_for_vault < 0 → vault keeps the loss, equity rises
    ///   * insolvency      → net_pnl_for_vault = -effective_collateral → payout 0, vault keeps all
    pub fn return_collateral_with_pnl(
        env: Env,
        trader: Address,
        effective_collateral: i128,
        net_pnl_for_vault: i128,
    ) -> Result<i128, VaultError> {
        require_position_manager(&env)?;
        if effective_collateral < 0 {
            return Err(VaultError::InvalidParam);
        }
        let gross_payout = effective_collateral
            .checked_add(net_pnl_for_vault)
            .ok_or(VaultError::MathFault)?;
        if gross_payout < 0 {
            return Err(VaultError::InvalidParam);
        }
        // Equity moves opposite the trader's realized PnL. The principal just
        // deposited by the PM nets out; only the PnL delta changes total_assets.
        let total_assets = storage::read_total_assets(&env);
        let new_total = total_assets
            .checked_sub(net_pnl_for_vault)
            .ok_or(VaultError::MathFault)?;
        if new_total < 0 {
            return Err(VaultError::InsufficientAssets);
        }
        storage::write_total_assets(&env, new_total);
        if gross_payout > 0 {
            usdc_client(&env)?.transfer(
                &env.current_contract_address(),
                &mux(&trader),
                &gross_payout,
            );
        }
        env.events().publish(
            (Symbol::new(&env, "settle"), trader),
            (effective_collateral, net_pnl_for_vault, gross_payout),
        );
        Ok(gross_payout)
    }

    /// Record bad debt for a pair (loss the vault must absorb beyond collateral).
    /// PM-only. Adds to `bad_debt_pool[pair_index]` and debits `total_assets`.
    pub fn record_bad_debt(
        env: Env,
        pair_index: u32,
        amount: i128,
    ) -> Result<(), VaultError> {
        require_position_manager(&env)?;
        if amount <= 0 {
            return Err(VaultError::InvalidParam);
        }
        let pool = storage::read_bad_debt_pool(&env, pair_index);
        storage::write_bad_debt_pool(
            &env,
            pair_index,
            pool.checked_add(amount).ok_or(VaultError::MathFault)?,
        );
        let total_assets = storage::read_total_assets(&env);
        let new_total = total_assets.checked_sub(amount).ok_or(VaultError::MathFault)?;
        // total_assets floors at 0 — LP equity cannot go negative on-chain.
        storage::write_total_assets(&env, if new_total < 0 { 0 } else { new_total });
        env.events()
            .publish((Symbol::new(&env, "bad_debt"), pair_index), amount);
        Ok(())
    }

    // ─────────────── admin ───────────────

    pub fn set_withdraw_lock(env: Env, ledgers: u32) -> Result<(), VaultError> {
        require_admin(&env)?;
        storage::write_withdraw_lock_ledgers(&env, ledgers);
        env.events()
            .publish((Symbol::new(&env, "set_lock"),), ledgers);
        Ok(())
    }

    pub fn set_position_manager(env: Env, position_manager: Address) -> Result<(), VaultError> {
        require_admin(&env)?;
        storage::write_position_manager(&env, &position_manager);
        env.events()
            .publish((Symbol::new(&env, "set_pm"),), position_manager);
        Ok(())
    }

    pub fn pause(env: Env) -> Result<(), VaultError> {
        require_admin(&env)?;
        storage::write_paused(&env, true);
        env.events().publish((Symbol::new(&env, "paused"),), ());
        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), VaultError> {
        require_admin(&env)?;
        storage::write_paused(&env, false);
        env.events().publish((Symbol::new(&env, "unpaused"),), ());
        Ok(())
    }

    /// Hot-swap the contract wasm. Admin-only. Storage layout MUST stay
    /// backwards-compatible (adding new `DataKey` variants is safe).
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), VaultError> {
        require_admin(&env)?;
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.events()
            .publish((Symbol::new(&env, "upgraded"),), new_wasm_hash);
        Ok(())
    }

    // ─────────────── views ───────────────

    pub fn total_assets(env: Env) -> i128 {
        storage::read_total_assets(&env)
    }

    pub fn total_shares(env: Env) -> i128 {
        storage::read_total_shares(&env)
    }

    pub fn convert_to_shares(env: Env, assets: i128) -> Result<i128, VaultError> {
        assets_to_shares(
            assets,
            storage::read_total_assets(&env),
            storage::read_total_shares(&env),
        )
    }

    pub fn convert_to_assets(env: Env, shares: i128) -> Result<i128, VaultError> {
        shares_to_assets(
            shares,
            storage::read_total_assets(&env),
            storage::read_total_shares(&env),
        )
    }

    pub fn bad_debt_pool(env: Env, pair_index: u32) -> i128 {
        storage::read_bad_debt_pool(&env, pair_index)
    }

    pub fn withdraw_lock_ledgers(env: Env) -> u32 {
        storage::read_withdraw_lock_ledgers(&env)
    }

    pub fn last_deposit_ledger(env: Env, holder: Address) -> u32 {
        storage::read_last_deposit_ledger(&env, &holder)
    }

    pub fn is_paused(env: Env) -> bool {
        storage::read_paused(&env)
    }

    pub fn admin(env: Env) -> Result<Address, VaultError> {
        storage::read_admin(&env)
    }

    pub fn position_manager(env: Env) -> Result<Address, VaultError> {
        storage::read_position_manager(&env)
    }

    pub fn usdc_token(env: Env) -> Result<Address, VaultError> {
        storage::read_usdc_token(&env)
    }


}

