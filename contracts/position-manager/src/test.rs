#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::Client as TokenClient,
    Address, Env, Symbol,
};

use crate::{PositionManagerContract, PositionManagerContractClient};
use pair_registry::{Group, PairInfo, PairRegistryContract, PairRegistryContractClient};
use vault::{VaultContract, VaultContractClient};
use reflector_adapter::{ReflectorAsset, mock::MockOracle, PriceObservation};
use math::scale::PRICE_SCALE;

// --- Setup Helper ---

struct Setup {
    env: Env,
    admin: Address,
    trader: Address,
    usdc: TokenClient<'static>,
    vault: VaultContractClient<'static>,
    registry: PairRegistryContractClient<'static>,
    pm: PositionManagerContractClient<'static>,
    oracle: Address, // Mock Oracle
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let trader = Address::generate(&env);

        // Deploy USDC Mock (Using SAC)
        let usdc_admin = Address::generate(&env);
        let usdc_addr = env.register_stellar_asset_contract_v2(usdc_admin.clone()).address();
        let usdc = TokenClient::new(&env, &usdc_addr);

        // Register all contracts first
        let registry_id = env.register(PairRegistryContract, ());
        let vault_id = env.register(VaultContract, ());
        let pm_id = env.register(PositionManagerContract, ());
        let oracle_id = env.register(MockOracleContract, ());

        let registry = PairRegistryContractClient::new(&env, &registry_id);
        let vault = VaultContractClient::new(&env, &vault_id);
        let pm = PositionManagerContractClient::new(&env, &pm_id);

        // Initialize them
        registry.init(&admin, &pm_id, &1_000_000_0000000);
        vault.init(&admin, &admin, &usdc_addr, &0);
        pm.init(&admin, &vault_id, &registry_id, &oracle_id);
        vault.set_position_manager(&pm_id);

        // Mint some USDC
        use soroban_sdk::token::StellarAssetClient;
        let usdc_admin_client = StellarAssetClient::new(&env, &usdc_addr);
        usdc_admin_client.mint(&trader, &100_000_0000000); // 100k USDC
        usdc_admin_client.mint(&admin, &100_000_0000000);  // 100k USDC

        Self {
            env,
            admin,
            trader,
            usdc,
            vault,
            registry,
            pm,
            oracle: oracle_id,
        }
    }
}

// --- Mock Oracle Contract ---
use reflector_adapter::ReflectorPriceData;

#[soroban_sdk::contract]
pub struct MockOracleContract;

#[soroban_sdk::contractimpl]
impl MockOracleContract {
    pub fn lastprice(env: Env, asset: ReflectorAsset) -> Option<ReflectorPriceData> {
        // Default to BTC at 50k
        let price = env.storage().instance().get(&asset).unwrap_or(5_000_000_000_000_000_000i128);
        Some(ReflectorPriceData { price, timestamp: 12345 })
    }
    pub fn decimals(_env: Env) -> u32 {
        14
    }
    pub fn set_price(env: Env, asset: ReflectorAsset, price: i128) {
        env.storage().instance().set(&asset, &price);
    }
}

// --- Tests ---

#[test]
fn test_scenario_open_and_close_market() {
    let mut setup = Setup::new();
    
    // Add Group
    let group = Group {
        name: Symbol::new(&setup.env, "crypto"),
        max_collateral_usdc: 1_000_000_0000000,
        open_fee_p: 800_000, // 0.008% (8e5 / 1e10)
        close_fee_p: 800_000,
    };
    setup.registry.add_group(&0, &group);

    // Add Pair
    let pair = PairInfo {
        symbol: Symbol::new(&setup.env, "BTC"),
        reflector_asset: ReflectorAsset::Other(Symbol::new(&setup.env, "BTC")),
        group_index: 0,
        spread_p: 0,
        min_leverage: 1,
        max_leverage: 100,
        min_lev_pos_usdc: 10_0000000,
        max_oi_usdc: 500_000_0000000,
        max_neg_pnl_p: 90_0000000,
        liq_threshold_p: 90,
        max_gain_p: 900,
        disabled: false,
    };
    setup.registry.add_pair(&0, &pair);

    // LP Deposit to Vault
    setup.usdc.mock_all_auths();
    setup.vault.deposit(&setup.admin, &100_000_0000000, &setup.admin);
    assert_eq!(setup.usdc.balance(&setup.vault.address), 100_000_0000000);

    // Trader opens Market Long BTC: 1000 USDC, 10x
    let collateral = 1_000_0000000;
    setup.pm.open_market_trade(&setup.trader, &0, &true, &collateral, &10);

    // Verify trade
    // 1000 USDC * 10x = 10000 USDC notional. Fee = 10000 * 0.00008 = 0.8 USDC = 8_000000
    // Effective collateral = 1000 - 0.8 = 999.2 USDC
    let usdc_balance_pm = setup.usdc.balance(&setup.pm.address);
    assert_eq!(usdc_balance_pm, collateral);

    // Trader closes trade (Wait, mock oracle always returns 50k for now, so PNL = 0)
    setup.pm.close_market_trade(&setup.trader, &0, &0);
    
    // Close fee: Notional is 999.2 * 10 = 9992. 
    // Fee = 9992 * 0.00008 = 0.79936 USDC = 7993600
    // Trader receives 999.2 - 0.79936 = 998.40064 USDC back = 9984006400
    let final_trader_bal = setup.usdc.balance(&setup.trader);
    assert_eq!(final_trader_bal, 100_000_0000000 - 1_000_0000000 + 998_4006400);
}

