import { LitElement } from 'lit';
import '../../components/wui-text/index.js';
export declare class WuiListButton extends LitElement {
    static styles: import("lit").CSSResult[];
    text: string;
    disabled: boolean;
    render(): import("lit-html").TemplateResult<1>;
}
declare global {
    interface HTMLElementTagNameMap {
        'wui-list-button': WuiListButton;
    }
}
