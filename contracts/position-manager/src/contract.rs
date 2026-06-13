use soroban_sdk::{contract, contractimpl, token::TokenClient, Address, Env, Symbol};

use math::{
    fees::current_percent_profit,
    liq::{is_liquidatable, liquidation_price, settlement_payout},
    scale::P_SCALE,
};
use reflector_adapter::{OracleSource, ReflectorOracle};

use crate::errors::PositionManagerError;
use crate::storage;
use crate::types::{DataKey, LimitOrder, Trade, PairInfo, Group, PairOi};

use soroban_sdk::contractclient;

#[contractclient(name = "PairRegistryClient")]
pub trait PairRegistryTrait {
    fn get_pair(env: Env, pair_index: u32) -> PairInfo;
    fn get_group(env: Env, group_index: u32) -> Group;
    fn add_oi(env: Env, pair_index: u32, is_long: bool, delta_usdc: i128) -> PairOi;
    fn sub_oi(env: Env, pair_index: u32, is_long: bool, delta_usdc: i128) -> PairOi;
}

#[contractclient(name = "VaultClient")]
pub trait VaultTrait {
    fn return_collateral_with_pnl(env: Env, trader: Address, effective_collateral: i128, net_pnl_for_vault: i128) -> i128;
    fn usdc_token(env: Env) -> Address;
}

#[contract]
pub struct PositionManagerContract;

// ---------------- Helpers ----------------

fn require_not_paused(env: &Env) -> Result<(), PositionManagerError> {
    if storage::read_paused(env) {
        return Err(PositionManagerError::Paused);
    }
    Ok(())
}

fn get_oracle(env: &Env) -> ReflectorOracle {
    let addr: Address = env.storage().instance().get(&DataKey::ReflectorContract).unwrap();
    ReflectorOracle::new(addr)
}

fn get_vault_client(env: &Env) -> VaultClient<'static> {
    VaultClient::new(env, &storage::read_vault(env))
}

fn get_registry_client(env: &Env) -> PairRegistryClient<'static> {
    PairRegistryClient::new(env, &storage::read_pair_registry(env))
}

fn get_usdc_client(env: &Env) -> TokenClient<'static> {
    let vault = get_vault_client(env);
    TokenClient::new(env, &vault.usdc_token())
}

#[contractimpl]
impl PositionManagerContract {
    pub fn init(
        env: Env,
        admin: Address,
        vault: Address,
        pair_registry: Address,
        reflector_contract: Address,
    ) -> Result<(), PositionManagerError> {
        if storage::is_initialized(&env) {
            return Err(PositionManagerError::AlreadyInitialized);
        }
        admin.require_auth();
        storage::write_admin(&env, &admin);
        storage::write_vault(&env, &vault);
        storage::write_pair_registry(&env, &pair_registry);
        env.storage()
            .instance()
            .set(&DataKey::ReflectorContract, &reflector_contract);
        storage::write_paused(&env, false);
        Ok(())
    }

