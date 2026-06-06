//! Integration tests for the PairRegistry contract.

extern crate std;

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _, LedgerInfo, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, Symbol,
};

use pair_registry::{
    Group, PairInfo, PairRegistryContract, PairRegistryContractClient, PairRegistryError, TradeMeta,
};
use reflector_adapter::ReflectorAsset;

const USDC_SCALE: i128 = 10_000_000;
const PRICE_SCALE: i128 = 10_000_000_000;
const P_SCALE: i128 = 10_000_000_000;

fn one_usdc() -> i128 {
    USDC_SCALE
}

fn install_registry(env: &Env) -> (Address, PairRegistryContractClient<'_>) {
    let id = env.register(PairRegistryContract, ());
    let client = PairRegistryContractClient::new(env, &id);
    (id, client)
}

fn default_pair(group_index: u32) -> PairInfo {
    PairInfo {
        symbol: symbol_short!("BTC"),
        reflector_asset: ReflectorAsset::Other(symbol_short!("BTC")),
        group_index,
        spread_p: 5_000_000,                    // 0.05%
        min_leverage: 2,
        max_leverage: 50,
        min_lev_pos_usdc: 100 * one_usdc(),     // 100 USDC notional min
        max_oi_usdc: 1_000_000 * one_usdc(),
        max_neg_pnl_p: 100 * P_SCALE,           // -100% PnL trigger ≈ liq
        liq_threshold_p: 90,
        max_gain_p: 900,
        disabled: false,
    }
}

fn default_group(env: &Env) -> Group {
    Group {
        name: Symbol::new(env, "crypto"),
        max_collateral_usdc: 10_000_000 * one_usdc(),
        open_fee_p: 80_000_000,
        close_fee_p: 80_000_000,
    }
}

fn init_registry<'a>(env: &'a Env) -> (PairRegistryContractClient<'a>, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let pm = Address::generate(env);
    let (_, client) = install_registry(env);
    client.init(&admin, &pm, &(100_000 * one_usdc()));
    (client, admin, pm)
}

// ────────────── init / admin gating ──────────────

#[test]
fn init_sets_admin_and_max_pos() {
    let env = Env::default();
    let (client, admin, pm) = init_registry(&env);
    assert_eq!(client.admin(), admin);
    assert_eq!(client.position_manager(), pm);
    assert_eq!(client.max_pos_usdc(), 100_000 * one_usdc());
    assert_eq!(client.pairs_count(), 0);
}

#[test]
fn init_twice_errors() {
    let env = Env::default();
    let (client, admin, pm) = init_registry(&env);
    let err = client
        .try_init(&admin, &pm, &(1))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, PairRegistryError::AlreadyInitialized);
}

#[test]
fn add_pair_requires_existing_group() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    // group 0 not added yet
    let err = client
        .try_add_pair(&0u32, &default_pair(0))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, PairRegistryError::GroupNotFound);
}

#[test]
fn add_pair_after_group_succeeds_and_increments_count() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&7u32, &default_pair(0));
    assert_eq!(client.pairs_count(), 1);
    assert_eq!(client.get_pair(&7u32).symbol, symbol_short!("BTC"));
}

#[test]
fn add_duplicate_pair_errors() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&7u32, &default_pair(0));
    let err = client
        .try_add_pair(&7u32, &default_pair(0))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, PairRegistryError::PairAlreadyExists);
}

#[test]
fn disable_pair_flips_flag() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&7u32, &default_pair(0));
    client.disable_pair(&7u32);
    let p = client.get_pair(&7u32);
    assert!(p.disabled);
}

#[test]
fn update_pair_replaces_entry() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&7u32, &default_pair(0));
    let mut updated = default_pair(0);
    updated.max_leverage = 100;
    client.update_pair(&7u32, &updated);
    assert_eq!(client.get_pair(&7u32).max_leverage, 100);
}

