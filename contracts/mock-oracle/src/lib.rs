#![no_std]
use soroban_sdk::{contract, contractimpl, Env};
use reflector_adapter::{ReflectorAsset, ReflectorPriceData};

#[contract]
pub struct MockOracleContract;

#[contractimpl]
impl MockOracleContract {
    pub fn lastprice(env: Env, asset: ReflectorAsset) -> Option<ReflectorPriceData> {
        let price = env.storage().instance().get(&asset).unwrap_or(50_000_000_000_000_000_000i128);
        Some(ReflectorPriceData { price, timestamp: env.ledger().timestamp() })
    }
    
    pub fn decimals(_env: Env) -> u32 {
        14
    }
    
    pub fn set_price(env: Env, asset: ReflectorAsset, price: i128) {
        env.storage().instance().set(&asset, &price);
    }
}
