// A CodeMirror 6 editor for scripts and request bodies: syntax highlighting
// on the site's colours, and syntax errors marked as you type. CodeMirror
// itself (code-editor-lib.js) loads with the first editor shown.
//   <hitop-code-editor language="javascript|json|text" label="..." .value=${...} @input=${...}>
// Fires `input` (bubbling) with `.value` set after every edit.
import { LitElement, html } from "lit";

export class HitopCodeEditor extends LitElement {
  static properties = {
    value: { type: String },
    language: { type: String },
    label: { type: String },
    placeholderText: { type: String, attribute: "placeholder" },
  };

  constructor() {
    super();
    this.value = "";
    this.language = "text";
    this.label = "";
    this.placeholderText = "";
    // Typing in CodeMirror fires the browser's own input events; only this
    // element's own `input` (with .value set) should reach listeners.
    this.addEventListener("input", (e) => {
      if (e.target !== this) e.stopImmediatePropagation();
    });
  }

  createRenderRoot() {
    return this;
  }

  render() {
    return html`<span class="code-editor-label" id=${`${this.labelId}`}>${this.label}</span><div class="code-editor-host"></div>`;
  }

  get labelId() {
    this._labelId ??= `code-editor-${Math.random().toString(36).slice(2)}`;
    return this._labelId;
  }

  async firstUpdated() {
    const cm = (this.cm ??= await import("./code-editor-lib.js"));
    if (!this.isConnected || this.view) return;
    const { EditorState, Compartment, EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection, placeholder, defaultKeymap, history, historyKeymap, indentWithTab, bracketMatching, indentOnInput, syntaxHighlighting, highlight, theme, languageExtensions } = cm;
    this.languageSlot = new Compartment();
    this.view = new EditorView({
      parent: this.querySelector(".code-editor-host"),
      state: EditorState.create({
        doc: this.value ?? "",
        extensions: [
          lineNumbers(),
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          highlightActiveLine(),
          syntaxHighlighting(highlight),
          keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({ "aria-labelledby": this.labelId, "aria-multiline": "true" }),
          placeholder(this.placeholderText),
          theme,
          this.languageSlot.of(languageExtensions(this.language)),
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) return;
            this.value = update.state.doc.toString();
            this.lastEmitted = this.value;
            this.dispatchEvent(new Event("input", { bubbles: true }));
          }),
        ],
      }),
    });
  }

  updated(changed) {
    if (!this.view) return;
    if (changed.has("language")) {
      this.view.dispatch({ effects: this.languageSlot.reconfigure(this.cm.languageExtensions(this.language)) });
    }
    // Text set from outside (another tab, history): replace the document,
    // but never echo back what the user just typed (it would move the caret).
    if (changed.has("value") && this.value !== this.lastEmitted && this.value !== this.view.state.doc.toString()) {
      this.view.dispatch({ changes: { from: 0, to: this.view.state.doc.length, insert: this.value ?? "" } });
    }
  }

  focus() {
    this.view?.focus();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.view?.destroy();
    this.view = null;
  }

  connectedCallback() {
    super.connectedCallback();
    // Re-created after a move in the DOM (Lit may detach and reattach).
    if (this.hasUpdated && !this.view) {
      this.querySelector(".code-editor-host")?.replaceChildren();
      this.firstUpdated();
    }
  }
}

customElements.define("hitop-code-editor", HitopCodeEditor);
