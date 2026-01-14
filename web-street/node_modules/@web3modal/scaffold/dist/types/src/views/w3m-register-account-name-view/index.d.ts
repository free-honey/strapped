import { LitElement } from 'lit';
export declare class W3mRegisterAccountNameView extends LitElement {
    static styles: import("lit").CSSResult;
    private formRef;
    private usubscribe;
    errorMessage?: string;
    private name;
    private error;
    private loading;
    private suggestions;
    private registered;
    constructor();
    firstUpdated(): void;
    disconnectedCallback(): void;
    render(): import("lit-html").TemplateResult<1>;
    private submitButtonTemplate;
    private onDebouncedNameInputChange;
    private onSelectSuggestion;
    private onNameInputChange;
    private nameSuggestionTagTemplate;
    private templateSuggestions;
    private availableNameTemplate;
    private isAllowedToSubmit;
    private onSubmitName;
    private onEnterKey;
}
declare global {
    interface HTMLElementTagNameMap {
        'w3m-register-account-name-view': W3mRegisterAccountNameView;
    }
}
