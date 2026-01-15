import { LitElement } from 'lit';
export declare class W3mConnectCoinbaseWidget extends LitElement {
    private unsubscribe;
    private connectors;
    constructor();
    disconnectedCallback(): void;
    render(): import("lit-html").TemplateResult<1> | null;
    private onCoinbaseConnector;
    private onConnector;
}
declare global {
    interface HTMLElementTagNameMap {
        'w3m-connect-coinbase-widget': W3mConnectCoinbaseWidget;
    }
}
