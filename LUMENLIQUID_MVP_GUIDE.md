# Hướng dẫn Build & Deploy (LumenLiquid V2)

Đây là tài liệu hướng dẫn hoàn chỉnh nhất để thiết lập và chạy giao thức LumenLiquid trên Mạng Stellar Testnet. Ở phiên bản V2, chúng ta đã tích hợp Real-time CEX Oracle của Reflector để lấy giá thật của thị trường.

### 1. Build Smart Contracts
Đảm bảo bạn đã cài đặt Rust mới nhất (`>=1.84`) và thêm target `wasm32`.
```bash
rustup target add wasm32-unknown-unknown
stellar contract build
```
Kết quả sẽ sinh ra các file `.wasm` trong thư mục `target/wasm32-unknown-unknown/release/`.

### 2. Deploy lên Testnet
Đẩy file Wasm lên mạng và lưu lại các ID.
```bash
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/pair_registry.wasm --source vi_deploy --network testnet
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/vault.wasm --source vi_deploy --network testnet
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/position_manager.wasm --source vi_deploy --network testnet
```

Cấu hình các biến môi trường để chạy lệnh:
```bash
export REGISTRY_ID="<Điền ID Pair Registry vừa deploy>"
export VAULT_ID="<Điền ID Vault vừa deploy>"
export PM_ID="<Điền ID Position Manager vừa deploy>"
export CEX_ORACLE_ID="CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63" # Testnet CEX Oracle
```

---

# Tổng hợp toàn bộ Public Functions (API Reference) với lệnh Bash

Dưới đây là danh sách chi tiết các hàm đã được expose (có `pub fn`) trong từng Smart Contract, kèm theo **cú pháp dòng lệnh thực tế (Bash CLI)** để bạn có thể copy/paste và chạy ngay. Cấu trúc sẽ đi từ bước khởi tạo đến bước giao dịch của User.

> [!TIP]
> - Các hàm quản trị (Admin) sử dụng `--source vi_deploy`.
> - Các hàm dành cho người dùng/Trader sử dụng `--source vi_test`.
> - Hãy đảm bảo đã export các biến `$PM_ID`, `$REGISTRY_ID`, `$VAULT_ID`, `$CEX_ORACLE_ID` trước khi chạy lệnh.

## Bước 1: Khởi tạo Hệ thống (Initialization)
> [!IMPORTANT]
> Lệnh `init` chỉ có thể chạy đúng 1 lần duy nhất cho mỗi Contract. Nếu chạy sai, bạn sẽ phải Deploy lại Contract mới.

**1. Khởi tạo Pair Registry**
Lưu trữ thông số của các cặp giao dịch.
```bash
stellar contract invoke --id $REGISTRY_ID --source vi_deploy --network testnet -- \
  init \
  --admin $(stellar keys address vi_deploy) \
  --position_manager $PM_ID \
  --max_pos_usdc "10000000000000"
```

**2. Khởi tạo Vault**
Kho bạc quản lý thanh khoản của giao thức (Nơi user gửi USDC vào lấy lãi).
```bash
stellar contract invoke --id $VAULT_ID --source vi_deploy --network testnet -- \
  init \
  --admin $(stellar keys address vi_deploy) \
  --position_manager $PM_ID \
  --usdc_token CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA \
  --withdraw_lock_ledgers 0
```

**3. Khởi tạo Position Manager**
"Bộ não" xử lý Logic Long/Short. Ở đây ta nối thẳng nó với CEX Oracle của Reflector.
```bash
stellar contract invoke --id $PM_ID --source vi_deploy --network testnet -- \
  init \
  --admin $(stellar keys address vi_deploy) \
  --vault $VAULT_ID \
  --pair_registry $REGISTRY_ID \
  --reflector_contract $CEX_ORACLE_ID
```

---

## Bước 2: Cấu hình Thông số Giao dịch (Admin Only)

**1. Thêm Nhóm giao dịch (Group)**
Ví dụ: Thêm nhóm "Crypto" với phí Mở/Đóng là 0.008% (`800000` ở hệ scale 1e10).
```bash
stellar contract invoke --id $REGISTRY_ID --source vi_deploy --network testnet -- \
  add_group \
  --group_index 0 \
  --group '{"name": "crypto", "max_collateral_usdc": "1000000000000", "open_fee_p": "800000", "close_fee_p": "800000"}'
```

**2. Thêm Cặp giao dịch (Pair)**
Thêm cặp BTC/USD. Dùng Oracle `{"Other": "BTC"}`.
```bash
stellar contract invoke --id $REGISTRY_ID --source vi_deploy --network testnet -- \
  add_pair \
  --pair_index 0 \
  --pair '{"symbol": "BTC", "reflector_asset": {"Other": "BTC"}, "group_index": 0, "spread_p": "0", "min_leverage": 1, "max_leverage": 100, "min_lev_pos_usdc": "100000000", "max_oi_usdc": "5000000000000", "max_neg_pnl_p": "900000000", "liq_threshold_p": 90, "max_gain_p": 900, "disabled": false}'
```

**3. Thay đổi Oracle của Position Manager (Khi cần thiết)**
```bash
stellar contract invoke --id $PM_ID --source vi_deploy --network testnet -- \
  set_reflector_contract --reflector_contract $CEX_ORACLE_ID
```

