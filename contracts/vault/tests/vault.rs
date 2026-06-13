//! Integration tests for the Vault contract.

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _, LedgerInfo},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

use vault::{VaultContract, VaultContractClient, VaultError};

const USDC_SCALE: i128 = 10_000_000;

fn one_usdc() -> i128 {
    USDC_SCALE
}

struct Fixture<'a> {
    env: Env,
    client: VaultContractClient<'a>,
    admin: Address,
    pm: Address,
    usdc: Address,
}

fn setup(withdraw_lock_ledgers: u32) -> Fixture<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pm = Address::generate(&env);

    // Underlying USDC SAC with `admin` as issuer (mint authority).
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let usdc = sac.address();

    let id = env.register(VaultContract, ());
    let client = VaultContractClient::new(&env, &id);
    client.init(&admin, &pm, &usdc, &withdraw_lock_ledgers);

    Fixture {
        env,
        client,
        admin,
        pm,
        usdc,
    }
}

fn mint_usdc(env: &Env, usdc: &Address, admin: &Address, to: &Address, amount: i128) {
    let sac_admin = StellarAssetClient::new(env, usdc);
    let _ = admin; // mock_all_auths covers issuer auth
    sac_admin.mint(to, &amount);
}

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

// ───────────────── init ─────────────────

#[test]
fn init_sets_state() {
    let f = setup(0);
    assert_eq!(f.client.admin(), f.admin);
    assert_eq!(f.client.position_manager(), f.pm);
    assert_eq!(f.client.usdc_token(), f.usdc);
    assert_eq!(f.client.total_assets(), 0);
    assert_eq!(f.client.total_shares(), 0);
    assert!(!f.client.is_paused());
}

#[test]
fn init_twice_errors() {
    let f = setup(0);
    let err = f
        .client
        .try_init(&f.admin, &f.pm, &f.usdc, &0)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VaultError::AlreadyInitialized);
}

// ───────────────── deposit / withdraw ─────────────────

#[test]
fn deposit_mints_one_to_one_first_time() {
    let f = setup(0);
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 1_000 * one_usdc());

    let shares = f.client.deposit(&lp, &(1_000 * one_usdc()), &lp);
    assert_eq!(shares, 1_000 * one_usdc());
    assert_eq!(f.client.balance(&lp), 1_000 * one_usdc());
    assert_eq!(f.client.total_assets(), 1_000 * one_usdc());
    assert_eq!(f.client.total_shares(), 1_000 * one_usdc());

    // Vault now holds the USDC.
    let usdc = TokenClient::new(&f.env, &f.usdc);
    assert_eq!(usdc.balance(&f.client.address), 1_000 * one_usdc());
}

#[test]
fn redeem_returns_assets_and_burns_shares() {
    let f = setup(0);
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 1_000 * one_usdc());
    let shares = f.client.deposit(&lp, &(1_000 * one_usdc()), &lp);

    let assets = f.client.redeem(&lp, &shares, &lp);
    assert_eq!(assets, 1_000 * one_usdc());
    assert_eq!(f.client.balance(&lp), 0);
    assert_eq!(f.client.total_assets(), 0);
    let usdc = TokenClient::new(&f.env, &f.usdc);
    assert_eq!(usdc.balance(&lp), 1_000 * one_usdc());
}

#[test]
fn withdraw_exact_assets_burns_required_shares() {
    let f = setup(0);
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 1_000 * one_usdc());
    f.client.deposit(&lp, &(1_000 * one_usdc()), &lp);

    let shares_burned = f.client.withdraw(&lp, &(400 * one_usdc()), &lp);
    assert_eq!(shares_burned, 400 * one_usdc());
    assert_eq!(f.client.total_assets(), 600 * one_usdc());
    assert_eq!(f.client.balance(&lp), 600 * one_usdc());
}

#[test]
fn withdraw_locked_until_window_elapses() {
    let f = setup(100); // 100-ledger lock
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 1_000 * one_usdc());
    let shares = f.client.deposit(&lp, &(1_000 * one_usdc()), &lp);

    let err = f.client.try_redeem(&lp, &shares, &lp).err().unwrap().unwrap();
    assert_eq!(err, VaultError::WithdrawLocked);

    advance_ledger(&f.env, 100);
    let assets = f.client.redeem(&lp, &shares, &lp);
    assert_eq!(assets, 1_000 * one_usdc());
}

// ───────────────── share price after PnL ─────────────────

