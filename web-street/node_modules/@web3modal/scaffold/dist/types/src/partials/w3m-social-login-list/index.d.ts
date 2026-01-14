import { LitElement } from 'lit';
export declare class W3mSocialLoginList extends LitElement {
    static styles: import("lit").CSSResult;
    private unsubscribe;
    private connectors;
    private connector;
    constructor();
    disconnectedCallback(): void;
    render(): import("lit-html").TemplateResult<1> | null;
}
declare global {
    interface HTMLElementTagNameMap {
        'w3m-social-login-list': W3mSocialLoginList;
    }
}