#[test]
fn test_scenario_limit_order() {
    let setup = Setup::new();
    
    let group = Group {
        name: Symbol::new(&setup.env, "crypto"),
        max_collateral_usdc: 1_000_000_0000000,
        open_fee_p: 800_000,
        close_fee_p: 800_000,
    };
    setup.registry.add_group(&0, &group);

    let pair = PairInfo {
        symbol: Symbol::new(&setup.env, "BTC"),
        reflector_asset: ReflectorAsset::Other(Symbol::new(&setup.env, "BTC")),
        group_index: 0,
        spread_p: 0,
        min_leverage: 1,
        max_leverage: 100,
        min_lev_pos_usdc: 10_0000000,
        max_oi_usdc: 500_000_0000000,
        max_neg_pnl_p: 90_0000000,
        liq_threshold_p: 90,
        max_gain_p: 900,
        disabled: false,
    };
    setup.registry.add_pair(&0, &pair);

    let collateral = 1_000_0000000;
    
    // Limit order at $49,000 for a LONG (current price is 50,000)
    // limit_price is at PRICE_SCALE (1e10)
    let limit_price = 49_000 * 10i128.pow(10);
    
    // Place Limit Order
    setup.pm.place_limit_order(&setup.trader, &0, &true, &collateral, &10, &limit_price);
    
    assert_eq!(setup.usdc.balance(&setup.trader), 100_000_0000000 - 1_000_0000000);
    assert_eq!(setup.usdc.balance(&setup.pm.address), 1_000_0000000);

    // Try to execute (Mock oracle price is 50,000)
    // Long limit executes if current_price <= limit_price.
    // 50,000 > 49,000 so execution should fail with PriceMismatch
    let res = setup.pm.try_execute_limit_order(&setup.trader, &0, &0);
    assert_eq!(res.unwrap_err(), Ok(crate::errors::PositionManagerError::PriceMismatch));

    // Update Oracle price to 48,000
    let oracle_client = MockOracleContractClient::new(&setup.env, &setup.oracle);
    let new_price = 48_000 * 10i128.pow(14);
    oracle_client.set_price(&ReflectorAsset::Other(Symbol::new(&setup.env, "BTC")), &new_price);

    // Execute should succeed now
    setup.pm.execute_limit_order(&setup.trader, &0, &0);
    
    // Balance is still same, but PM has the trade
    assert_eq!(setup.usdc.balance(&setup.pm.address), 1_000_0000000);
}

#[test]
fn test_scenario_liquidation() {
    let setup = Setup::new();
    
    let group = Group {
        name: Symbol::new(&setup.env, "crypto"),
        max_collateral_usdc: 1_000_000_0000000,
        open_fee_p: 800_000,
        close_fee_p: 800_000,
    };
    setup.registry.add_group(&0, &group);

    let pair = PairInfo {
        symbol: Symbol::new(&setup.env, "BTC"),
        reflector_asset: ReflectorAsset::Other(Symbol::new(&setup.env, "BTC")),
        group_index: 0,
        spread_p: 0,
        min_leverage: 1,
        max_leverage: 100,
        min_lev_pos_usdc: 10_0000000,
        max_oi_usdc: 500_000_0000000,
        max_neg_pnl_p: 90_0000000, // max loss 90%
        liq_threshold_p: 90, // liquidate at 90% loss
        max_gain_p: 900,
        disabled: false,
    };
    setup.registry.add_pair(&0, &pair);

    setup.usdc.mock_all_auths();
    setup.vault.deposit(&setup.admin, &100_000_0000000, &setup.admin);

    let collateral = 1_000_0000000;
    setup.pm.open_market_trade(&setup.trader, &0, &true, &collateral, &10);

    // Open price is 50,000. Leverage is 10x.
    // 90% loss means price drops by 9%. 50,000 * 0.91 = 45,500
    // Try liquidating at 46,000 (Loss = 80%) -> Should fail
    let oracle_client = MockOracleContractClient::new(&setup.env, &setup.oracle);
    
    let price_80_loss = 46_000 * 10i128.pow(14);
    oracle_client.set_price(&ReflectorAsset::Other(Symbol::new(&setup.env, "BTC")), &price_80_loss);
    
    let res = setup.pm.try_liquidate_trade(&setup.trader, &0, &0);
    assert_eq!(res.unwrap_err(), Ok(crate::errors::PositionManagerError::NotLiquidatable));

    // Liquidate at 45,000 (Loss = 100% > 90%) -> Should succeed
    let price_100_loss = 45_000 * 10i128.pow(14);
    oracle_client.set_price(&ReflectorAsset::Other(Symbol::new(&setup.env, "BTC")), &price_100_loss);
    
    setup.pm.liquidate_trade(&setup.trader, &0, &0);
    
    // Vault balance should have increased by effective collateral
    // initial: 100_000_0000000
    // effective collateral = 1000 - 0.8 = 999.2
    assert_eq!(setup.usdc.balance(&setup.vault.address), 100_000_0000000 + 999_2000000);
}
