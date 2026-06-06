//! In-memory mock implementations of [`crate::OracleSource`] and
//! [`crate::SubscriptionSource`] for unit tests. Lives in a `mock` module rather
//! than under `cfg(test)` so downstream contract test crates can reuse it.
//!
//! These mocks deliberately do NOT mimic Reflector's auth or fee-burning
//! semantics — they exist only to drive the position-manager state machine
//! through scripted price reads and subscription IDs.

extern crate alloc;
use alloc::vec::Vec as StdVec;
use core::cell::RefCell;

use soroban_sdk::{Address, Env};

use crate::{
    OracleSource, PriceObservation, ReflectorAsset, Subscription, SubscriptionInitParams,
    SubscriptionSource, SubscriptionStatus,
};

/// Stub price source. Returns the configured `(price, timestamp, decimals)`
/// triple regardless of the asset queried, unless an asset-specific override
/// has been pushed.
#[derive(Default)]
pub struct MockOracle {
    inner: RefCell<MockOracleInner>,
}

#[derive(Default)]
struct MockOracleInner {
    /// Default response when no asset override is set.
    default: Option<PriceObservation>,
    /// Per-asset overrides, matched by `ReflectorAsset` debug equality.
    overrides: StdVec<(ReflectorAsset, PriceObservation)>,
}

impl MockOracle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the price returned for any asset that has no specific override.
    pub fn set_default(&self, obs: PriceObservation) {
        self.inner.borrow_mut().default = Some(obs);
    }

    /// Set the price returned specifically for `asset`.
    pub fn set_for(&self, asset: ReflectorAsset, obs: PriceObservation) {
        let mut g = self.inner.borrow_mut();
        if let Some(existing) = g.overrides.iter_mut().find(|(a, _)| *a == asset) {
            existing.1 = obs;
        } else {
            g.overrides.push((asset, obs));
        }
    }

    /// Remove all configured prices.
    pub fn clear(&self) {
        let mut g = self.inner.borrow_mut();
        g.default = None;
        g.overrides.clear();
    }
}

impl OracleSource for MockOracle {
    fn read_price(&self, _env: &Env, asset: &ReflectorAsset) -> Option<PriceObservation> {
        let g = self.inner.borrow();
        if let Some((_, obs)) = g.overrides.iter().find(|(a, _)| a == asset) {
            return Some(*obs);
        }
        g.default
    }
}

/// Stub subscription source. Hands out monotonically increasing IDs and
/// echoes back a `Subscription` reflecting the `params` plus a synthetic
/// `balance = amount`. Cancellation removes the entry; subsequent calls
/// to `cancel` panic to mirror the real contract's `SubscriptionNotFound`.
#[derive(Default)]
pub struct MockSubscription {
    inner: RefCell<MockSubscriptionInner>,
}

#[derive(Default)]
struct MockSubscriptionInner {
    next_id: u64,
    active: StdVec<(u64, Subscription)>,
    /// Captured cancellations, in call order. Useful for asserting test flow.
    cancellations: StdVec<u64>,
    /// Captured deposits, in call order: `(subscription_id, amount)`.
    deposits: StdVec<(u64, u64)>,
}

impl MockSubscription {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancellations(&self) -> StdVec<u64> {
        self.inner.borrow().cancellations.clone()
    }

    pub fn deposits(&self) -> StdVec<(u64, u64)> {
        self.inner.borrow().deposits.clone()
    }

    pub fn is_active(&self, id: u64) -> bool {
        self.inner.borrow().active.iter().any(|(i, _)| *i == id)
    }
}

impl SubscriptionSource for MockSubscription {
    fn create_subscription(
        &self,
        _env: &Env,
        params: SubscriptionInitParams,
        amount: u64,
    ) -> (u64, Subscription) {
        let mut g = self.inner.borrow_mut();
        g.next_id += 1;
        let id = g.next_id;
        let sub = Subscription {
            owner: params.owner.clone(),
            base: params.base.clone(),
            quote: params.quote.clone(),
            threshold: params.threshold,
            heartbeat: params.heartbeat,
            webhook: params.webhook.clone(),
            balance: amount,
            status: SubscriptionStatus::Active,
            updated: 0,
        };
        g.active.push((id, sub.clone()));
        (id, sub)
    }

    fn cancel(&self, _env: &Env, subscription_id: u64) {
        let mut g = self.inner.borrow_mut();
        let pos = g
            .active
            .iter()
            .position(|(i, _)| *i == subscription_id)
            .unwrap_or_else(|| panic!("MockSubscription::cancel: id not found"));
        g.active.remove(pos);
        g.cancellations.push(subscription_id);
    }

    fn deposit(&self, _env: &Env, _from: &Address, subscription_id: u64, amount: u64) {
        let mut g = self.inner.borrow_mut();
        if let Some((_, sub)) = g.active.iter_mut().find(|(i, _)| *i == subscription_id) {
            sub.balance = sub.balance.checked_add(amount).expect("balance overflow");
        }
        g.deposits.push((subscription_id, amount));
    }
}
