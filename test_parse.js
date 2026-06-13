import * as StellarSdk from '@stellar/stellar-sdk';

const RPC_URL = 'https://soroban-testnet.stellar.org';
const rpc = new StellarSdk.rpc.Server(RPC_URL);

async function test() {
    const ORACLE_ID = 'CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63';
    const contract = new StellarSdk.Contract(ORACLE_ID);
    
    const assetSym = StellarSdk.xdr.ScVal.scvSymbol("BTC");
    const assetEnum = StellarSdk.xdr.ScVal.scvVec([
        StellarSdk.xdr.ScVal.scvSymbol("Other"),
        assetSym
    ]);

    const txBuilder = await rpc.prepareTransaction(
        new StellarSdk.TransactionBuilder(await rpc.getAccount("GB4OWSPNR4HPEZCF2VXSIE4ALAYDMZ4EGPVT2GVL7X7HFZGSKZ5AVSTH"), {
            fee: "1000",
            networkPassphrase: StellarSdk.Networks.TESTNET,
        }).addOperation(
            contract.call("lastprice", assetEnum)
        ).setTimeout(30).build()
    );

    const sim = await rpc.simulateTransaction(txBuilder);
    const native = StellarSdk.scValToNative(sim.result.retval);
    console.log("Parsed:", native);
}
test();
