var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
import { AssetUtil, ConnectionController, ConnectorController, EventsController, ModalController, OptionsController, RouterController, StorageUtil } from '@web3modal/core';
import { ConstantsUtil } from '@web3modal/scaffold-utils';
import { customElement } from '@web3modal/ui';
import { LitElement, html } from 'lit';
import { state } from 'lit/decorators.js';
import { ifDefined } from 'lit/directives/if-defined.js';
let W3mConnectCoinbaseWidget = class W3mConnectCoinbaseWidget extends LitElement {
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
        const coinbaseConnector = this.connectors.find(connector => connector.id === 'coinbaseWalletSDK');
        if (!coinbaseConnector) {
            this.style.cssText = `display: none`;
            return null;
        }
        return html `
      <wui-flex flexDirection="column" gap="xs">
        <wui-list-wallet
          imageSrc=${ifDefined(AssetUtil.getConnectorImage(coinbaseConnector))}
          .installed=${true}
          name=${ifDefined(coinbaseConnector.name)}
          data-testid=${`wallet-selector-${coinbaseConnector.id}`}
          @click=${() => this.onConnector(coinbaseConnector)}
        >
        </wui-list-wallet>
      </wui-flex>
    `;
    }
    async onCoinbaseConnector(connector) {
        try {
            ConnectionController.setWcError(false);
            if (connector.imageUrl) {
                StorageUtil.setConnectedWalletImageUrl(connector.imageUrl);
            }
            await ConnectionController.connectExternal(connector);
            if (OptionsController.state.isSiweEnabled) {
                RouterController.push('ConnectingSiwe');
            }
            else {
                ModalController.close();
            }
            EventsController.sendEvent({
                type: 'track',
                event: 'CONNECT_SUCCESS',
                properties: { method: 'browser', name: connector.name || 'Unknown' }
            });
        }
        catch (error) {
            EventsController.sendEvent({
                type: 'track',
                event: 'CONNECT_ERROR',
                properties: { message: error?.message ?? 'Unknown' }
            });
            ConnectionController.setWcError(true);
        }
    }
    onConnector(connector) {
        RouterController.push('ConnectingExternal', { connector });
        if (connector.id === ConstantsUtil.COINBASE_SDK_CONNECTOR_ID) {
            this.onCoinbaseConnector(connector);
        }
    }
};
__decorate([
    state()
], W3mConnectCoinbaseWidget.prototype, "connectors", void 0);
W3mConnectCoinbaseWidget = __decorate([
    customElement('w3m-connect-coinbase-widget')
], W3mConnectCoinbaseWidget);
export { W3mConnectCoinbaseWidget };
//# sourceMappingURL=index.js.map