#![no_std]
//! PairRegistry — single source of truth for trading pair metadata, group
//! configs, and rollover/funding fee accumulators.
//!
//! Phase-1 surface (per `openspec/changes/port-trading-to-soroban`):
//!
//! * Admin: `init`, `add_pair`, `update_pair`, `disable_pair`,
//!   `add_group`, `update_group`, `set_group_open_fee_p`,
//!   `set_group_close_fee_p`, `set_max_pos_usdc`,
//!   `set_pair_rollover_fee_per_ledger_p`,
//!   `set_pair_funding_fee_per_ledger_p`, `set_pair_one_percent_depth`.
//! * Views: `get_pair`, `pairs_count`, `get_group`,
//!   `get_acc_rollover`, `get_acc_funding`, `get_oi`, `max_pos_usdc`.
//! * Stateful (PositionManager-only): `commit_acc_rollover`,
//!   `commit_acc_funding`, `add_oi`, `sub_oi`. These mutate accumulators or
//!   per-side OI as part of open/close paths and require auth from the
//!   admin-pinned `position_manager` address.
//! * Pure views: `get_trade_liquidation_price` and `is_liquidatable_view`.

pub mod contract;
pub mod errors;
pub mod storage;
pub mod types;

pub use contract::{PairRegistryContract, PairRegistryContractClient};
pub use errors::PairRegistryError;
pub use types::*;
