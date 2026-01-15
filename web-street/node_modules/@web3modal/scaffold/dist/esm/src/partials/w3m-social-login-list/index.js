var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
import { ConnectorController } from '@web3modal/core';
import { customElement } from '@web3modal/ui';
import { LitElement, html } from 'lit';
import { state } from 'lit/decorators.js';
import styles from './styles.js';
let W3mSocialLoginList = class W3mSocialLoginList extends LitElement {
    constructor() {
        super();
        this.unsubscribe = [];
        this.connectors = ConnectorController.state.connectors;
        this.connector = this.connectors.find(c => c.type === 'AUTH');
        this.unsubscribe.push(ConnectorController.subscribeKey('connectors', val => {
            this.connectors = val;
            this.connector = this.connectors.find(c => c.type === 'AUTH');
        }));
    }
    disconnectedCallback() {
        this.unsubscribe.forEach(unsubscribe => unsubscribe());
    }
    render() {
        if (!this.connector?.socials) {
            return null;
        }
        return html ` <wui-flex flexDirection="column" gap="xs">
      ${this.connector.socials.map(social => html `<wui-list-social name=${social} logo=${social}></wui-list-social>`)}
    </wui-flex>`;
    }
};
W3mSocialLoginList.styles = styles;
__decorate([
    state()
], W3mSocialLoginList.prototype, "connectors", void 0);
W3mSocialLoginList = __decorate([
    customElement('w3m-social-login-list')
], W3mSocialLoginList);
export { W3mSocialLoginList };
//# sourceMappingURL=index.js.map