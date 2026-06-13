#![no_std]
//! Vault — SEP-0056 tokenized vault holding LP USDC that backs trader PnL.
//!
//! Surface (per `openspec/changes/port-trading-to-soroban`):
//!
//! * SEP-41 gToken shares: `allowance`, `approve`, `balance`, `transfer`,
//!   `transfer_from`, `burn`, `burn_from`, `decimals`, `name`, `symbol`.
//! * SEP-0056 vault: `deposit`, `mint`, `withdraw`, `redeem`,
//!   `total_assets`, `total_shares`, `convert_to_shares`, `convert_to_assets`.
//! * PositionManager-only: `take_collateral`, `return_collateral_with_pnl`,
//!   `record_bad_debt`.
//! * Admin: `set_withdraw_lock`, `set_position_manager`, `pause`, `unpause`,
//!   `upgrade`.
//! * Views: `bad_debt_pool`, `withdraw_lock_ledgers`, `last_deposit_ledger`,
//!   `is_paused`, `admin`, `position_manager`, `usdc_token`.

pub mod contract;
pub mod errors;
pub mod storage;
pub mod types;

pub use contract::{VaultContract, VaultContractClient};
pub use errors::VaultError;
pub use types::*;
