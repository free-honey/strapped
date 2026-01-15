import { LitElement } from 'lit';
export declare class W3mConnectFeaturedWidget extends LitElement {
    private unsubscribe;
    private connectors;
    constructor();
    disconnectedCallback(): void;
    render(): import("lit-html").TemplateResult<1> | null;
    private filterOutDuplicateWallets;
    private onConnectWallet;
}
declare global {
    interface HTMLElementTagNameMap {
        'w3m-connect-featured-widget': W3mConnectFeaturedWidget;
    }
}
