//! `PairRegistryContract` — entry points.

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Symbol};

use math::errors::MathError;
use math::fees::{pending_acc_funding, pending_acc_rollover};
use math::liq::{is_liquidatable, liquidation_price};
use math::scale::USDC_SCALE;
use math::fees::{funding_fee_for_trade, rollover_fee_for_trade};

use crate::errors::PairRegistryError;
use crate::storage;
use crate::types::{
    DataKey, FundingState, Group, PairInfo, PairOi, RolloverState, TradeMeta,
};

#[contract]
pub struct PairRegistryContract;

// ───────────────────────────── helpers ─────────────────────────────

fn require_admin(env: &Env) -> Result<(), PairRegistryError> {
    let admin = storage::read_admin(env)?;
    admin.require_auth();
    Ok(())
}

fn require_position_manager(env: &Env) -> Result<(), PairRegistryError> {
    let pm = storage::read_position_manager(env)?;
    pm.require_auth();
    Ok(())
}

fn map_math_err<T>(r: Result<T, MathError>) -> Result<T, PairRegistryError> {
    r.map_err(|_| PairRegistryError::MathFault)
}

fn validate_pair_info(pair: &PairInfo) -> Result<(), PairRegistryError> {
    if pair.min_leverage == 0
        || pair.max_leverage == 0
        || pair.max_leverage < pair.min_leverage
        || pair.spread_p < 0
        || pair.min_lev_pos_usdc < 0
        || pair.max_oi_usdc < 0
        || pair.max_neg_pnl_p < 0
        || pair.liq_threshold_p == 0
        || pair.liq_threshold_p > 100
        || pair.max_gain_p == 0
    {
        return Err(PairRegistryError::InvalidParam);
    }
    Ok(())
}

fn validate_group(group: &Group) -> Result<(), PairRegistryError> {
    if group.max_collateral_usdc < 0
        || group.open_fee_p < 0
        || group.close_fee_p < 0
    {
        return Err(PairRegistryError::InvalidParam);
    }
    Ok(())
}

fn delta_ledgers_to(state_last: u32, at_ledger: u32) -> Result<u32, PairRegistryError> {
    if at_ledger < state_last {
        return Err(PairRegistryError::StaleLedger);
    }
    Ok(at_ledger - state_last)
}

// ───────────────────────────── entry points ─────────────────────────────

#[contractimpl]
impl PairRegistryContract {
    /// One-shot initialization. `admin` will own all admin entry points;
    /// `position_manager` is the only address allowed to mutate accumulators
    /// and OI.
    pub fn init(
        env: Env,
        admin: Address,
        position_manager: Address,
        max_pos_usdc: i128,
    ) -> Result<(), PairRegistryError> {
        if storage::is_initialized(&env) {
            return Err(PairRegistryError::AlreadyInitialized);
        }
        if max_pos_usdc < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        admin.require_auth();
        storage::write_admin(&env, &admin);
        storage::write_position_manager(&env, &position_manager);
        storage::write_max_pos_usdc(&env, max_pos_usdc);
        storage::write_pairs_count(&env, 0);
        env.events().publish(
            (Symbol::new(&env, "init"),),
            (admin, position_manager, max_pos_usdc),
        );
        Ok(())
    }

    // ─────────────── admin: pair config ───────────────

    pub fn add_pair(
        env: Env,
        pair_index: u32,
        pair: PairInfo,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        validate_pair_info(&pair)?;
        if storage::has_pair(&env, pair_index) {
            return Err(PairRegistryError::PairAlreadyExists);
        }
        if !storage::has_group(&env, pair.group_index) {
            return Err(PairRegistryError::GroupNotFound);
        }
        let symbol = pair.symbol.clone();
        storage::write_pair(&env, pair_index, &pair);
        // Seed accumulator state at the current ledger so the first commit
        // computes a real delta (otherwise `last_update_ledger == 0` is
        // indistinguishable from "never committed" once the chain advances).
        let now_ledger = env.ledger().sequence();
        storage::write_rollover(
            &env,
            pair_index,
            &RolloverState {
                acc_per_collateral: 0,
                fee_per_ledger_p: 0,
                last_update_ledger: now_ledger,
            },
        );
        storage::write_funding(
            &env,
            pair_index,
            &FundingState {
                acc_long: 0,
                acc_short: 0,
                fee_per_ledger_p: 0,
                last_update_ledger: now_ledger,
            },
        );
        let count = storage::read_pairs_count(&env);
        storage::write_pairs_count(&env, count + 1);
        env.events().publish(
            (Symbol::new(&env, "pair_added"), pair_index),
            symbol,
        );
        Ok(())
    }

