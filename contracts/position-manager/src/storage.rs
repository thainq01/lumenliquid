use soroban_sdk::{Address, Env};

use crate::types::{DataKey, LimitOrder, Trade};

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn write_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

pub fn write_vault(env: &Env, vault: &Address) {
    env.storage().instance().set(&DataKey::Vault, vault);
}

pub fn read_vault(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Vault).unwrap()
}

pub fn write_pair_registry(env: &Env, registry: &Address) {
    env.storage().instance().set(&DataKey::PairRegistry, registry);
}

pub fn read_pair_registry(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::PairRegistry).unwrap()
}

pub fn write_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

pub fn read_paused(env: &Env) -> bool {
    env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
}

pub fn write_max_trades_per_pair(env: &Env, max_trades: u32) {
    env.storage().instance().set(&DataKey::MaxTradesPerPair, &max_trades);
}

pub fn read_max_trades_per_pair(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::MaxTradesPerPair).unwrap_or(3)
}

// ---------------- Trades ----------------

pub fn read_trades_count(env: &Env, trader: &Address, pair_index: u32) -> u32 {
    let key = DataKey::TradesCount(trader.clone(), pair_index);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn write_trades_count(env: &Env, trader: &Address, pair_index: u32, count: u32) {
    let key = DataKey::TradesCount(trader.clone(), pair_index);
    env.storage().persistent().set(&key, &count);
}

pub fn read_trade(env: &Env, trader: &Address, pair_index: u32, trade_index: u32) -> Option<Trade> {
    let key = DataKey::Trade(trader.clone(), pair_index, trade_index);
    env.storage().persistent().get(&key)
}

pub fn write_trade(env: &Env, trader: &Address, pair_index: u32, trade_index: u32, trade: &Trade) {
    let key = DataKey::Trade(trader.clone(), pair_index, trade_index);
    env.storage().persistent().set(&key, trade);
}

pub fn remove_trade(env: &Env, trader: &Address, pair_index: u32, trade_index: u32) {
    let key = DataKey::Trade(trader.clone(), pair_index, trade_index);
    env.storage().persistent().remove(&key);
}

// ---------------- Limit Orders ----------------

pub fn read_limits_count(env: &Env, trader: &Address, pair_index: u32) -> u32 {
    let key = DataKey::LimitsCount(trader.clone(), pair_index);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn write_limits_count(env: &Env, trader: &Address, pair_index: u32, count: u32) {
    let key = DataKey::LimitsCount(trader.clone(), pair_index);
    env.storage().persistent().set(&key, &count);
}

pub fn read_limit_order(env: &Env, trader: &Address, pair_index: u32, limit_index: u32) -> Option<LimitOrder> {
    let key = DataKey::LimitOrder(trader.clone(), pair_index, limit_index);
    env.storage().persistent().get(&key)
}

pub fn write_limit_order(env: &Env, trader: &Address, pair_index: u32, limit_index: u32, order: &LimitOrder) {
    let key = DataKey::LimitOrder(trader.clone(), pair_index, limit_index);
    env.storage().persistent().set(&key, order);
}

pub fn remove_limit_order(env: &Env, trader: &Address, pair_index: u32, limit_index: u32) {
    let key = DataKey::LimitOrder(trader.clone(), pair_index, limit_index);
    env.storage().persistent().remove(&key);
}
