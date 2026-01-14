import { PublicKey, SystemProgram, Transaction, TransactionInstruction, TransactionMessage, VersionedTransaction } from '@solana/web3.js';
import BN from 'bn.js';
import base58 from 'bs58';
import borsh from 'borsh';
import { Buffer } from 'buffer';
import { registerListener, unregisterListener } from '../utils/clusterFactory.js';
import { SolConstantsUtil, SolStoreUtil } from '../utils/scaffold/index.js';
import { getHashedName, getNameAccountKey } from '../utils/hash.js';
import { NameRegistry } from '../utils/nameService.js';
export class BaseConnector {
    getConnectorName() {
        return 'base';
    }
    get publicKey() {
        return new PublicKey(SolStoreUtil.state.address ?? '');
    }
    async getProvider() {
        return Promise.reject(new Error('No provider in base connector'));
    }
    async constructTransaction(type, params) {
        const transaction = new Transaction();
        const fromAddress = SolStoreUtil.state.address;
        if (!fromAddress) {
            throw new Error('No address connected');
        }
        const fromPubkey = new PublicKey(fromAddress);
        if (type === 'transfer') {
            const transferParams = params;
            const toPubkey = new PublicKey(transferParams.to);
            transaction.add(SystemProgram.transfer({
                fromPubkey,
                toPubkey: new PublicKey(transferParams.to),
                lamports: transferParams.amountInLamports
            }));
            transaction.feePayer = transferParams.feePayer === 'from' ? fromPubkey : toPubkey;
        }
        else if (type === 'program') {
            const programParams = params;
            transaction.add(new TransactionInstruction({
                keys: [
                    { pubkey: fromPubkey, isSigner: true, isWritable: programParams.isWritableSender }
                ],
                programId: new PublicKey(programParams.programId),
                data: Buffer.from(base58.decode(JSON.stringify(programParams.data)))
            }));
            transaction.feePayer = fromPubkey;
        }
        else {
            throw new Error(`No transaction configuration for type ${String(type)}`);
        }
        const response = await this.requestCluster('getLatestBlockhash', [{}]);
        const { blockhash: recentBlockhash } = response.value;
        transaction.recentBlockhash = recentBlockhash;
        return transaction;
    }
    async constructVersionedTransaction(params) {
        const fromAddress = SolStoreUtil.state.address;
        if (!fromAddress) {
            throw new Error('No address connected');
        }
        const fromPubkey = new PublicKey(fromAddress);
        const toPubkey = new PublicKey(params.to);
        const instructions = [
            SystemProgram.transfer({
                fromPubkey,
                toPubkey,
                lamports: params.amountInLamports
            })
        ];
        const response = await this.requestCluster('getLatestBlockhash', [{}]);
        const { blockhash: recentBlockhash } = response.value;
        const messageV0 = new TransactionMessage({
            payerKey: fromPubkey,
            recentBlockhash,
            instructions
        }).compileToV0Message();
        const transactionV0 = new VersionedTransaction(messageV0);
        return transactionV0;
    }
    async getTransaction(transactionSignature) {
        const transaction = await this.requestCluster('getTransaction', [
            transactionSignature,
            { encoding: 'jsonParsed', commitment: 'confirmed' }
        ]);
        return transaction;
    }
    async watchTransaction(transactionSignature, callback) {
        return this.subscribeToCluster('signatureSubscribe', [transactionSignature], callback);
    }
    async getBalance(requestedAddress, currency = 'sol') {
        try {
            const address = requestedAddress ?? SolStoreUtil.state.address;
            const balance = await this.requestCluster('getBalance', [
                address,
                { commitment: 'processed' }
            ]);
            const BALANCE_VALUE_DECIMAL_DIVIDER = 1000000000;
            const formatted = currency === 'lamports'
                ? `${balance?.value || 0} lamports`
                : `${(balance?.value || 0) / BALANCE_VALUE_DECIMAL_DIVIDER} sol`;
            return {
                value: new BN(balance.value),
                formatted,
                decimals: balance.value / BALANCE_VALUE_DECIMAL_DIVIDER,
                symbol: currency
            };
        }
        catch (err) {
            SolStoreUtil.setError("Can't get balance");
            return {
                value: new BN(0),
                formatted: '0 sol',
                decimals: 0,
                symbol: currency
            };
        }
    }
    async getFeeForMessage(type, params) {
        const transaction = await this.constructTransaction(type, params);
        const message = transaction.compileMessage().serialize();
        const encodedMessage = message.toString('base64');
        const result = await this.requestCluster('getFeeForMessage', [encodedMessage]);
        return result;
    }
    async getProgramAccounts(requestedAddress, filters) {
        const programAccounts = await this.requestCluster('getProgramAccounts', [
            requestedAddress,
            { filters: filters ?? [], encoding: 'jsonParsed', withContext: true }
        ]);
        return programAccounts.value;
    }
    async getAllDomains(address) {
        const accounts = await this.getProgramAccounts(SolConstantsUtil.NAME_PROGRAM_ID.toBase58(), [
            {
                memcmp: {
                    offset: 32,
                    bytes: address
                }
            },
            {
                memcmp: {
                    offset: 0,
                    bytes: SolConstantsUtil.ROOT_DOMAIN_ACCOUNT.toBase58()
                }
            }
        ]);
        return accounts.map(({ pubkey }) => pubkey);
    }
    async getAccount(nameAccountKey, encoding = 'base58') {
        const address = nameAccountKey ?? SolStoreUtil.state.address;
        if (!address) {
            throw new Error('No address supplied and none connected');
        }
        const response = await this.requestCluster('getAccountInfo', [
            address,
            {
                encoding
            }
        ]);
        if (!response) {
            throw new Error('Invalid name account provided');
        }
        const { value: nameAccount } = response;
        return nameAccount;
    }
    async getBlock(slot) {
        const block = await this.requestCluster('getBlock', [slot]);
        return block;
    }
    async getAddressFromDomain(domain) {
        const hashed = getHashedName(domain.replace('.sol', ''));
        const nameAccountKey = await getNameAccountKey(hashed, undefined, SolConstantsUtil.ROOT_DOMAIN_ACCOUNT);
        const ownerDataRaw = await this.getAccount(nameAccountKey.toBase58(), 'base64');
        if (!ownerDataRaw) {
            return null;
        }
        const ownerData = borsh.deserializeUnchecked(NameRegistry.schema, NameRegistry, Buffer.from(String(ownerDataRaw.data[0]), 'base64'));
        return ownerData.owner.toBase58();
    }
    async request(method, params) {
        return (await this.getProvider()).request({ method, params });
    }
    async subscribeToCluster(method, params, callback) {
        const id = await registerListener(method, params, callback);
        return () => {
            unregisterListener(id);
        };
    }
    async requestCluster(method, params) {
        const cluster = SolStoreUtil.getCluster();
        const { endpoint } = cluster;
        const res = await fetch(endpoint, {
            method: 'post',
            body: JSON.stringify({
                method,
                params,
                jsonrpc: '2.0',
                id: SolStoreUtil.getNewRequestId()
            }),
            headers: {
                'Content-Type': 'application/json'
            }
        }).then(async (httpRes) => {
            const json = await httpRes.json();
            return json;
        });
        return res.result;
    }
}
//# sourceMappingURL=baseConnector.js.map