#[test]
fn update_pair_unknown_index_errors() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    let err = client
        .try_update_pair(&99u32, &default_pair(0))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, PairRegistryError::PairNotFound);
}

#[test]
fn invalid_pair_param_rejected() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    let mut bad = default_pair(0);
    bad.min_leverage = 10;
    bad.max_leverage = 5; // max < min
    let err = client.try_add_pair(&1u32, &bad).err().unwrap().unwrap();
    assert_eq!(err, PairRegistryError::InvalidParam);
}

#[test]
fn set_max_pos_usdc_admin_only() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pm = Address::generate(&env);
    let (id, client) = install_registry(&env);
    client.init(&admin, &pm, &(100));
    // explicit auth scoping: prove a non-admin call would fail authentication.
    let stranger = Address::generate(&env);
    let result = client
        .mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &id,
                fn_name: "set_max_pos_usdc",
                args: (200i128,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_set_max_pos_usdc(&200);
    // stranger isn't admin → require_auth fails as a host error (not contract error)
    assert!(result.is_err());
}

// ────────────── group ops ──────────────

#[test]
fn add_group_then_update_fees() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.set_group_open_fee_p(&0u32, &50_000_000);
    client.set_group_close_fee_p(&0u32, &60_000_000);
    let g = client.get_group(&0u32);
    assert_eq!(g.open_fee_p, 50_000_000);
    assert_eq!(g.close_fee_p, 60_000_000);
}

#[test]
fn add_duplicate_group_errors() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    let err = client
        .try_add_group(&0u32, &default_group(&env))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, PairRegistryError::GroupAlreadyExists);
}

// ────────────── accumulators ──────────────

fn advance_ledger(env: &Env, by: u32) {
    let info = env.ledger().get();
    let mut next = LedgerInfo {
        timestamp: info.timestamp + (by as u64) * 5,
        protocol_version: info.protocol_version,
        sequence_number: info.sequence_number + by,
        network_id: info.network_id,
        base_reserve: info.base_reserve,
        min_temp_entry_ttl: info.min_temp_entry_ttl,
        min_persistent_entry_ttl: info.min_persistent_entry_ttl,
        max_entry_ttl: info.max_entry_ttl,
    };
    next.sequence_number = info.sequence_number + by;
    env.ledger().set(next);
}

#[test]
fn rollover_acc_grows_after_first_baseline() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&0u32, &default_pair(0));
    client.set_rollover_rate_p(&0u32, &1_000_000); // 0.0001% per ledger

    // Establish baseline at current ledger
    let l0 = env.ledger().sequence();
    let acc0 = client.commit_acc_rollover(&0u32, &l0);
    assert_eq!(acc0, 0);

    advance_ledger(&env, 100);
    let l1 = env.ledger().sequence();
    let acc1 = client.commit_acc_rollover(&0u32, &l1);
    // expected = 100 * 1e6 * 1e7 / 1e10 / 100 = 1000
    assert_eq!(acc1, 1000);

    // Per-trade rollover fee on 1000 USDC after this advance
    let trade = TradeMeta {
        pair_index: 0,
        is_long: true,
        leverage: 10,
        open_price: 50_000 * PRICE_SCALE,
        collateral: 1000 * one_usdc(),
        acc_rollover_open: 0,
        acc_funding_open: 0,
    };
    let liq = client.get_trade_liquidation_price(&trade, &l1);
    // No funding rate set → only rollover bites. acc=1000 → fee = 1000 * 1e10 / 1e7 = 1e6 USDC scale = 0.1 USDC
    // (effectively nothing) liq just below open*0.99.
    assert!(liq > 0);
    assert!(liq < trade.open_price);
}

