import * as StellarSdk from '@stellar/stellar-sdk';
const rpc = new StellarSdk.rpc.Server('https://soroban-testnet.stellar.org');
async function test() {
    const contract = new StellarSdk.Contract('CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63');
    const txBuilder = await rpc.prepareTransaction(
        new StellarSdk.TransactionBuilder(await rpc.getAccount("GB4OWSPNR4HPEZCF2VXSIE4ALAYDMZ4EGPVT2GVL7X7HFZGSKZ5AVSTH"), {
            fee: "1000",
            networkPassphrase: StellarSdk.Networks.TESTNET,
        }).addOperation(contract.call("decimals")).setTimeout(30).build()
    );
    const sim = await rpc.simulateTransaction(txBuilder);
    console.log("Decimals:", StellarSdk.scValToNative(sim.result.retval));
}
test();
