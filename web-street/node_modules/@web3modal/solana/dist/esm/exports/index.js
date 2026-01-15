import { ConstantsUtil } from '@web3modal/scaffold-utils';
import { Web3Modal } from '../src/client.js';
export { defaultSolanaConfig } from '../src/utils/defaultConfig.js';
export function createWeb3Modal(options) {
    return new Web3Modal({
        ...options,
        defaultChain: undefined,
        _sdkVersion: `html-solana-${ConstantsUtil.VERSION}`
    });
}
//# sourceMappingURL=index.js.map