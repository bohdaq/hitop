// The OpenAPI editor page: a spec (YAML or JSON) as text, checked as you type with the rules of
// `hitop openapi lint` (hitop-core, through hitop-wasm), opened from a file or a URL, saved as a
// file, imported as a collection or synced into one. The text is kept in this browser only
// (localStorage hitop_openapi_draft); it is never uploaded.
import { LitElement, html, nothing } from "lit";
import { icon } from "../icons.js";
import { IS_TAURI } from "../platform/index.js";
import { openapiRecord } from "./openapi-dialogs.js";
import "./code-editor.js";

const KEY = "hitop_openapi_draft";
const CHECK_AFTER_MS = 300;
const SHOWN = 200;
const EXAMPLE = `openapi: 3.1.0
info:
  title: My API
  version: "1.0"
servers:
  - url: https://api.example.com/v1
paths:
  /items/{id}:
    get:
      operationId: getItem
      summary: One item
      parameters:
        - name: id
          in: path
          required: true
          schema: {type: string}
      responses:
        "200":
          description: The item
          content:
            application/json:
              schema: {$ref: "#/components/schemas/Item"}
components:
  schemas:
    Item:
      type: object
      required: [id]
      properties:
        id: {type: string}
        name: {type: string}
`;

const plural = (n, one) => `${n} ${one}${n === 1 ? "" : "s"}`;

function loadDraft() {
  try {
    const d = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (typeof d?.text === "string") return { text: d.text, name: typeof d.name === "string" ? d.name : "" };
  } catch {
    // Unreadable or blocked storage: start empty.
  }
  return { text: "", name: "" };
}