    pub fn set_reflector_contract(
        env: Env,
        reflector_contract: Address,
    ) -> Result<(), PositionManagerError> {
        let admin = storage::read_admin(&env);
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ReflectorContract, &reflector_contract);
        Ok(())
    }

    // Lấy thông tin Trade (View)
    pub fn get_trade(env: Env, trader: Address, pair_index: u32, trade_index: u32) -> Trade {
        storage::read_trade(&env, &trader, pair_index, trade_index).expect("Trade not found")
    }

    // Tính toán PnL hiện tại của Trade (View)
    pub fn get_trade_pnl(env: Env, trader: Address, pair_index: u32, trade_index: u32) -> i128 {
        let trade = storage::read_trade(&env, &trader, pair_index, trade_index).expect("Trade not found");
        let registry = get_registry_client(&env);
        let pair = registry.get_pair(&trade.pair_index);
        
        let oracle = get_oracle(&env);
        let current_price = oracle.read_price(&env, &pair.reflector_asset).expect("No oracle price").price;

        let pnl_p = current_percent_profit(
            trade.open_price,
            current_price,
            trade.is_long,
            trade.leverage,
            pair.max_gain_p,
        ).unwrap();

        let pnl = (trade.collateral * pnl_p) / P_SCALE / 100;
        pnl
    }

    // MVP: Open Market Trade
    pub fn open_market_trade(
        env: Env,
        trader: Address,
        pair_index: u32,
        is_long: bool,
        collateral: i128,
        leverage: u32,
        tp_price: i128,
        sl_price: i128,
    ) -> Result<u32, PositionManagerError> {
        require_not_paused(&env)?;
        trader.require_auth();

        if collateral <= 0 || leverage == 0 {
            return Err(PositionManagerError::InvalidParam);
        }

        let registry = get_registry_client(&env);
        let pair = registry.get_pair(&pair_index);
        if pair.disabled {
            return Err(PositionManagerError::PairDisabled);
        }
        if leverage < pair.min_leverage || leverage > pair.max_leverage {
            return Err(PositionManagerError::LeverageIncorrect);
        }

        let oracle = get_oracle(&env);
        let obs = oracle
            .read_price(&env, &pair.reflector_asset)
            .ok_or(PositionManagerError::OracleUnavailable)?;
        let open_price = obs.price;

        let group = registry.get_group(&pair.group_index);
        let notional = collateral.checked_mul(leverage as i128).unwrap();
        // Calculate open fee
        let open_fee = math::mul_div_floor(notional, group.open_fee_p, P_SCALE)
            .map_err(|_| PositionManagerError::MathFault)?;
        let effective_collateral = collateral - open_fee;
        if effective_collateral <= 0 {
            return Err(PositionManagerError::InvalidParam);
        }

        // Pull collateral
        let usdc = get_usdc_client(&env);
        usdc.transfer(&trader, &env.current_contract_address(), &collateral);

        // Save trade
        let max_trades = storage::read_max_trades_per_pair(&env);
        let mut assigned_index: Option<u32> = None;
        for i in 0..max_trades {
            if storage::read_trade(&env, &trader, pair_index, i).is_none() {
                assigned_index = Some(i);
                break;
            }
        }
        
        let trade_index = assigned_index.ok_or(PositionManagerError::MaxTradesReached)?;

        let trade = Trade {
            pair_index,
            is_long,
            leverage,
            open_price,
            collateral: effective_collateral,
            acc_rollover_open: 0,
            acc_funding_open: 0,
            tp_price,
            sl_price,
        };
        storage::write_trade(&env, &trader, pair_index, trade_index, &trade);

        // Update OI
        let effective_notional = effective_collateral.checked_mul(leverage as i128).unwrap();
        registry.add_oi(&pair_index, &is_long, &effective_notional);

        env.events()
            .publish((Symbol::new(&env, "opened"), trader), (trade_index, trade.clone()));

        Ok(trade_index)
    }

    // MVP: Close Market Trade
    pub fn close_market_trade(
        env: Env,
        trader: Address,
        pair_index: u32,
        trade_index: u32,
    ) -> Result<(), PositionManagerError> {
        trader.require_auth();
        let trade = storage::read_trade(&env, &trader, pair_index, trade_index)
            .ok_or(PositionManagerError::TradeNotFound)?;

        let registry = get_registry_client(&env);
        let pair = registry.get_pair(&pair_index);
        let group = registry.get_group(&pair.group_index);

        let oracle = get_oracle(&env);
        let obs = oracle
            .read_price(&env, &pair.reflector_asset)
            .ok_or(PositionManagerError::OracleUnavailable)?;
        let close_price = obs.price;

        let pnl_p = current_percent_profit(
            trade.open_price,
            close_price,
            trade.is_long,
            trade.leverage,
            pair.max_gain_p,
        )
        .map_err(|_| PositionManagerError::MathFault)?;

        let notional = trade.collateral.checked_mul(trade.leverage as i128).unwrap();
        let close_fee = math::mul_div_floor(notional, group.close_fee_p, P_SCALE)
            .map_err(|_| PositionManagerError::MathFault)?;

        let (gross_payout, close_fee_charged) = settlement_payout(
            trade.collateral,
            pnl_p, // pnl_p is already scaled correctly
            0,
            0,
            close_fee,
            pair.liq_threshold_p,
        )
        .map_err(|_| PositionManagerError::MathFault)?;

        // PM keeps close_fee_charged. Send the rest of collateral to Vault.
        let amount_from_pm = trade.collateral - close_fee_charged;
        let net_pnl_for_vault = gross_payout - amount_from_pm;

        // Vault Settlement
        let vault = get_vault_client(&env);
        let usdc = get_usdc_client(&env);
        // PositionManager transfers remaining collateral to Vault for settlement
        usdc.transfer(&env.current_contract_address(), &vault.address, &amount_from_pm);
        vault.return_collateral_with_pnl(&trader, &amount_from_pm, &net_pnl_for_vault);

        // Cleanup
        storage::remove_trade(&env, &trader, pair_index, trade_index);
        let effective_notional = trade.collateral.checked_mul(trade.leverage as i128).unwrap();
        registry.sub_oi(&pair_index, &trade.is_long, &effective_notional);

        env.events()
            .publish((Symbol::new(&env, "closed"), trader), trade_index);

        Ok(())
    }

    // MVP: Place Limit Order
    pub fn place_limit_order(
        env: Env,
        trader: Address,
        pair_index: u32,
        is_long: bool,
        collateral: i128,
        leverage: u32,
        limit_price: i128,
        tp_price: i128,
        sl_price: i128,
    ) -> Result<u32, PositionManagerError> {
        require_not_paused(&env)?;
        trader.require_auth();

        if collateral <= 0 || leverage == 0 || limit_price <= 0 {
            return Err(PositionManagerError::InvalidParam);
        }

        let registry = get_registry_client(&env);
        let pair = registry.get_pair(&pair_index);
        if pair.disabled {
            return Err(PositionManagerError::PairDisabled);
        }
        if leverage < pair.min_leverage || leverage > pair.max_leverage {
            return Err(PositionManagerError::LeverageIncorrect);
        }

        // Pull collateral
        let usdc = get_usdc_client(&env);
        usdc.transfer(&trader, &env.current_contract_address(), &collateral);

        let max_limits = storage::read_max_trades_per_pair(&env);
        let mut assigned_limit_index: Option<u32> = None;
        for i in 0..max_limits {
            if storage::read_limit_order(&env, &trader, pair_index, i).is_none() {
                assigned_limit_index = Some(i);
                break;
            }
        }
        let limit_index = assigned_limit_index.ok_or(PositionManagerError::MaxTradesReached)?;

        let order = LimitOrder {
            pair_index,
            is_long,
            collateral,
            leverage,
            limit_price,
            tp_price,
            sl_price,
        };
        storage::write_limit_order(&env, &trader, pair_index, limit_index, &order);

        env.events()
            .publish((Symbol::new(&env, "placed"), trader), limit_index);

        Ok(limit_index)
    }

    // MVP: Execute Limit Order (Keeper calls this)
    pub fn execute_limit_order(
        env: Env,
        trader: Address,
        pair_index: u32,
        limit_index: u32,
    ) -> Result<u32, PositionManagerError> {
        require_not_paused(&env)?;
        let order = storage::read_limit_order(&env, &trader, pair_index, limit_index)
            .ok_or(PositionManagerError::LimitNotFound)?;

        let registry = get_registry_client(&env);
        let pair = registry.get_pair(&pair_index);
        let group = registry.get_group(&pair.group_index);

        let oracle = get_oracle(&env);
        let obs = oracle
            .read_price(&env, &pair.reflector_asset)
            .ok_or(PositionManagerError::OracleUnavailable)?;

        // Check if limit price is matched
        if order.is_long {
            if obs.price > order.limit_price {
                return Err(PositionManagerError::PriceMismatch);
            }
        } else {
            if obs.price < order.limit_price {
                return Err(PositionManagerError::PriceMismatch);
            }
        }

        // Calculate fees & open
        let notional = order.collateral.checked_mul(order.leverage as i128).unwrap();
        let open_fee = math::mul_div_floor(notional, group.open_fee_p, P_SCALE)
            .map_err(|_| PositionManagerError::MathFault)?;
        let effective_collateral = order.collateral - open_fee;

        let max_trades = storage::read_max_trades_per_pair(&env);
        let mut assigned_index: Option<u32> = None;
        for i in 0..max_trades {
            if storage::read_trade(&env, &trader, pair_index, i).is_none() {
                assigned_index = Some(i);
                break;
            }
        }
        let trade_index = assigned_index.ok_or(PositionManagerError::MaxTradesReached)?;

        let trade = Trade {
            pair_index,
            is_long: order.is_long,
            leverage: order.leverage,
            open_price: obs.price, // Open at current market price (or limit price if preferred, sticking to market)
            collateral: effective_collateral,
            acc_rollover_open: 0,
            acc_funding_open: 0,
            tp_price: order.tp_price,
            sl_price: order.sl_price,
        };

        storage::remove_limit_order(&env, &trader, pair_index, limit_index);
        storage::write_trade(&env, &trader, pair_index, trade_index, &trade);

        let effective_notional = effective_collateral.checked_mul(order.leverage as i128).unwrap();
        registry.add_oi(&pair_index, &order.is_long, &effective_notional);

        env.events()
            .publish((Symbol::new(&env, "executed"), trader), trade_index);

        Ok(trade_index)
    }

    pub fn cancel_limit_order(
        env: Env,
        trader: Address,
        pair_index: u32,
        limit_index: u32,
    ) -> Result<(), PositionManagerError> {
        require_not_paused(&env)?;
        trader.require_auth();

        let order = storage::read_limit_order(&env, &trader, pair_index, limit_index)
            .ok_or(PositionManagerError::LimitNotFound)?;

        let usdc = get_usdc_client(&env);
        usdc.transfer(&env.current_contract_address(), &trader, &order.collateral);

        storage::remove_limit_order(&env, &trader, pair_index, limit_index);

        env.events()
            .publish((Symbol::new(&env, "canceled"), trader), limit_index);

        Ok(())
    }

    pub fn update_limit_order(
        env: Env,
        trader: Address,
        pair_index: u32,
        limit_index: u32,
        limit_price: i128,
        tp_price: i128,
        sl_price: i128,
    ) -> Result<(), PositionManagerError> {
        require_not_paused(&env)?;
        trader.require_auth();

        let mut order = storage::read_limit_order(&env, &trader, pair_index, limit_index)
            .ok_or(PositionManagerError::LimitNotFound)?;

        if limit_price <= 0 {
            return Err(PositionManagerError::InvalidParam);
        }

        order.limit_price = limit_price;
        order.tp_price = tp_price;
        order.sl_price = sl_price;

        storage::write_limit_order(&env, &trader, pair_index, limit_index, &order);

        env.events()
            .publish((Symbol::new(&env, "updated_limit"), trader), limit_index);

        Ok(())
    }

    // MVP: Liquidate Trade (Keeper calls this)
    pub fn liquidate_trade(
        env: Env,
        trader: Address,
        pair_index: u32,
        trade_index: u32,
    ) -> Result<(), PositionManagerError> {
        let trade = storage::read_trade(&env, &trader, pair_index, trade_index)
            .ok_or(PositionManagerError::TradeNotFound)?;

        let registry = get_registry_client(&env);
        let pair = registry.get_pair(&pair_index);

        let oracle = get_oracle(&env);
        let obs = oracle
            .read_price(&env, &pair.reflector_asset)
            .ok_or(PositionManagerError::OracleUnavailable)?;

        let liq_price = liquidation_price(
            trade.open_price,
            trade.is_long,
            trade.collateral,
            trade.leverage,
            0,
            0,
            pair.liq_threshold_p,
        )
        .map_err(|_| PositionManagerError::MathFault)?;

        if !is_liquidatable(obs.price, liq_price, trade.is_long) {
            return Err(PositionManagerError::NotLiquidatable);
        }

        // Vault Settlement: vault keeps all collateral
        let net_pnl_for_vault = -trade.collateral;
        let vault = get_vault_client(&env);
        let usdc = get_usdc_client(&env);
        usdc.transfer(&env.current_contract_address(), &vault.address, &trade.collateral);
        vault.return_collateral_with_pnl(&trader, &trade.collateral, &net_pnl_for_vault);

        storage::remove_trade(&env, &trader, pair_index, trade_index);
        let effective_notional = trade.collateral.checked_mul(trade.leverage as i128).unwrap();
        registry.sub_oi(&pair_index, &trade.is_long, &effective_notional);

        env.events()
            .publish((Symbol::new(&env, "liq"), trader), trade_index);

        Ok(())
    }

    pub fn update_tp_sl(
        env: Env,
        trader: Address,
        pair_index: u32,
        trade_index: u32,
        tp_price: i128,
        sl_price: i128,
    ) -> Result<(), PositionManagerError> {
        require_not_paused(&env)?;
        trader.require_auth();

        let mut trade = storage::read_trade(&env, &trader, pair_index, trade_index)
            .ok_or(PositionManagerError::TradeNotFound)?;

        trade.tp_price = tp_price;
        trade.sl_price = sl_price;

        storage::write_trade(&env, &trader, pair_index, trade_index, &trade);

        env.events()
            .publish((Symbol::new(&env, "updated_tp_sl"), trader), trade_index);

        Ok(())
    }

    pub fn execute_tp_sl(
        env: Env,
        keeper: Address,
        trader: Address,
        pair_index: u32,
        trade_index: u32,
    ) -> Result<(), PositionManagerError> {
        require_not_paused(&env)?;
        keeper.require_auth(); // Or restrict to a whitelisted keeper later

        let trade = storage::read_trade(&env, &trader, pair_index, trade_index)
            .ok_or(PositionManagerError::TradeNotFound)?;

        let registry = get_registry_client(&env);
        let pair = registry.get_pair(&pair_index);

        let oracle = get_oracle(&env);
        let obs = oracle
            .read_price(&env, &pair.reflector_asset)
            .ok_or(PositionManagerError::OracleUnavailable)?;

        let mut condition_met = false;
        if trade.is_long {
            if trade.tp_price > 0 && obs.price >= trade.tp_price {
                condition_met = true;
            }
            if trade.sl_price > 0 && obs.price <= trade.sl_price {
                condition_met = true;
            }
        } else {
            if trade.tp_price > 0 && obs.price <= trade.tp_price {
                condition_met = true;
            }
            if trade.sl_price > 0 && obs.price >= trade.sl_price {
                condition_met = true;
            }
        }

        if !condition_met {
            return Err(PositionManagerError::PriceMismatch);
        }

        // Just execute normal close market trade logic
        // We can call `close_market_trade_internal` or just duplicate the logic
        // To avoid code duplication, it's better to refactor, but for MVP we will just do it inline
        let group = registry.get_group(&pair.group_index);
        let pnl_p = current_percent_profit(
            trade.open_price,
            obs.price,
            trade.is_long,
            trade.leverage,
            pair.max_gain_p,
        ).unwrap();

        let notional = trade.collateral.checked_mul(trade.leverage as i128).unwrap();
        let close_fee = math::mul_div_floor(notional, group.close_fee_p, P_SCALE)
            .map_err(|_| PositionManagerError::MathFault)?;

        let (gross_payout, close_fee_charged) = settlement_payout(
            trade.collateral,
            pnl_p,
            0,
            0,
            close_fee,
            pair.liq_threshold_p,
        )
        .map_err(|_| PositionManagerError::MathFault)?;

        // PM keeps close_fee_charged. Send the rest of collateral to Vault.
        let amount_from_pm = trade.collateral - close_fee_charged;
        let net_pnl_for_vault = gross_payout - amount_from_pm;

        let vault = get_vault_client(&env);
        let usdc = get_usdc_client(&env);
        usdc.transfer(&env.current_contract_address(), &vault.address, &amount_from_pm);
        vault.return_collateral_with_pnl(&trader, &amount_from_pm, &net_pnl_for_vault);

        storage::remove_trade(&env, &trader, pair_index, trade_index);
        let effective_notional = trade.collateral.checked_mul(trade.leverage as i128).unwrap();
        registry.sub_oi(&pair_index, &trade.is_long, &effective_notional);

        env.events()
            .publish((Symbol::new(&env, "tp_sl_executed"), trader), trade_index);

        Ok(())
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) -> Result<(), PositionManagerError> {
        let admin = storage::read_admin(&env);
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    pub fn set_max_trades_per_pair(env: Env, max_trades: u32) -> Result<(), PositionManagerError> {
        let admin = storage::read_admin(&env);
        admin.require_auth();
        storage::write_max_trades_per_pair(&env, max_trades);
        Ok(())
    }
}