#[test]
fn funding_acc_long_heavy_pays_short() {
    let env = Env::default();
    let (client, _admin, pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&0u32, &default_pair(0));
    client.set_funding_rate_p(&0u32, &1_000_000);

    // Seed OI: long-heavy
    let _ = pm; // (mock_all_auths handles it)
    client.add_oi(&0u32, &true, &(1_500_000 * one_usdc()));
    client.add_oi(&0u32, &false, &(500_000 * one_usdc()));

    let l0 = env.ledger().sequence();
    let _ = client.commit_acc_funding(&0u32, &l0);
    advance_ledger(&env, 100);
    let l1 = env.ledger().sequence();
    let (acc_long, acc_short) = client.commit_acc_funding(&0u32, &l1);
    assert!(acc_long > 0, "long should pay (acc grows)");
    assert!(acc_short < 0, "short should receive (acc decreases)");
}

#[test]
fn pending_view_does_not_mutate() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&0u32, &default_pair(0));
    client.set_rollover_rate_p(&0u32, &1_000_000);

    let l0 = env.ledger().sequence();
    let _ = client.commit_acc_rollover(&0u32, &l0);
    advance_ledger(&env, 50);
    let l1 = env.ledger().sequence();
    let preview = client.pending_acc_rollover_view(&0u32, &l1);
    // Stored state still at baseline acc=0
    let stored = client.get_acc_rollover(&0u32);
    assert_eq!(stored.acc_per_collateral, 0);
    assert!(preview > 0);
}

// ────────────── liq price view ──────────────

#[test]
fn liquidation_price_long_no_fees() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&0u32, &default_pair(0));

    // No accumulator commits → fees stay at 0 regardless of ledger advance.
    let trade = TradeMeta {
        pair_index: 0,
        is_long: true,
        leverage: 10,
        open_price: 100 * PRICE_SCALE,
        collateral: 1000 * one_usdc(),
        acc_rollover_open: 0,
        acc_funding_open: 0,
    };
    let l = env.ledger().sequence();
    let liq = client.get_trade_liquidation_price(&trade, &l);
    // distance = 100 * (1000*90/100) / 1000 / 10 = 9 → liq = 91
    assert_eq!(liq, 91 * PRICE_SCALE);
}

#[test]
fn is_liquidatable_long_threshold_via_view() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&0u32, &default_pair(0));
    let trade = TradeMeta {
        pair_index: 0,
        is_long: true,
        leverage: 10,
        open_price: 100 * PRICE_SCALE,
        collateral: 1000 * one_usdc(),
        acc_rollover_open: 0,
        acc_funding_open: 0,
    };
    let l = env.ledger().sequence();
    assert!(client.is_liquidatable_view(&trade, &(91 * PRICE_SCALE), &l));
    assert!(client.is_liquidatable_view(&trade, &(90 * PRICE_SCALE), &l));
    assert!(!client.is_liquidatable_view(&trade, &(92 * PRICE_SCALE), &l));
}

// ────────────── OI ops ──────────────

#[test]
fn oi_add_sub_floors_at_zero() {
    let env = Env::default();
    let (client, _admin, _pm) = init_registry(&env);
    client.add_group(&0u32, &default_group(&env));
    client.add_pair(&0u32, &default_pair(0));
    let oi1 = client.add_oi(&0u32, &true, &(100 * one_usdc()));
    assert_eq!(oi1.long, 100 * one_usdc());
    // Subtract more than present — clamps to 0
    let oi2 = client.sub_oi(&0u32, &true, &(500 * one_usdc()));
    assert_eq!(oi2.long, 0);
}

// ────────────── upgrade ──────────────

#[test]
fn upgrade_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pm = Address::generate(&env);
    let (id, client) = install_registry(&env);
    client.init(&admin, &pm, &(100 * one_usdc()));

    let stranger = Address::generate(&env);
    let dummy_hash: BytesN<32> = BytesN::from_array(&env, &[0u8; 32]);
    let result = client
        .mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &id,
                fn_name: "upgrade",
                args: (dummy_hash.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_upgrade(&dummy_hash);
    // stranger isn't admin → require_auth fails as a host error
    assert!(result.is_err());
}
