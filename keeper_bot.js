/**
 * LumenLiquid Keeper Bot (MVP)
 *
 * Bot này hoạt động ở chế độ Off-chain (ngoài chuỗi). Nó sẽ làm nhiệm vụ:
 * 1. Định kỳ lấy giá BTC từ Reflector Oracle (CEX Testnet).
 * 2. Tính toán xem lệnh của User có chạm ngưỡng thanh lý hay chưa.
 * 3. Tự động gửi Transaction gọi hàm `liquidate_trade` lên mạng lưới nếu đủ điều kiện.
 *
 * Yêu cầu cài đặt:
 * npm install @stellar/stellar-sdk dotenv
 */

import * as StellarSdk from "@stellar/stellar-sdk";

// --- CONFIGURATION ---
const RPC_URL = "https://soroban-testnet.stellar.org";
const NETWORK_PASSPHRASE = StellarSdk.Networks.TESTNET;
const rpc = new StellarSdk.rpc.Server(RPC_URL);

// Contract IDs
const PM_ID = process.env.PM_ID || "CA5KQQUTGQTQZBE22QVUH7S4DNTIJSZ7KLTCOEACTRRHMQBH4HVMBXAZ";
const ORACLE_ID = process.env.ORACLE_ID || "CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63";

// Bot's Wallet (Keeper)
const KEEPER_SECRET = process.env.KEEPER_SECRET;
if (!KEEPER_SECRET) {
    console.error("❌ BẠN QUÊN CUNG CẤP KEEPER_SECRET!");
    console.error("Vui lòng lấy Secret Key của tài khoản testnet (ví dụ ví vi_test) và chạy lại lệnh sau:");
    console.error('KEEPER_SECRET="SA..." node keeper_bot.js');
    process.exit(1);
}
const keeperKeypair = StellarSdk.Keypair.fromSecret(KEEPER_SECRET);

// --- TRADES TO WATCH (Trong thực tế, bạn sẽ quét Event trên chuỗi để tự lấy list này) ---
const activeTrades = [
    {
        trader: "GB4OWSPNR4HPEZCF2VXSIE4ALAYDMZ4EGPVT2GVL7X7HFZGSKZ5AVSTH",
        pair_index: 0,
        trade_index: 1, // Lệnh số 1 vừa được mở thành công với CEX Oracle
        is_long: true,
        open_price: 614717920722247n, // ~61,471 USD (scaled 1e10)
        collateral: 20000000n, // 2 USDC (scaled 1e7)
        leverage: 5,
        liq_threshold_p: 90 // 90%
    }
];

// Helper: Tính giá thanh lý (Mô phỏng lại y hệt hàm `liquidation_price` trong Rust)
function calculateLiquidationPrice(trade) {
    const collateral = trade.collateral;
    const notional = collateral * BigInt(trade.leverage);
    const maintenance_margin = (collateral * BigInt(100 - trade.liq_threshold_p)) / 100n;

    const pnl_needed = collateral - maintenance_margin;

    let price_delta;
    if (trade.is_long) {
        price_delta = (pnl_needed * trade.open_price) / notional;
        return trade.open_price - price_delta;
    } else {
        price_delta = (pnl_needed * trade.open_price) / notional;
        return trade.open_price + price_delta;
    }
}

// Gọi RPC lấy giá Oracle
async function fetchOraclePrice() {
    const contract = new StellarSdk.Contract(ORACLE_ID);

    // Tạo tham số ReflectorAsset::Other("BTC")
    const assetSym = StellarSdk.xdr.ScVal.scvSymbol("BTC");
    const assetEnum = StellarSdk.xdr.ScVal.scvVec([StellarSdk.xdr.ScVal.scvSymbol("Other"), assetSym]);

    const sourceAccount = await rpc.getAccount(keeperKeypair.publicKey());
    const txBuilder = await rpc.prepareTransaction(
        new StellarSdk.TransactionBuilder(sourceAccount, {
            fee: "1000",
            networkPassphrase: NETWORK_PASSPHRASE
        })
            .addOperation(contract.call("lastprice", assetEnum))
            .setTimeout(30)
            .build()
    );

    const sim = await rpc.simulateTransaction(txBuilder);

    // Parse Option<PriceData> return value
    if (!sim.result || !sim.result.retval) return null;

    // Sử dụng scValToNative để decode chuẩn xác từ XDR sang JS Object!
    const native = StellarSdk.scValToNative(sim.result.retval);
    if (!native) return null;

    // Reflector CEX Oracle trả về price có 14 decimals (1e14).
    // Giao thức LumenLiquid (Position Manager) dùng PRICE_SCALE là 10 decimals (1e10).
    // Do đó, ta phải rescale (chia cho 10^4) để khớp với scale nội bộ của hợp đồng!
    const oraclePrice = native.price;
    const priceScaledTo1e10 = oraclePrice / 10000n;

    return priceScaledTo1e10;
}

// Bắn Transaction Thanh Lý
async function executeLiquidation(trade) {
    console.log(
        `[!] Kích hoạt thanh lý cho Trader ${trade.trader} (Pair: ${trade.pair_index}, Trade: ${trade.trade_index})`
    );

    const pmContract = new StellarSdk.Contract(PM_ID);
    const sourceAccount = await rpc.getAccount(keeperKeypair.publicKey());

    const tx = new StellarSdk.TransactionBuilder(sourceAccount, {
        fee: "10000",
        networkPassphrase: NETWORK_PASSPHRASE
    })
        .addOperation(
            pmContract.call(
                "liquidate_trade",
                new StellarSdk.Address(trade.trader).toScVal(),
                StellarSdk.nativeToScVal(trade.pair_index, { type: "u32" }),
                StellarSdk.nativeToScVal(trade.trade_index, { type: "u32" })
            )
        )
        .setTimeout(30)
        .build();

    const preparedTx = await rpc.prepareTransaction(tx);
    preparedTx.sign(keeperKeypair);

    console.log("Đang gửi giao dịch lên mạng...");
    const sendRes = await rpc.sendTransaction(preparedTx);
    console.log(`Hash: ${sendRes.hash}`);
}

async function loop() {
    console.log("Keeper Bot đang chạy. Quét giá mỗi 10 giây...");
    while (true) {
        try {
            const currentPrice = await fetchOraclePrice();
            if (currentPrice) {
                console.log(
                    `[Oracle] Giá BTC hiện tại: $${(Number(currentPrice) / 1e10).toLocaleString("en-US", {
                        minimumFractionDigits: 2
                    })}`
                );

                for (const trade of activeTrades) {
                    const liqPrice = calculateLiquidationPrice(trade);

                    const isLiquidatable = trade.is_long ? currentPrice <= liqPrice : currentPrice >= liqPrice;

                    if (isLiquidatable) {
                        await executeLiquidation(trade);
                        // Xóa lệnh khỏi danh sách theo dõi
                        activeTrades.splice(activeTrades.indexOf(trade), 1);
                    }
                }
            }
        } catch (e) {
            console.error("Lỗi:", e.message);
        }
        await new Promise(r => setTimeout(r, 10000));
    }
}

loop();
