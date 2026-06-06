# soroban-contracts

Soroban (Stellar) port of the perpetual DEX. See `openspec/changes/port-trading-to-soroban/` for the full design.

## Layout

- `contracts/position-manager` — main trading contract (open/close/limit/liquidate, Reflector callback)
- `contracts/pair-registry` — pair config + funding/rollover accumulators
- `contracts/vault` — SEP-0056 tokenized vault + SEP-41 gToken shares
- `crates/math` — shared scale constants, newtypes, fee/PnL/liq-price math (workspace-only, not deployed)
- `crates/reflector-adapter` — Reflector (SEP-40) client wrapper used by all three contracts

## Build

```bash
cargo build --release --target wasm32v1-none
```

## Test

```bash
cargo test
```

## Pinned versions

- `soroban-sdk = 26.0.1`
- `soroban-fixed-point-math = 1.5.0`
- `stellar-access`, `stellar-macros`, `stellar-contract-utils`, `stellar-tokens` = `0.7.1`