    pub fn update_pair(
        env: Env,
        pair_index: u32,
        new_pair: PairInfo,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        validate_pair_info(&new_pair)?;
        if !storage::has_pair(&env, pair_index) {
            return Err(PairRegistryError::PairNotFound);
        }
        if !storage::has_group(&env, new_pair.group_index) {
            return Err(PairRegistryError::GroupNotFound);
        }
        storage::write_pair(&env, pair_index, &new_pair);
        env.events()
            .publish((Symbol::new(&env, "pair_updated"), pair_index), ());
        Ok(())
    }

    pub fn disable_pair(env: Env, pair_index: u32) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        let mut pair = storage::read_pair(&env, pair_index)?;
        pair.disabled = true;
        storage::write_pair(&env, pair_index, &pair);
        env.events()
            .publish((Symbol::new(&env, "pair_disabled"), pair_index), ());
        Ok(())
    }

    pub fn set_rollover_rate_p(
        env: Env,
        pair_index: u32,
        rate_p: i128,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        if rate_p < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        if !storage::has_pair(&env, pair_index) {
            return Err(PairRegistryError::PairNotFound);
        }
        let mut state = storage::read_rollover(&env, pair_index);
        state.fee_per_ledger_p = rate_p;
        storage::write_rollover(&env, pair_index, &state);
        env.events().publish(
            (Symbol::new(&env, "rollover_rate"), pair_index),
            rate_p,
        );
        Ok(())
    }

    pub fn set_funding_rate_p(
        env: Env,
        pair_index: u32,
        rate_p: i128,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        if rate_p < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        if !storage::has_pair(&env, pair_index) {
            return Err(PairRegistryError::PairNotFound);
        }
        let mut state = storage::read_funding(&env, pair_index);
        state.fee_per_ledger_p = rate_p;
        storage::write_funding(&env, pair_index, &state);
        env.events()
            .publish((Symbol::new(&env, "funding_rate"), pair_index), rate_p);
        Ok(())
    }

    pub fn set_one_percent_depth(
        env: Env,
        pair_index: u32,
        depth_usdc: i128,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        if depth_usdc < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        if !storage::has_pair(&env, pair_index) {
            return Err(PairRegistryError::PairNotFound);
        }
        storage::write_depth(&env, pair_index, depth_usdc);
        env.events().publish(
            (Symbol::new(&env, "depth"), pair_index),
            depth_usdc,
        );
        Ok(())
    }

    // ─────────────── admin: group config ───────────────

    pub fn add_group(
        env: Env,
        group_index: u32,
        group: Group,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        validate_group(&group)?;
        if storage::has_group(&env, group_index) {
            return Err(PairRegistryError::GroupAlreadyExists);
        }
        let name = group.name.clone();
        storage::write_group(&env, group_index, &group);
        env.events()
            .publish((Symbol::new(&env, "group_added"), group_index), name);
        Ok(())
    }

    pub fn update_group(
        env: Env,
        group_index: u32,
        new_group: Group,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        validate_group(&new_group)?;
        if !storage::has_group(&env, group_index) {
            return Err(PairRegistryError::GroupNotFound);
        }
        storage::write_group(&env, group_index, &new_group);
        env.events()
            .publish((Symbol::new(&env, "group_updated"), group_index), ());
        Ok(())
    }

    pub fn set_group_open_fee_p(
        env: Env,
        group_index: u32,
        fee_p: i128,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        if fee_p < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        let mut group = storage::read_group(&env, group_index)?;
        group.open_fee_p = fee_p;
        storage::write_group(&env, group_index, &group);
        env.events()
            .publish((Symbol::new(&env, "open_fee"), group_index), fee_p);
        Ok(())
    }

    pub fn set_group_close_fee_p(
        env: Env,
        group_index: u32,
        fee_p: i128,
    ) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        if fee_p < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        let mut group = storage::read_group(&env, group_index)?;
        group.close_fee_p = fee_p;
        storage::write_group(&env, group_index, &group);
        env.events()
            .publish((Symbol::new(&env, "close_fee"), group_index), fee_p);
        Ok(())
    }

    pub fn set_max_pos_usdc(env: Env, value: i128) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        if value < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        storage::write_max_pos_usdc(&env, value);
        env.events()
            .publish((Symbol::new(&env, "max_pos"),), value);
        Ok(())
    }

    // ─────────────── admin: upgrade ───────────────

    /// Hot-swap the contract's wasm. Admin uploads a new wasm hash off-chain
    /// (e.g. via `stellar contract upload`) — this entry point points the
    /// running contract at it, allowing functions to be added or removed in
    /// future versions without changing the contract address.
    ///
    /// Storage layout MUST stay backwards-compatible with existing entries.
    /// Adding new `DataKey` variants is safe; renaming or repurposing them is not.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), PairRegistryError> {
        require_admin(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());
        env.events()
            .publish((Symbol::new(&env, "upgraded"),), new_wasm_hash);
        Ok(())
    }

    // ─────────────── views ───────────────

    pub fn get_pair(env: Env, pair_index: u32) -> Result<PairInfo, PairRegistryError> {
        storage::read_pair(&env, pair_index)
    }

    pub fn pairs_count(env: Env) -> u32 {
        storage::read_pairs_count(&env)
    }

    pub fn get_group(env: Env, group_index: u32) -> Result<Group, PairRegistryError> {
        storage::read_group(&env, group_index)
    }

    pub fn max_pos_usdc(env: Env) -> i128 {
        storage::read_max_pos_usdc(&env)
    }

    pub fn admin(env: Env) -> Result<Address, PairRegistryError> {
        storage::read_admin(&env)
    }

    pub fn position_manager(env: Env) -> Result<Address, PairRegistryError> {
        storage::read_position_manager(&env)
    }

    pub fn get_acc_rollover(env: Env, pair_index: u32) -> RolloverState {
        storage::read_rollover(&env, pair_index)
    }

    pub fn get_acc_funding(env: Env, pair_index: u32) -> FundingState {
        storage::read_funding(&env, pair_index)
    }

    pub fn get_oi(env: Env, pair_index: u32) -> PairOi {
        storage::read_oi(&env, pair_index)
    }

    pub fn get_depth(env: Env, pair_index: u32) -> i128 {
        storage::read_depth(&env, pair_index)
    }

    /// Compute pending rollover acc value at `at_ledger` without committing.
    pub fn pending_acc_rollover_view(
        env: Env,
        pair_index: u32,
        at_ledger: u32,
    ) -> Result<i128, PairRegistryError> {
        let state = storage::read_rollover(&env, pair_index);
        if at_ledger <= state.last_update_ledger {
            return Ok(state.acc_per_collateral);
        }
        let delta = (at_ledger - state.last_update_ledger) as u64;
        map_math_err(pending_acc_rollover(
            state.acc_per_collateral,
            delta,
            state.fee_per_ledger_p,
            USDC_SCALE,
        ))
    }

    /// Compute pending funding accs at `at_ledger` without committing.
    pub fn pending_acc_funding_view(
        env: Env,
        pair_index: u32,
        at_ledger: u32,
    ) -> Result<(i128, i128), PairRegistryError> {
        let state = storage::read_funding(&env, pair_index);
        let oi = storage::read_oi(&env, pair_index);
        if at_ledger <= state.last_update_ledger {
            return Ok((state.acc_long, state.acc_short));
        }
        let delta = (at_ledger - state.last_update_ledger) as u64;
        map_math_err(pending_acc_funding(
            state.acc_long,
            state.acc_short,
            oi.long,
            oi.short,
            delta,
            state.fee_per_ledger_p,
            USDC_SCALE,
        ))
    }

    /// Liquidation price using `trade.acc_*_open` snapshots and accumulators
    /// projected to `at_ledger`.
    pub fn get_trade_liquidation_price(
        env: Env,
        trade: TradeMeta,
        at_ledger: u32,
    ) -> Result<i128, PairRegistryError> {
        let pair = storage::read_pair(&env, trade.pair_index)?;
        let acc_rollover_now =
            Self::pending_acc_rollover_view(env.clone(), trade.pair_index, at_ledger)?;
        let (acc_long_now, acc_short_now) =
            Self::pending_acc_funding_view(env.clone(), trade.pair_index, at_ledger)?;
        let acc_funding_now = if trade.is_long { acc_long_now } else { acc_short_now };

        let rollover_fee = map_math_err(rollover_fee_for_trade(
            trade.acc_rollover_open,
            acc_rollover_now,
            trade.collateral,
            USDC_SCALE,
        ))?;
        let funding_fee = map_math_err(funding_fee_for_trade(
            trade.acc_funding_open,
            acc_funding_now,
            trade.collateral,
            trade.leverage,
            USDC_SCALE,
        ))?;
        map_math_err(liquidation_price(
            trade.open_price,
            trade.is_long,
            trade.collateral,
            trade.leverage,
            rollover_fee,
            funding_fee,
            pair.liq_threshold_p,
        ))
    }

    /// Whether `observed_price` (PRICE_SCALE) crosses this trade's liq price
    /// projected to `at_ledger`.
    pub fn is_liquidatable_view(
        env: Env,
        trade: TradeMeta,
        observed_price: i128,
        at_ledger: u32,
    ) -> Result<bool, PairRegistryError> {
        let liq =
            Self::get_trade_liquidation_price(env, trade.clone(), at_ledger)?;
        Ok(is_liquidatable(observed_price, liq, trade.is_long))
    }

    // ─────────────── mutators (PositionManager only) ───────────────

    /// Commit pending rollover accumulator up to `at_ledger`. Returns the new
    /// accumulator value.
    pub fn commit_acc_rollover(
        env: Env,
        pair_index: u32,
        at_ledger: u32,
    ) -> Result<i128, PairRegistryError> {
        require_position_manager(&env)?;
        if !storage::has_pair(&env, pair_index) {
            return Err(PairRegistryError::PairNotFound);
        }
        let mut state = storage::read_rollover(&env, pair_index);
        let delta_u32 = delta_ledgers_to(state.last_update_ledger, at_ledger)?;
        if delta_u32 == 0 {
            return Ok(state.acc_per_collateral);
        }
        let new_acc = map_math_err(pending_acc_rollover(
            state.acc_per_collateral,
            delta_u32 as u64,
            state.fee_per_ledger_p,
            USDC_SCALE,
        ))?;
        state.acc_per_collateral = new_acc;
        state.last_update_ledger = at_ledger;
        storage::write_rollover(&env, pair_index, &state);
        Ok(new_acc)
    }

    /// Commit pending funding accumulators up to `at_ledger`. Returns
    /// `(acc_long, acc_short)`.
    pub fn commit_acc_funding(
        env: Env,
        pair_index: u32,
        at_ledger: u32,
    ) -> Result<(i128, i128), PairRegistryError> {
        require_position_manager(&env)?;
        if !storage::has_pair(&env, pair_index) {
            return Err(PairRegistryError::PairNotFound);
        }
        let mut state = storage::read_funding(&env, pair_index);
        let oi = storage::read_oi(&env, pair_index);
        let delta_u32 = delta_ledgers_to(state.last_update_ledger, at_ledger)?;
        if delta_u32 == 0 {
            return Ok((state.acc_long, state.acc_short));
        }
        let (new_long, new_short) = map_math_err(pending_acc_funding(
            state.acc_long,
            state.acc_short,
            oi.long,
            oi.short,
            delta_u32 as u64,
            state.fee_per_ledger_p,
            USDC_SCALE,
        ))?;
        state.acc_long = new_long;
        state.acc_short = new_short;
        state.last_update_ledger = at_ledger;
        storage::write_funding(&env, pair_index, &state);
        Ok((new_long, new_short))
    }

    pub fn add_oi(
        env: Env,
        pair_index: u32,
        is_long: bool,
        delta_usdc: i128,
    ) -> Result<PairOi, PairRegistryError> {
        require_position_manager(&env)?;
        if delta_usdc < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        let mut oi = storage::read_oi(&env, pair_index);
        if is_long {
            oi.long = oi.long.checked_add(delta_usdc).ok_or(PairRegistryError::MathFault)?;
        } else {
            oi.short = oi.short.checked_add(delta_usdc).ok_or(PairRegistryError::MathFault)?;
        }
        storage::write_oi(&env, pair_index, &oi);
        env.events().publish(
            (Symbol::new(&env, "oi_add"), pair_index, is_long),
            delta_usdc,
        );
        Ok(oi)
    }

    pub fn sub_oi(
        env: Env,
        pair_index: u32,
        is_long: bool,
        delta_usdc: i128,
    ) -> Result<PairOi, PairRegistryError> {
        require_position_manager(&env)?;
        if delta_usdc < 0 {
            return Err(PairRegistryError::InvalidParam);
        }
        let mut oi = storage::read_oi(&env, pair_index);
        if is_long {
            oi.long = oi.long.checked_sub(delta_usdc).ok_or(PairRegistryError::MathFault)?;
            if oi.long < 0 {
                oi.long = 0;
            }
        } else {
            oi.short = oi.short.checked_sub(delta_usdc).ok_or(PairRegistryError::MathFault)?;
            if oi.short < 0 {
                oi.short = 0;
            }
        }
        storage::write_oi(&env, pair_index, &oi);
        env.events().publish(
            (Symbol::new(&env, "oi_sub"), pair_index, is_long),
            delta_usdc,
        );
        Ok(oi)
    }
}

// Suppress dead_code for the not-yet-used DataKey variants when only certain
// entry points reference them in tests.
#[allow(dead_code)]
fn _datakey_uses(_k: DataKey) {}