class HitopOpenapiEditor extends LitElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    app: { attribute: false },
    text: { state: true },
    name: { state: true },
    result: { state: true },
    error: { state: true },
    kept: { state: true },
    busy: { state: true },
  };

  constructor() {
    super();
    Object.assign(this, loadDraft());
    this.result = null;
    this.error = "";
    this.kept = true;
    this.busy = false;
    this.checks = 0;
  }

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.check();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    // Nothing typed is lost between the last key and leaving the page.
    clearTimeout(this.timer);
    this.keep();
  }

  keep() {
    try {
      localStorage.setItem(KEY, JSON.stringify({ text: this.text, name: this.name }));
      this.kept = true;
    } catch {
      this.kept = false;
    }
  }

  edited(text) {
    this.text = text;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.keep();
      this.check();
    }, CHECK_AFTER_MS);
  }

  async check() {
    const n = ++this.checks;
    if (!this.text.trim()) {
      this.result = null;
      return;
    }
    try {
      const result = await this.store.lintOpenapi(this.text);
      // Only the newest text's result is shown.
      if (n === this.checks) this.result = result;
    } catch (e) {
      if (n === this.checks) this.result = { problems: [{ rule: "", severity: "error", location: "", message: String(e?.message ?? e) }], errors: 1, warnings: 0 };
    }
  }

  // Another text in the editor; the one that was there can come back.
  replace(text, name, message) {
    const before = { text: this.text, name: this.name };
    const set = (d) => {
      Object.assign(this, d);
      clearTimeout(this.timer);
      this.error = "";
      this.keep();
      this.check();
    };
    set({ text, name });
    if (before.text.trim() && before.text !== text) this.app.toast(message, { label: "Undo", run: () => set(before) });
  }

  async openFile(file) {
    if (!file) return;
    try {
      this.replace(await file.text(), file.name, `Opened ${file.name}`);
    } catch (e) {
      this.error = `Couldn't read ${file.name}: ${e?.message ?? e}`;
    }
  }

  async openUrl() {
    const url = this.querySelector("#openapi-url").value.trim();
    if (!/^https?:\/\//.test(url)) {
      this.error = "Enter the spec's URL, starting with http:// or https://";
      return;
    }
    this.busy = true;
    try {
      const text = await this.store.fetchSpec(url);
      this.replace(text, url.split(/[?#]/)[0].split("/").pop() || "openapi.yaml", `Loaded ${url}`);
    } catch (e) {
      this.error = `Couldn't load the spec: ${e?.message ?? e}`;
    }
    this.busy = false;
  }

  get json() {
    return this.text.trimStart().startsWith("{");
  }

  save() {
    const name = this.name || (this.json ? "openapi.json" : "openapi.yaml");
    this.app.download(name, this.text, this.json ? "application/json" : "application/yaml");
  }

  async importIt() {
    try {
      const report = await this.store.importOpenapi({ text: this.text });
      this.app.openDialog("openapi-report", { report });
    } catch (e) {
      this.error = `Couldn't import the spec: ${e?.message ?? e}`;
    }
  }

  sync(id) {
    const col = this.store.visibleCollections.find((c) => String(c.id) === id);
    if (col) this.app.openDialog("openapi-sync", { id: col.id, url: "", text: this.text });
  }

  problemList(r) {
    if (!r.problems.length) return html`<p class="hint">No problems found.</p>`;
    const editor = () => this.querySelector("hitop-code-editor");
    return html`
      <ul class="openapi-problems" aria-label="Problems">
        ${r.problems.slice(0, SHOWN).map(
          (p) => html`<li data-severity=${p.severity}>
            <button type="button" ?disabled=${!p.line} @click=${() => editor()?.goToLine(p.line)}>
              ${icon(p.severity === "error" ? "warning-circle" : "warning")}
              <span class="visually-hidden">${p.severity === "error" ? "Error" : "Warning"}</span>
              <span class="openapi-problem-text">${p.message}</span>
              <span class="hint">${p.line ? `line ${p.line} · ` : ""}${p.rule}</span>
            </button>
          </li>`,
        )}
      </ul>
      ${r.problems.length > SHOWN ? html`<p class="hint">The first ${SHOWN} of ${r.problems.length}.</p>` : nothing}`;
  }

  render() {
    const r = this.result;
    const empty = !this.text.trim();
    const imported = this.store.visibleCollections.filter((c) => openapiRecord(c));
    return html`
      <div class="page openapi-editor" @dragover=${(e) => e.preventDefault()} @drop=${(e) => (e.preventDefault(), this.openFile(e.dataTransfer?.files?.[0]))}>
        <div class="page-header">
          <h1>OpenAPI editor</h1>
          <div class="page-actions">
            <label class="file-pick">
              <input type="file" class="visually-hidden" accept=".json,.yaml,.yml,application/json,application/yaml,text/yaml"
                @change=${(e) => (this.openFile(e.target.files?.[0]), (e.target.value = ""))} />
              <span class="file-button">${icon("folder-open")} Open a file</span>
            </label>
            <lit-material-button variant="outlined" ?disabled=${empty} @click=${() => this.save()}>${icon("file-arrow-down")} Save as a file</lit-material-button>
            <lit-material-button variant="filled" ?disabled=${empty} @click=${() => this.importIt()}>Import as a collection</lit-material-button>
          </div>
        </div>
        <p class="page-lead">An OpenAPI 3.0 or 3.1 spec in YAML or JSON, checked as you type. It stays in ${IS_TAURI ? "this app" : "this browser"}.</p>
        <form class="openapi-url" @submit=${(e) => (e.preventDefault(), this.openUrl())}>
          <lit-material-text-field id="openapi-url" variant="outlined" type="url" label="Load from a URL" placeholder="https://api.example.com/openapi.yaml"></lit-material-text-field>
          <lit-material-button variant="outlined" type="submit" ?disabled=${this.busy}>${this.busy ? "Loading…" : "Load"}</lit-material-button>
          ${imported.length
            ? html`<label class="openapi-sync">Update a collection from this spec
                <select ?disabled=${empty} @change=${(e) => (this.sync(e.target.value), (e.target.value = ""))}>
                  <option value="">Choose…</option>
                  ${imported.map((c) => html`<option value=${String(c.id)}>${c.name}</option>`)}
                </select>
              </label>`
            : nothing}
        </form>
        ${this.error ? html`<p class="dialog-note warn" role="alert">${this.error}</p>` : nothing}
        ${this.kept ? nothing : html`<p class="dialog-note warn" role="alert">This spec is too large to keep here. Save it as a file before you leave.</p>`}
        <div class="openapi-panes">
          <hitop-code-editor language="yaml" label=${this.name || "Spec"} placeholder="Type or paste a spec, or drop a file here" .value=${this.text}
            .problems=${r?.problems ?? []} @input=${(e) => this.edited(e.target.value)}></hitop-code-editor>
          <section class="openapi-check" aria-labelledby="openapi-check-title">
            <h2 id="openapi-check-title">Check</h2>
            <p role="status" class="openapi-count">${r ? `${plural(r.errors, "error")}, ${plural(r.warnings, "warning")}` : ""}</p>
            ${r
              ? this.problemList(r)
              : html`<p class="hint">Nothing to check yet.</p><lit-material-button variant="text" @click=${() => this.replace(EXAMPLE, "openapi.yaml", "Started from the example")}>Start from an example</lit-material-button>`}
          </section>
        </div>
      </div>`;
  }
}

customElements.define("hitop-openapi-editor", HitopOpenapiEditor);