#[test]
fn share_price_rises_after_vault_gains_on_trader_loss() {
    let f = setup(0);
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 1_000 * one_usdc());
    f.client.deposit(&lp, &(1_000 * one_usdc()), &lp);

    // Simulate a closing trade where the trader lost 100 USDC of a 100 USDC
    // effective collateral. PM first moves effective_collateral into the vault,
    // then settles with net_pnl_for_vault = +100 (vault keeps the loss).
    let trader = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &f.pm, 100 * one_usdc());
    let usdc = TokenClient::new(&f.env, &f.usdc);
    usdc.transfer(&f.pm, &f.client.address, &(100 * one_usdc()));

    // net_pnl_for_vault = gross_payout - effective_collateral = 0 - 100 = -100
    let gross = f.client.return_collateral_with_pnl(&trader, &(100 * one_usdc()), &(-100 * one_usdc()));
    assert_eq!(gross, 0);
    // Vault equity rose by the full 100 USDC loss.
    assert_eq!(f.client.total_assets(), 1_100 * one_usdc());
    // Shares unchanged → each share now worth more.
    assert_eq!(f.client.total_shares(), 1_000 * one_usdc());
    assert_eq!(f.client.convert_to_assets(&(1_000 * one_usdc())), 1_100 * one_usdc());
}

#[test]
fn settle_pays_trader_profit_from_equity() {
    let f = setup(0);
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 1_000 * one_usdc());
    f.client.deposit(&lp, &(1_000 * one_usdc()), &lp);

    let trader = Address::generate(&f.env);
    // PM moves the 100 USDC effective collateral into the vault before settling.
    mint_usdc(&f.env, &f.usdc, &f.admin, &f.pm, 100 * one_usdc());
    let usdc = TokenClient::new(&f.env, &f.usdc);
    usdc.transfer(&f.pm, &f.client.address, &(100 * one_usdc()));

    // Trader profit of 50: gross_payout = 150, net_pnl_for_vault = +50.
    let gross = f.client.return_collateral_with_pnl(&trader, &(100 * one_usdc()), &(50 * one_usdc()));
    assert_eq!(gross, 150 * one_usdc());
    assert_eq!(usdc.balance(&trader), 150 * one_usdc());
    // Vault equity dropped by the 50 it paid out.
    assert_eq!(f.client.total_assets(), 950 * one_usdc());
}

// ───────────────── bad debt ─────────────────

#[test]
fn record_bad_debt_debits_equity_and_pools() {
    let f = setup(0);
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 1_000 * one_usdc());
    f.client.deposit(&lp, &(1_000 * one_usdc()), &lp);

    f.client.record_bad_debt(&3u32, &(200 * one_usdc()));
    assert_eq!(f.client.bad_debt_pool(&3u32), 200 * one_usdc());
    assert_eq!(f.client.total_assets(), 800 * one_usdc());
}

// ───────────────── auth gating ─────────────────

#[test]
fn pm_only_paths_reject_stranger() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let pm = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let usdc = sac.address();
    let id = env.register(VaultContract, ());
    let client = VaultContractClient::new(&env, &id);

    // init with full auth, then drop to no auth so PM calls fail authentication.
    env.mock_all_auths();
    client.init(&admin, &pm, &usdc, &0);
    env.set_auths(&[]);

    let trader = Address::generate(&env);
    let res = client.try_return_collateral_with_pnl(&trader, &0, &0);
    assert!(res.is_err());
    let res2 = client.try_record_bad_debt(&0u32, &(1));
    assert!(res2.is_err());
}

#[test]
fn pause_blocks_deposit() {
    let f = setup(0);
    f.client.pause();
    assert!(f.client.is_paused());
    let lp = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 100 * one_usdc());
    let err = f
        .client
        .try_deposit(&lp, &(100 * one_usdc()), &lp)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VaultError::Paused);

    f.client.unpause();
    let shares = f.client.deposit(&lp, &(100 * one_usdc()), &lp);
    assert_eq!(shares, 100 * one_usdc());
}

// ───────────────── gToken share transfer ─────────────────

#[test]
fn gtoken_transfer_moves_shares() {
    let f = setup(0);
    let lp = Address::generate(&f.env);
    let other = Address::generate(&f.env);
    mint_usdc(&f.env, &f.usdc, &f.admin, &lp, 500 * one_usdc());
    f.client.deposit(&lp, &(500 * one_usdc()), &lp);

    f.client.transfer(&lp, &other, &(200 * one_usdc()));
    assert_eq!(f.client.balance(&lp), 300 * one_usdc());
    assert_eq!(f.client.balance(&other), 200 * one_usdc());
}
