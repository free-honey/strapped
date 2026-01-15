import { Web3Modal } from '../src/client.js';
import { ConstantsUtil } from '@web3modal/scaffold-utils';
export { defaultWagmiConfig } from '../src/utils/defaultWagmiCoreConfig.js';
export { authConnector } from '../src/connectors/AuthConnectorExport.js';
export function createWeb3Modal(options) {
    return new Web3Modal({ ...options, _sdkVersion: `html-wagmi-${ConstantsUtil.VERSION}` });
}
//# sourceMappingURL=index.js.map