# Pair Registry Contract

Single source of truth for trading pair metadata, group configs, rollover/funding fee accumulators, and open interest.

## Data Types

```rust
// PairInfo — per-pair configuration
// spread_p, max_neg_pnl_p: P_SCALE (1e10, e.g. 5e7 = 0.05%)
// min_lev_pos_usdc, max_oi_usdc: USDC_SCALE (1e7)
// liq_threshold_p, max_gain_p: integer percent (not P_SCALE)
// min_leverage, max_leverage: integer leverage
struct PairInfo {
    symbol: Symbol,
    reflector_asset: ReflectorAsset,  // { Stellar(Address) | Other(Symbol) }
    group_index: u32,
    spread_p: i128,
    min_leverage: u32,
    max_leverage: u32,
    min_lev_pos_usdc: i128,
    max_oi_usdc: i128,
    max_neg_pnl_p: i128,
    liq_threshold_p: u32,
    max_gain_p: u32,
    disabled: bool,
}

// Group — fee and collateral limits shared across pairs
// open_fee_p, close_fee_p: P_SCALE (1e10, default 8e7 = 0.08%)
struct Group {
    name: Symbol,
    max_collateral_usdc: i128,  // USDC_SCALE
    open_fee_p: i128,
    close_fee_p: i128,
}

struct RolloverState {
    acc_per_collateral: i128,  // monotonic, USDC_SCALE
    fee_per_ledger_p: i128,    // P_SCALE
    last_update_ledger: u32,
}

struct FundingState {
    acc_long: i128,
    acc_short: i128,
    fee_per_ledger_p: i128,
    last_update_ledger: u32,
}

struct PairOi {
    long: i128,   // OI in USDC
    short: i128,
}

struct TradeMeta {
    pair_index: u32,
    is_long: bool,
    leverage: u32,
    open_price: i128,
    collateral: i128,         // post-open-fee, USDC_SCALE
    acc_rollover_open: i128,
    acc_funding_open: i128,
}

enum ReflectorAsset {
    Stellar(Address),
    Other(Symbol),
}
```

---

## 1. Initialization

### `init`

One-shot initialization. Sets admin, position manager, and max position size.

**Usecase:** Called once on deployment to bootstrap the registry.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  init \
  --admin <ADMIN_ADDRESS> \
  --position_manager <PM_ADDRESS> \
  --max_pos_usdc <i128>
```

---

## 2. Admin — Pair Config

### `add_pair`

Add a new trading pair. Requires the referenced group to exist.

**Usecase:** Register a new trading pair (e.g. BTC_USDC) under an existing group.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  add_pair \
  --pair_index <u32> \
  --pair '{"symbol": "BTC_USDC", "reflector_asset": {"Other": "BTC"}, "group_index": 0, "spread_p": "50000000", "min_leverage": 2, "max_leverage": 200, "min_lev_pos_usdc": "100000000", "max_oi_usdc": "10000000000000", "max_neg_pnl_p": "9000000000", "liq_threshold_p": 90, "max_gain_p": 900, "disabled": false}'
```

### `update_pair`

Replace an existing pair's config. Group must still exist.

**Usecase:** Change spread, leverage limits, or OI caps of an active pair.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  update_pair \
  --pair_index 0 \
  --new_pair '{"symbol": "BTC_USDC", "reflector_asset": {"Other": "BTC"}, "group_index": 0, "spread_p": "50000000", "min_leverage": 2, "max_leverage": 200, "min_lev_pos_usdc": "100000000", "max_oi_usdc": "10000000000000", "max_neg_pnl_p": "9000000000", "liq_threshold_p": 90, "max_gain_p": 900, "disabled": false}'
```

### `disable_pair`

Set `disabled = true` on a pair (without removing it).

**Usecase:** Halt trading on a pair — new positions cannot be opened while disabled.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  disable_pair \
  --pair_index 0
```

### `set_rollover_rate_p`

