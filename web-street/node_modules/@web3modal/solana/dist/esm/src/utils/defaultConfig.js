import '@web3modal/polyfills';
export function defaultSolanaConfig(options) {
    const { enableInjected = true, metadata } = options;
    let injectedProvider = undefined;
    const providers = { metadata };
    function getInjectedProvider() {
        if (injectedProvider) {
            return injectedProvider;
        }
        if (typeof window === 'undefined') {
            return undefined;
        }
        if (!window.solana) {
            return undefined;
        }
        injectedProvider = window.solana;
        return injectedProvider;
    }
    if (enableInjected) {
        providers.injected = getInjectedProvider();
    }
    return providers;
}
//# sourceMappingURL=defaultConfig.js.map