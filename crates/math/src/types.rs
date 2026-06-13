use soroban_sdk::contracttype;

/// USDC amount at `USDC_SCALE` (1e7). Always a non-negative balance in storage; signed deltas
/// (PnL) use raw `i128`.
#[contracttype]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsdcAmount(pub i128);

/// Price at `PRICE_SCALE` (1e10).
#[contracttype]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Price(pub i128);

/// Percentage at `P_SCALE` (1e10). `1e10 == 100%`. Can be negative for signed PnL %.
#[contracttype]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PercentP(pub i128);

/// Leverage. Stored as `u32` — leverages are integer-valued (e.g. `2`, `5`, `50`).
#[contracttype]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Leverage(pub u32);

impl UsdcAmount {
    pub const ZERO: Self = Self(0);
    pub const fn raw(self) -> i128 { self.0 }
}

impl Price {
    pub const ZERO: Self = Self(0);
    pub const fn raw(self) -> i128 { self.0 }
}

impl PercentP {
    pub const ZERO: Self = Self(0);
    pub const fn raw(self) -> i128 { self.0 }
}

impl Leverage {
    pub const fn raw(self) -> u32 { self.0 }
}
