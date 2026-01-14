import type { WalletConnectAppMetadata } from './walletConnectConnector.js';
import type UniversalProvider from '@walletconnect/universal-provider';
export declare class UniversalProviderFactory {
    protected static provider: UniversalProvider | undefined;
    protected static relayerRegion: string | undefined;
    protected static projectId: string | undefined;
    protected static metadata: WalletConnectAppMetadata | undefined;
    static setSettings(params: {
        projectId: string;
        relayerRegion: string;
        metadata: WalletConnectAppMetadata;
        qrcode: boolean;
    }): void;
    static init(): Promise<void>;
    static getProvider(): Promise<UniversalProvider>;
}
