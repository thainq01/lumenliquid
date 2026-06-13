#![no_std]
//! Position manager — implemented in Phase 3+. Phase 1 only requires the crate to load.

mod contract;
pub mod errors;
mod storage;
pub mod types;

pub use contract::{PositionManagerContract, PositionManagerContractClient};

#[cfg(test)]
mod test;
