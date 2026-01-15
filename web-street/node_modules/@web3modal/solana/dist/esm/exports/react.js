'use client';
import { useSnapshot } from 'valtio';
import { ConstantsUtil } from '@web3modal/scaffold-utils';
import { getWeb3Modal } from '@web3modal/scaffold-react';
import { Web3Modal } from '../src/client.js';
import { SolStoreUtil } from '../src/utils/scaffold/index.js';
let modal = undefined;
export function createWeb3Modal(options) {
    if (!modal) {
        modal = new Web3Modal({
            ...options,
            _sdkVersion: `react-solana-${ConstantsUtil.VERSION}`
        });
    }
    getWeb3Modal(modal);
    return modal;
}
export function useWeb3ModalProvider() {
    const { provider, providerType, connection } = useSnapshot(SolStoreUtil.state);
    return {
        walletProvider: provider,
        walletProviderType: providerType,
        connection
    };
}
export function useDisconnect() {
    function disconnect() {
        modal?.disconnect();
    }
    return {
        disconnect
    };
}
export function useWeb3ModalAccount() {
    const { address, isConnected, chainId, currentChain } = useSnapshot(SolStoreUtil.state);
    return {
        address,
        isConnected,
        currentChain,
        chainId
    };
}
export { useWeb3ModalTheme, useWeb3Modal, useWeb3ModalState, useWeb3ModalEvents } from '@web3modal/scaffold-react';
export { defaultSolanaConfig } from '../src/utils/defaultConfig.js';
//# sourceMappingURL=react.js.map