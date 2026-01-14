import { PublicKey, Transaction, VersionedTransaction } from '@solana/web3.js';
import BN from 'bn.js';
import type { ConfirmOptions, Signer, TransactionSignature } from '@solana/web3.js';
import type { BlockResult, AccountInfo, ClusterRequestMethods, ClusterSubscribeRequestMethods, FilterObject, RequestMethods, TransactionArgs, TransactionType } from '../utils/scaffold/index.js';
export interface Connector {
    id: string;
    name: string;
    ready: boolean;
    getConnectorName: () => string;
    disconnect: () => Promise<void>;
    connect: () => Promise<string>;
    signMessage: (message: Uint8Array) => Promise<string>;
    signTransaction: (transaction: Transaction | VersionedTransaction) => Promise<{
        signatures: {
            signature: string;
        }[];
    }>;
    sendTransaction: (transaction: Transaction | VersionedTransaction) => Promise<string>;
    signAndSendTransaction: (transaction: Transaction | VersionedTransaction, signers: Signer[], confirmOptions?: ConfirmOptions) => Promise<TransactionSignature>;
    getAccount: (requestedAddress?: string, encoding?: 'base58' | 'base64' | 'jsonParsed') => Promise<AccountInfo | null>;
    getBalance: (requestedAddress: string) => Promise<{
        formatted: string;
        value: BN;
        decimals: number;
        symbol: string;
    }>;
    getTransaction: (transactionSignature: string) => Promise<ClusterRequestMethods['getTransaction']['returns']>;
    watchTransaction: (transactionSignature: string, callback: (params: unknown) => void) => Promise<() => void>;
    getAddressFromDomain: (address: string) => Promise<string | null>;
    getBlock: (slot: number) => Promise<BlockResult | null>;
    getFeeForMessage: <Type extends TransactionType>(type: Type, params: TransactionArgs[Type]['params']) => Promise<number>;
}
type Currency = 'lamports' | 'sol';
export declare class BaseConnector {
    getConnectorName(): string;
    get publicKey(): PublicKey;
    protected getProvider(): Promise<{
        request: (args: any) => any;
    }>;
    protected constructTransaction<TransType extends keyof TransactionArgs>(type: TransType, params: TransactionArgs[TransType]['params']): Promise<Transaction>;
    protected constructVersionedTransaction(params: TransactionArgs['transfer']['params']): Promise<VersionedTransaction>;
    getTransaction(transactionSignature: string): Promise<import("../utils/scaffold/SolanaTypesUtil.js").TransactionResult | null>;
    watchTransaction(transactionSignature: string, callback: (params: Transaction | number) => void): Promise<() => void>;
    getBalance(requestedAddress: string, currency?: Currency): Promise<{
        value: BN;
        formatted: string;
        decimals: number;
        symbol: Currency;
    }>;
    getFeeForMessage<TransType extends keyof TransactionArgs>(type: TransType, params: TransactionArgs[TransType]['params']): Promise<number>;
    getProgramAccounts(requestedAddress: string, filters?: FilterObject[]): Promise<{
        account: AccountInfo;
    }[]>;
    getAllDomains(address: string): Promise<any[]>;
    getAccount(nameAccountKey?: string, encoding?: 'base58' | 'base64' | 'jsonParsed'): Promise<AccountInfo | null>;
    getBlock(slot: number): Promise<BlockResult | null>;
    getAddressFromDomain(domain: string): Promise<any>;
    request<Method extends keyof RequestMethods>(method: Method, params: RequestMethods[Method]['params']): Promise<RequestMethods[Method]['returns']>;
    subscribeToCluster<Method extends keyof ClusterSubscribeRequestMethods>(method: Method, params: ClusterSubscribeRequestMethods[Method]['params'], callback: (cb_params: Transaction | number) => void): Promise<() => void>;
    requestCluster<Method extends keyof ClusterRequestMethods>(method: Method, params: ClusterRequestMethods[Method]['params']): Promise<ClusterRequestMethods[Method]['returns']>;
}
export {};
