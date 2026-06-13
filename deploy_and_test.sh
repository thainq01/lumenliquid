#!/bin/bash
set -e

# Setup keys
DEPLOY_KEY="vi_deploy"
TEST_KEY="vi_test"
NETWORK="testnet"
USDC_ID="CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA" # USDC ID fetched from vi_test balance

echo "======================================"
echo "1. Build Contracts (wasm32v1-none)"
echo "======================================"
cargo build --target wasm32v1-none --release

echo "======================================"
echo "2. Deploy Contracts"
echo "======================================"
REGISTRY_ID=$(stellar contract deploy --wasm target/wasm32v1-none/release/pair_registry.wasm --source $DEPLOY_KEY --network $NETWORK)
echo "Registry ID: $REGISTRY_ID"

VAULT_ID=$(stellar contract deploy --wasm target/wasm32v1-none/release/vault.wasm --source $DEPLOY_KEY --network $NETWORK)
echo "Vault ID: $VAULT_ID"

PM_ID=$(stellar contract deploy --wasm target/wasm32v1-none/release/position_manager.wasm --source $DEPLOY_KEY --network $NETWORK)
echo "Position Manager ID: $PM_ID"

ORACLE_ID=$(stellar contract deploy --wasm target/wasm32v1-none/release/mock_oracle.wasm --source $DEPLOY_KEY --network $NETWORK)
echo "Oracle ID: $ORACLE_ID"

echo "======================================"
echo "3. Initialize Mock Oracle"
echo "======================================"
# Set BTC price to 50,000 USD (50,000 * 10^14)
stellar contract invoke --id $ORACLE_ID --source $DEPLOY_KEY --network $NETWORK -- \
  set_price --asset '{"Other": ["BTC"]}' --price 5000000000000000000

echo "======================================"
echo "4. Initialize Core Contracts"
echo "======================================"
# Init Registry: max global OI = 1,000,000 USDC (1e13)
stellar contract invoke --id $REGISTRY_ID --source $DEPLOY_KEY --network $NETWORK -- \
  init \
  --admin $(stellar keys address $DEPLOY_KEY) \
  --position_manager $PM_ID \
  --max_global_oi_usdc 10000000000000

# Init Vault
stellar contract invoke --id $VAULT_ID --source $DEPLOY_KEY --network $NETWORK -- \
  init \
  --admin $(stellar keys address $DEPLOY_KEY) \
  --fee_receiver $(stellar keys address $DEPLOY_KEY) \
  --usdc_address $USDC_ID \
  --emergency_timelock 0

# Set PM in Vault
stellar contract invoke --id $VAULT_ID --source $DEPLOY_KEY --network $NETWORK -- \
  set_position_manager --pm_address $PM_ID

# Init Position Manager
stellar contract invoke --id $PM_ID --source $DEPLOY_KEY --network $NETWORK -- \
  init \
  --admin $(stellar keys address $DEPLOY_KEY) \
  --vault_address $VAULT_ID \
  --registry_address $REGISTRY_ID \
  --oracle_address $ORACLE_ID

echo "======================================"
echo "5. Setup Trading Config (BTC)"
echo "======================================"
# Add Group
stellar contract invoke --id $REGISTRY_ID --source $DEPLOY_KEY --network $NETWORK -- \
  add_group --group_index 0 --group '{"name": "crypto", "max_collateral_usdc": 1000000000000, "open_fee_p": 800000, "close_fee_p": 800000}'

# Add Pair
stellar contract invoke --id $REGISTRY_ID --source $DEPLOY_KEY --network $NETWORK -- \
  add_pair --pair_index 0 --pair '{"symbol": "BTC", "reflector_asset": {"Other": ["BTC"]}, "group_index": 0, "spread_p": 0, "min_leverage": 1, "max_leverage": 100, "min_lev_pos_usdc": 100000000, "max_oi_usdc": 5000000000000, "max_neg_pnl_p": 900000000, "liq_threshold_p": 90, "max_gain_p": 900, "disabled": false}'

echo "======================================"
echo "6. Test Trade with vi_test"
echo "======================================"
# 1. Provide Vault Liquidity
# To trade, the Vault needs USDC to pay for potential PnL wins. We deposit 100 USDC from vi_test.
echo "Depositing 100 USDC to Vault from vi_test..."
stellar contract invoke --id $VAULT_ID --source $TEST_KEY --network $NETWORK -- \
  deposit --from $(stellar keys address $TEST_KEY) --amount 1000000000 --to $(stellar keys address $TEST_KEY)

# 2. Open Market Trade
# Open Long Trade with 100 USDC, 10x leverage
echo "Opening Long Trade of 100 USDC on BTC..."
stellar contract invoke --id $PM_ID --source $TEST_KEY --network $NETWORK -- \
  open_market_trade \
  --trader $(stellar keys address $TEST_KEY) \
  --pair_index 0 \
  --is_long true \
  --collateral 1000000000 \
  --leverage 10

echo "✅ Deploy and Trade successful!"
echo "Registry: $REGISTRY_ID"
echo "Vault: $VAULT_ID"
echo "Position Manager: $PM_ID"
echo "Oracle: $ORACLE_ID"
