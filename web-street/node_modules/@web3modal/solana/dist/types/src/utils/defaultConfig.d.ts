/// <reference types="node" />
import '@web3modal/polyfills';
import type { Chain, Metadata, Provider, ProviderType } from './scaffold/index.js';
declare global {
    interface Navigator {
        brave?: {
            isBrave(): boolean;
        };
    }
    interface Window {
        originalSolana?: Record<string, unknown>;
        solana?: Provider;
        solflare?: {
            solana: Provider & {
                isSoflare: boolean;
            };
        };
        backpack?: {
            solana: Provider;
        };
        trustWallet?: {
            solana: Provider;
        };
        phantom?: {
            solana: Provider & {
                isPhantom: boolean;
            };
        };
        getHashedName: (name: string) => Buffer;
    }
}
export interface ConfigOptions {
    projectId?: string;
    chains: Chain[];
    enableInjected?: boolean;
    rpcUrl?: string;
    defaultChainId?: number;
    metadata: Metadata;
}
export declare function defaultSolanaConfig(options: ConfigOptions): ProviderType;
