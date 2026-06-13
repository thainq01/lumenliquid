use soroban_sdk::{contracttype, Address, Symbol};
use reflector_adapter::ReflectorAsset;

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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    pub name: Symbol,
    pub max_collateral_usdc: i128,
    pub open_fee_p: i128,
    pub close_fee_p: i128,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct PairOi {
    pub long: i128,
    pub short: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trade {
    pub pair_index: u32,
    pub is_long: bool,
    pub leverage: u32,
    pub open_price: i128,
    pub collateral: i128, // Effective collateral (post-fee)
    pub acc_rollover_open: i128,
    pub acc_funding_open: i128,
    pub tp_price: i128,
    pub sl_price: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitOrder {
    pub pair_index: u32,
    pub is_long: bool,
    pub collateral: i128, // Raw collateral (before fee)
    pub leverage: u32,
    pub limit_price: i128,
    pub tp_price: i128,
    pub sl_price: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Vault,
    PairRegistry,
    ReflectorContract,
    Paused,
    MaxTradesPerPair,
    TradesCount(Address, u32), // (trader, pair_index) -> u32
    Trade(Address, u32, u32),  // (trader, pair_index, trade_index) -> Trade
    LimitsCount(Address, u32), // (trader, pair_index) -> u32
    LimitOrder(Address, u32, u32), // (trader, pair_index, limit_index) -> LimitOrder
}