Set rollover fee per ledger rate (P_SCALE).

**Usecase:** Adjust how fast rollover fees accrue for a specific pair.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  set_rollover_rate_p \
  --pair_index 0 \
  --rate_p <i128>
```

### `set_funding_rate_p`

Set funding fee per ledger rate (P_SCALE).

**Usecase:** Adjust funding rate for a pair to balance long/short demand.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  set_funding_rate_p \
  --pair_index 0 \
  --rate_p <i128>
```

### `set_one_percent_depth`

Set the 1% market depth (USDC_SCALE) used for price-impact calculations.

**Usecase:** Calibrate price impact — lower depth = higher impact for same trade size.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  set_one_percent_depth \
  --pair_index 0 \
  --depth_usdc <i128>
```

---

## 3. Admin — Group Config

### `add_group`

Create a new group with fee rates and collateral cap.

**Usecase:** Add a new asset class group (e.g. crypto, forex, commodities).

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  add_group \
  --group_index <u32> \
  --group '{"name": "Crypto", "max_collateral_usdc": "100000000000000", "open_fee_p": "80000000", "close_fee_p": "80000000"}'
```

### `update_group`

Replace a group's config entirely.

**Usecase:** Change group name, fees, or collateral cap.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  update_group \
  --group_index 0 \
  --new_group '{"name": "Crypto", "max_collateral_usdc": "200000000000000", "open_fee_p": "80000000", "close_fee_p": "80000000"}'
```

### `set_group_open_fee_p`

Update only the open-fee rate for a group.

**Usecase:** Adjust open fees for all pairs in a group without touching other fields.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  set_group_open_fee_p \
  --group_index 0 \
  --fee_p <i128>
```

### `set_group_close_fee_p`

Update only the close-fee rate for a group.

**Usecase:** Adjust close fees for all pairs in a group independently.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  set_group_close_fee_p \
  --group_index 0 \
  --fee_p <i128>
```

### `set_max_pos_usdc`

Set the protocol-wide maximum position size (USDC_SCALE).

**Usecase:** Risk management — cap the largest single position allowed.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  set_max_pos_usdc \
  --value <i128>
```

---

## 4. Admin — Upgrade

### `upgrade`

Hot-swap the contract wasm. Upload the new wasm via `stellar contract upload`, then call this.

**Usecase:** Deploy a new version of the contract without changing the contract address.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ADDRESS> \
  -- \
  upgrade \
  --new_wasm_hash <BytesN<32>>
```

---

## 5. Views (read-only)

### `get_pair`

Read a pair's configuration.

**Usecase:** Frontend or other contracts fetch pair metadata (spread, leverage range, OI caps).

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_pair \
  --pair_index 0
```

### `pairs_count`

Get total number of registered pairs.

**Usecase:** Iterate over all pairs for bulk reads.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  pairs_count
```

### `get_group`

Read a group's configuration.

**Usecase:** Fetch group fees and collateral cap.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_group \
  --group_index 0
```

### `max_pos_usdc`

Get protocol-wide max position size.

**Usecase:** Frontend validates position size before submission.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  max_pos_usdc
```

### `admin`

Get the admin address.

**Usecase:** Verify who the current admin is.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  admin
```

### `position_manager`

Get the position manager address.

**Usecase:** Verify which contract is authorized to mutate accumulators/OI.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  position_manager
```

### `get_acc_rollover`

Read the stored rollover accumulator state for a pair.

**Usecase:** Get last committed rollover values (non-pending).

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_acc_rollover \
  --pair_index 0
```

### `get_acc_funding`

Read the stored funding accumulator state for a pair.

**Usecase:** Get last committed funding values (non-pending).

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_acc_funding \
  --pair_index 0
```

### `get_oi`

Read the open interest for a pair (long + short).

**Usecase:** Check current OI for a pair.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_oi \
  --pair_index 0
```

### `get_depth`