---

## Bước 3: Cung cấp Thanh khoản (Liquidity)

**1. Nạp tiền (Deposit - Nhận gToken)**
Người dùng (Liquidity Provider) nạp tiền (VD: 100 USDC) vào Vault để cho trader vay mượn thanh khoản.
```bash
stellar contract invoke --id $VAULT_ID --source vi_test --network testnet -- \
  deposit \
  --from $(stellar keys address vi_test) \
  --assets "1000000000" \
  --receiver $(stellar keys address vi_test)
```

**2. Rút tiền (Withdraw)**
LP muốn rút đúng một số lượng USDC cố định (VD: 50 USDC).
```bash
stellar contract invoke --id $VAULT_ID --source vi_test --network testnet -- \
  withdraw \
  --from $(stellar keys address vi_test) \
  --assets "500000000" \
  --receiver $(stellar keys address vi_test)
```

**3. Xem dữ liệu Vault**
```bash
# Xem tổng số lượng USDC đang có
stellar contract invoke --id $VAULT_ID --network testnet -- total_assets
```

---

## Bước 4: Giao dịch Long/Short (Trading)

**1. Mở lệnh Market (Open Market Trade)**
Trader đánh lệnh LONG BTC (Đòn bẩy x10, cược 100 USDC = `1000000000`).
```bash
stellar contract invoke --id $PM_ID --source vi_test --network testnet -- \
  open_market_trade \
  --trader $(stellar keys address vi_test) \
  --pair_index 0 \
  --is_long true \
  --collateral "1000000000" \
  --leverage 10
```

**2. Đóng lệnh Market (Close Market Trade)**
Trader chốt lời lệnh số 0.
```bash
stellar contract invoke --id $PM_ID --source vi_test --network testnet -- \
  close_market_trade \
  --trader $(stellar keys address vi_test) \
  --pair_index 0 \
  --trade_index 0
```
> [!NOTE]
> Khi mở và đóng lệnh, hệ thống sẽ tính phí (VD: 0.008%). Toàn bộ Open Fee và Close Fee sẽ được giữ lại tại ví của hợp đồng `PositionManager` làm doanh thu của sàn (Protocol Revenue). Lợi nhuận/thua lỗ ròng (PnL) của lệnh mới được kết toán với `Vault`.

**3. Xem trạng thái lệnh hiện tại (View Trade)**
Xem toàn bộ thông số chi tiết (Tiền cược, đòn bẩy, giá mở) của lệnh số 0.
```bash
stellar contract invoke --id $PM_ID --source vi_test --network testnet -- \
  get_trade \
  --trader $(stellar keys address vi_test) \
  --pair_index 0 \
  --trade_index 0
```

**4. Xem PnL (Lãi/Lỗ) Real-time**
Hàm View lấy giá hiện tại từ CEX Oracle để tính toán PnL của lệnh (Đơn vị: `1e7`, ví dụ `-13500000` là `-1.35 USDC`).
```bash
stellar contract invoke --id $PM_ID --source vi_test --network testnet -- \
  get_trade_pnl \
  --trader $(stellar keys address vi_test) \
  --pair_index 0 \
  --trade_index 0
```

**5. Đặt lệnh chờ Limit (Place Limit Order)**
Trader muốn LONG nếu BTC sập xuống 48k.
```bash
stellar contract invoke --id $PM_ID --source vi_test --network testnet -- \
  place_limit_order \
  --trader $(stellar keys address vi_test) \
  --pair_index 0 \
  --is_long true \
  --collateral "1000000000" \
  --leverage 10 \
  --limit_price "4800000000000000000"
```

**4. Kích hoạt lệnh Limit (Keeper/Bot)**
Bot liên tục quét Oracle, nếu giá chạm 48k thì Bot bắn lệnh này để mở vị thế cho Trader.
```bash
stellar contract invoke --id $PM_ID --source vi_deploy --network testnet -- \
  execute_limit_order \
  --keeper $(stellar keys address vi_deploy) \
  --trader $(stellar keys address vi_test) \
  --pair_index 0 \
  --limit_index 0
```

**5. Thanh lý lệnh (Keeper/Bot)**
Bot (Ví dụ `keeper_bot.js`) gọi lệnh này khi Trader gồng lỗ chạm Maintenance Margin (90%).
```bash
stellar contract invoke --id $PM_ID --source vi_deploy --network testnet -- \
  liquidate_trade \
  --trader $(stellar keys address vi_test) \
  --pair_index 0 \
  --trade_index 0
```

---

## Bước 5: Nâng cấp Smart Contract (Upgrade)

Khi có bản cập nhật tính năng mới (V3, V4), Admin không cần Deploy mới mà chỉ cần `install` file wasm để lấy chuỗi Hash, rồi dùng tính năng `upgrade` có sẵn trong tất cả các hợp đồng.

```bash
# Lấy mã Hash
stellar contract install --wasm target/wasm32-unknown-unknown/release/position_manager.wasm --source vi_deploy --network testnet

# Nâng cấp
stellar contract invoke --id $PM_ID --source vi_deploy --network testnet -- \
  upgrade --new_wasm_hash <HEX_HASH_TỪ_LỆNH_INSTALL>
```
