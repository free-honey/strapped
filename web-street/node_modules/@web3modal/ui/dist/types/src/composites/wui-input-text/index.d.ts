import { LitElement } from 'lit';
import '../../components/wui-icon/index.js';
import type { IconType, InputType, SizeType, SpacingType } from '../../utils/TypeUtil.js';
export declare class WuiInputText extends LitElement {
    static styles: import("lit").CSSResult[];
    inputElementRef: import("lit-html/directives/ref.js").Ref<HTMLInputElement>;
    size: Exclude<SizeType, 'inherit' | 'xs' | 'xxs'>;
    icon?: IconType;
    disabled: boolean;
    placeholder: string;
    type: InputType;
    keyHint?: HTMLInputElement['enterKeyHint'];
    value?: string;
    inputRightPadding?: SpacingType;
    render(): import("lit-html").TemplateResult<1>;
    private templateIcon;
    private dispatchInputChangeEvent;
}
declare global {
    interface HTMLElementTagNameMap {
        'wui-input-text': WuiInputText;
    }
}