Read the 1% depth value for a pair.

**Usecase:** Fetch depth for price-impact calculations.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_depth \
  --pair_index 0
```

### `pending_acc_rollover_view`

Compute projected rollover accumulator at `at_ledger` without committing.

**Usecase:** Preview rollover fee for a trade before opening (no state change).

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  pending_acc_rollover_view \
  --pair_index 0 \
  --at_ledger <u32>
```

### `pending_acc_funding_view`

Compute projected funding accumulators at `at_ledger` without committing.

**Usecase:** Preview funding fee for a trade before opening (no state change).

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  pending_acc_funding_view \
  --pair_index 0 \
  --at_ledger <u32>
```

### `get_trade_liquidation_price`

Compute the liquidation price for a trade, accounting for rollover + funding fees projected to `at_ledger`.

**Usecase:** Frontend shows risk — "your position is liquidated if BTC drops to X".

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_trade_liquidation_price \
  --trade '{"pair_index": 0, "is_long": true, "leverage": 10, "open_price": "50000000", "collateral": "100000000", "acc_rollover_open": 0, "acc_funding_open": 0}' \
  --at_ledger <u32>
```

### `is_liquidatable_view`

Check if `observed_price` crosses the trade's liquidation price at `at_ledger`.

**Usecase:** Keeper bots check if a position can be liquidated.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  -- \
  is_liquidatable_view \
  --trade '{"pair_index": 0, "is_long": true, "leverage": 10, "open_price": "50000000", "collateral": "100000000", "acc_rollover_open": 0, "acc_funding_open": 0}' \
  --observed_price <i128> \
  --at_ledger <u32>
```

---

## 6. Mutators (PositionManager only)

### `commit_acc_rollover`

Commit pending rollover accumulator up to `at_ledger`. Auth required from `position_manager`.

**Usecase:** Called by PositionManager during trade open/close to crystallize rollover fees.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <PM_ADDRESS> \
  -- \
  commit_acc_rollover \
  --pair_index 0 \
  --at_ledger <u32>
```

### `commit_acc_funding`

Commit pending funding accumulators up to `at_ledger`. Auth required from `position_manager`.

**Usecase:** Called by PositionManager during trade open/close to crystallize funding fees.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <PM_ADDRESS> \
  -- \
  commit_acc_funding \
  --pair_index 0 \
  --at_ledger <u32>
```

### `add_oi`

Increase OI on one side (long or short) for a pair. Auth required from `position_manager`.

**Usecase:** Called when a trade is opened — adds to total open interest.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <PM_ADDRESS> \
  -- \
  add_oi \
  --pair_index 0 \
  --is_long true \
  --delta_usdc <i128>
```

### `sub_oi`

Decrease OI on one side (long or short) for a pair, floored at 0. Auth required from `position_manager`.

**Usecase:** Called when a trade is closed — subtracts from total open interest.

```
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <PM_ADDRESS> \
  -- \
  sub_oi \
  --pair_index 0 \
  --is_long true \
  --delta_usdc <i128>
```

---

## Error Codes

| Code | Variant               | Description                                          |
|------|-----------------------|------------------------------------------------------|
|  1   | AlreadyInitialized    | `init` called twice                                  |
|  2   | NotInitialized        | Entry point requires `init`                          |
|  3   | NotAdmin              | Caller is not the admin                              |
|  4   | NotPositionManager    | Caller is not the pinned PositionManager             |
|  5   | PairNotFound          | Pair index not found                                 |
|  6   | PairAlreadyExists     | Pair already exists at the given index               |
|  7   | GroupNotFound         | Group index not found                                |
|  8   | GroupAlreadyExists    | Group already exists at the given index              |
|  9   | InvalidParam          | Numeric input outside accepted range                 |
| 10   | MathFault             | Underlying math overflow or div-by-zero              |
| 11   | StaleLedger           | `commit_*` called with a past `at_ledger`            |
