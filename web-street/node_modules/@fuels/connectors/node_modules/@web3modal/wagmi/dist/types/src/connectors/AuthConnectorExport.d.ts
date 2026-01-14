import type { CreateConfigParameters } from '@wagmi/core';
import type { SocialProvider } from '@web3modal/scaffold-utils';
interface W3mFrameProviderOptions {
    projectId: string;
}
export type AuthParameters = {
    chains?: CreateConfigParameters['chains'];
    options: W3mFrameProviderOptions;
    socials?: SocialProvider[];
    showWallets?: boolean;
    walletFeatures?: boolean;
    email?: boolean;
};
export declare function authConnector(parameters: AuthParameters): import("@wagmi/core").CreateConnectorFn<import("packages/wallet/dist/types/index.js").W3mFrameProvider, {
    provider?: import("packages/wallet/dist/types/index.js").W3mFrameProvider | undefined;
}, Record<string, unknown>>;
export {};
