//! Typed accessors over Soroban storage. Keeps the contract entry points free
//! of `storage().instance().get(...)` boilerplate and consolidates the
//! "instance vs persistent" decision in one place.
//!
//! Storage layout:
//! * **Instance**: admin, position_manager, max_pos_usdc, pairs_count.
//!   These are touched by almost every entry point — keeping them in
//!   instance storage means one bumped read.
//! * **Persistent**: PairInfo, Group, RolloverState, FundingState, PairOi,
//!   per-pair depth. Sized by number of pairs/groups, lifetimes bumped by
//!   the contract's TTL extension policy (added in Phase 2).

use soroban_sdk::{Address, Env};

use crate::errors::PairRegistryError;
use crate::types::{DataKey, FundingState, Group, PairInfo, PairOi, RolloverState};

// ───────────────────────── instance ─────────────────────────

pub fn read_admin(env: &Env) -> Result<Address, PairRegistryError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(PairRegistryError::NotInitialized)
}

pub fn write_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_position_manager(env: &Env) -> Result<Address, PairRegistryError> {
    env.storage()
        .instance()
        .get(&DataKey::PositionManager)
        .ok_or(PairRegistryError::NotInitialized)
}

pub fn write_position_manager(env: &Env, pm: &Address) {
    env.storage().instance().set(&DataKey::PositionManager, pm);
}

pub fn read_max_pos_usdc(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MaxPosUsdc)
        .unwrap_or(0)
}

pub fn write_max_pos_usdc(env: &Env, value: i128) {
    env.storage().instance().set(&DataKey::MaxPosUsdc, &value);
}

pub fn read_pairs_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::PairsCount)
        .unwrap_or(0)
}

pub fn write_pairs_count(env: &Env, value: u32) {
    env.storage().instance().set(&DataKey::PairsCount, &value);
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

// ───────────────────────── persistent: pair / group ─────────────────────────

pub fn has_pair(env: &Env, pair_index: u32) -> bool {
    env.storage().persistent().has(&DataKey::Pair(pair_index))
}

pub fn read_pair(env: &Env, pair_index: u32) -> Result<PairInfo, PairRegistryError> {
    env.storage()
        .persistent()
        .get(&DataKey::Pair(pair_index))
        .ok_or(PairRegistryError::PairNotFound)
}

pub fn write_pair(env: &Env, pair_index: u32, pair: &PairInfo) {
    env.storage()
        .persistent()
        .set(&DataKey::Pair(pair_index), pair);
}

pub fn has_group(env: &Env, group_index: u32) -> bool {
    env.storage().persistent().has(&DataKey::Group(group_index))
}

pub fn read_group(env: &Env, group_index: u32) -> Result<Group, PairRegistryError> {
    env.storage()
        .persistent()
        .get(&DataKey::Group(group_index))
        .ok_or(PairRegistryError::GroupNotFound)
}

pub fn write_group(env: &Env, group_index: u32, group: &Group) {
    env.storage()
        .persistent()
        .set(&DataKey::Group(group_index), group);
}

// ───────────────────────── persistent: accumulators ─────────────────────────

pub fn read_rollover(env: &Env, pair_index: u32) -> RolloverState {
    env.storage()
        .persistent()
        .get(&DataKey::Rollover(pair_index))
        .unwrap_or_default()
}

pub fn write_rollover(env: &Env, pair_index: u32, state: &RolloverState) {
    env.storage()
        .persistent()
        .set(&DataKey::Rollover(pair_index), state);
}

pub fn read_funding(env: &Env, pair_index: u32) -> FundingState {
    env.storage()
        .persistent()
        .get(&DataKey::Funding(pair_index))
        .unwrap_or_default()
}

pub fn write_funding(env: &Env, pair_index: u32, state: &FundingState) {
    env.storage()
        .persistent()
        .set(&DataKey::Funding(pair_index), state);
}

pub fn read_oi(env: &Env, pair_index: u32) -> PairOi {
    env.storage()
        .persistent()
        .get(&DataKey::OI(pair_index))
        .unwrap_or_default()
}

pub fn write_oi(env: &Env, pair_index: u32, oi: &PairOi) {
    env.storage().persistent().set(&DataKey::OI(pair_index), oi);
}

pub fn read_depth(env: &Env, pair_index: u32) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Depth(pair_index))
        .unwrap_or(0)
}

pub fn write_depth(env: &Env, pair_index: u32, value: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Depth(pair_index), &value);
}
