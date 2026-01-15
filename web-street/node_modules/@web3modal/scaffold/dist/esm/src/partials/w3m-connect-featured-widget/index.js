var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
import { ApiController, AssetUtil, ConnectorController, CoreHelperUtil, RouterController, StorageUtil } from '@web3modal/core';
import { customElement } from '@web3modal/ui';
import { LitElement, html } from 'lit';
import { state } from 'lit/decorators.js';
import { ifDefined } from 'lit/directives/if-defined.js';
let W3mConnectFeaturedWidget = class W3mConnectFeaturedWidget extends LitElement {
    constructor() {
        super();
        this.unsubscribe = [];
        this.connectors = ConnectorController.state.connectors;
        this.unsubscribe.push(ConnectorController.subscribeKey('connectors', val => (this.connectors = val)));
    }
    disconnectedCallback() {
        this.unsubscribe.forEach(unsubscribe => unsubscribe());
    }
    render() {
        const { featured } = ApiController.state;
        if (!featured.length) {
            this.style.cssText = `display: none`;
            return null;
        }
        const wallets = this.filterOutDuplicateWallets(featured);
        return html `
      <wui-flex flexDirection="column" gap="xs">
        ${wallets.map(wallet => html `
            <wui-list-wallet
              imageSrc=${ifDefined(AssetUtil.getWalletImage(wallet))}
              name=${wallet.name ?? 'Unknown'}
              @click=${() => this.onConnectWallet(wallet)}
            >
            </wui-list-wallet>
          `)}
      </wui-flex>
    `;
    }
    filterOutDuplicateWallets(wallets) {
        const recent = StorageUtil.getRecentWallets();
        const connectorRDNSs = this.connectors
            .map(connector => connector.info?.rdns)
            .filter(Boolean);
        const recentRDNSs = recent.map(wallet => wallet.rdns).filter(Boolean);
        const allRDNSs = connectorRDNSs.concat(recentRDNSs);
        if (allRDNSs.includes('io.metamask.mobile') && CoreHelperUtil.isMobile()) {
            const index = allRDNSs.indexOf('io.metamask.mobile');
            allRDNSs[index] = 'io.metamask';
        }
        const filtered = wallets.filter(wallet => !allRDNSs.includes(String(wallet?.rdns)));
        return filtered;
    }
    onConnectWallet(wallet) {
        RouterController.push('ConnectingWalletConnect', { wallet });
    }
};
__decorate([
    state()
], W3mConnectFeaturedWidget.prototype, "connectors", void 0);
W3mConnectFeaturedWidget = __decorate([
    customElement('w3m-connect-featured-widget')
], W3mConnectFeaturedWidget);
export { W3mConnectFeaturedWidget };
//# sourceMappingURL=index.js.map