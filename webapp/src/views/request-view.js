// One request tab: method, URL, send; headers, body and scripts; the response.
import { LitElement, html, nothing } from "lit";
import { guard } from "lit/directives/guard.js";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import hljs from "highlight.js/lib/core";
import json from "highlight.js/lib/languages/json";
import xml from "highlight.js/lib/languages/xml";
import { getStatusText } from "../../../frontend/src/services/httpService.jsx";
import { usesProxy } from "../../../frontend/src/services/transportService.jsx";
import { icon } from "../icons.js";
import { AUTH_TYPES, emptyAuth, parseQuery, withQuery } from "../features.js";
import { BODY_METHODS, METHODS } from "../store.js";

hljs.registerLanguage("json", json);
hljs.registerLanguage("xml", xml);

const PRE_HELP = `// Runs before the request. Available:
// url, method, headers, body, variables, context
// setHeader(name, value), setUrl(url), setBody(body)
// setContext(key, value), getContext(key), getVariable(key)
//
// Example: send the token a previous request saved
// setHeader('Authorization', 'Bearer ' + getContext('authToken'));`;

const POST_HELP = `// Runs after the response. Available:
// response (parsed JSON or text), responseText, responseHeaders,
// statusCode, variables, context
// getResponseValue('path.to.value'), getResponseHeader(name)
// setContext(key, value), getContext(key), getVariable(key)
//
// Example: keep a token for the next request
// setContext('authToken', getResponseValue('data.token'));
//
// Checks, shown under Tests and in collection runs:
// assert.status(200), assert.bodyContains('id'),
// assert.header('content-type', 'json'), assert.responseTime(500),
// assert.equal(getResponseValue('user.id'), 42), assert.ok(statusCode < 300)`;

const scriptHelp = (text) => html`<details class="script-help"><summary>What a script can use</summary><pre>${text}</pre></details>`;

// Editor sections, in tab order.
const PARAMS = 0, AUTH = 1, HEADERS = 2, BODY = 3, PRE = 4, POST = 5;

function highlighted(text, type) {
  const lang = type === "json" ? "json" : type === "html" || type === "xml" ? "xml" : null;
  if (!lang || text.length > 500_000) return null;
  try {
    return hljs.highlight(text, { language: lang }).value;
  } catch {
    return null;
  }
}

const bytes = (text) => {
  const n = new TextEncoder().encode(text).length;
  return n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`;
};

export const testList = (tests) => html`
  <ul class="tests">
    ${tests.map(
      (x) => html`<li class=${x.pass ? "pass" : "fail"}>
        <lit-material-status status=${x.pass ? "ok" : "error"}>${x.pass ? "Passed" : "Failed"}</lit-material-status>
        <span>${x.name}</span>${x.detail ? html`<span class="test-detail">${x.detail}</span>` : nothing}
      </li>`,
    )}
  </ul>`;

export class HitopRequest extends LitElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    app: { attribute: false },
    tab: { attribute: false },
    section: { state: true },
    responseSection: { state: true },
  };

  constructor() {
    super();
    this.section = 0;
    this.responseSection = 0;
  }

  createRenderRoot() {
    return this;
  }

  update(changed) {
    // A different tab starts on Headers, or on Body when it has one.
    if (changed.has("tab") && changed.get("tab")?.id !== this.tab?.id) {
      this.section = this.tab?.requestBody && BODY_METHODS.includes(this.tab.method) ? BODY : this.tab?.url?.includes("?") ? PARAMS : HEADERS;
      this.responseSection = 0;
    }
    // The header editor rebuilds its rows whenever `entries` is set, and Lit
    // sets object properties on every render, so the binding is guarded:
    // rows are handed over only when the headers changed outside the editor
    // (another tab, history, a save), never while typing in it.
    const json = JSON.stringify(this.tab?.headers ?? []);
    if (json !== this.headersJson) {
      this.headersJson = json;
      this.headerRows = this.headerEntries();
      this.headerRowsVersion = (this.headerRowsVersion ?? 0) + 1;
    }
    // Same for the query parameter editor, which follows the URL.
    const params = JSON.stringify(parseQuery(this.tab?.url ?? ""));
    if (params !== this.paramsJson) {
      this.paramsJson = params;
      this.paramRows = JSON.parse(params);
      this.paramRowsVersion = (this.paramRowsVersion ?? 0) + 1;
    }
    super.update(changed);
  }

  onParams(e) {
    const url = withQuery(this.tab.url, e.detail.entries);
    this.paramsJson = JSON.stringify(parseQuery(url));
    this.set({ url });
  }

  get auth() {
    return { ...emptyAuth(), ...(this.tab.auth || {}) };
  }

  setAuth(updates) {
    this.set({ auth: { ...this.auth, ...updates } });
  }

  renderAuth() {
    const a = this.auth;
    const field = (label, key, extra = {}) => html`<lit-material-text-field variant="outlined" label=${label} .value=${a[key] || ""}
      type=${extra.type ?? "text"} supporting-text=${extra.help ?? ""} @input=${(e) => this.setAuth({ [key]: e.target.value })}></lit-material-text-field>`;
    return html`
      <div class="auth">
        <lit-material-select variant="outlined" label="Type" .value=${a.type} @change=${(e) => this.setAuth({ type: e.target.value })}>
          ${AUTH_TYPES.map(([v, l]) => html`<lit-material-select-option value=${v}>${l}</lit-material-select-option>`)}
        </lit-material-select>
        ${a.type === "bearer" ? field("Token", "token", { help: "Sent as Authorization: Bearer <token>. ${variables} work here." }) : nothing}
        ${a.type === "basic"
          ? html`${field("Username", "username")} ${field("Password", "password", { type: "password", help: "Sent base64 encoded in Authorization: Basic." })}`
          : nothing}
        ${a.type === "apikey"
          ? html`${field("Name", "key")} ${field("Value", "value")}
              <lit-material-select variant="outlined" label="Send as" .value=${a.in} @change=${(e) => this.setAuth({ in: e.target.value })}>
                <lit-material-select-option value="header">Header</lit-material-select-option>
                <lit-material-select-option value="query">Query parameter</lit-material-select-option>
              </lit-material-select>`
          : nothing}
        ${a.type === "none" ? html`<p class="hint">Pick a type to add credentials. A header you set by hand with the same name wins.</p>` : nothing}
      </div>`;
  }

  set(updates) {
    this.store.updateTab(updates);
  }

  headerEntries() {
    return (this.tab.headers || []).filter((h) => h.name || h.value).map((h) => ({ key: h.name, value: h.value }));
  }

  onHeaders(e) {
    const rows = e.detail.entries.map((x) => ({ name: x.key, value: x.value }));
    const headers = rows.length ? rows : [{ name: "", value: "" }];
    this.headersJson = JSON.stringify(headers);
    this.set({ headers });
  }

  send(e) {
    e?.preventDefault();
    if (!this.tab.loading) this.store.send();
  }

  renderEditor() {
    const t = this.tab;
    const hasBody = BODY_METHODS.includes(t.method);
    const headerCount = this.headerEntries().length;
    const paramCount = parseQuery(t.url || "").length;
    const authType = t.auth?.type && t.auth.type !== "none" ? AUTH_TYPES.find(([v]) => v === t.auth.type)?.[1] : null;
    const labels = [
      paramCount ? `Params (${paramCount})` : "Params",
      authType ? `Auth ●` : "Auth",
      headerCount ? `Headers (${headerCount})` : "Headers",
      "Body",
      t.preRequestScript?.trim() ? "Pre-request script ●" : "Pre-request script",
      t.postRequestScript?.trim() ? "Post-request script ●" : "Post-request script",
    ];
    return html`
      <section class="editor" aria-label="Request details">
        <lit-material-tabs aria-label="Request details" .selected=${this.section} @change=${(e) => (this.section = e.target.selected)}>
          ${labels.map((label) => html`<lit-material-tab>${label}</lit-material-tab>`)}
        </lit-material-tabs>
        <div class="editor-panel">
          ${this.section === PARAMS
            ? html`<lit-material-key-value-editor
                key-label="Parameter"
                value-label="Value"
                add-label="Add parameter"
                .entries=${guard([this.paramRowsVersion], () => this.paramRows)}
                @change=${(e) => this.onParams(e)}
              ></lit-material-key-value-editor>
              <p class="hint">Kept in sync with the URL. Values are sent as typed, so encode special characters yourself or use \${variables}.</p>`
            : nothing}
          ${this.section === AUTH ? this.renderAuth() : nothing}
          ${this.section === HEADERS
            ? html`<lit-material-key-value-editor
                key-label="Header"
                value-label="Value"
                add-label="Add header"
                .entries=${guard([this.headerRowsVersion], () => this.headerRows)}
                @change=${(e) => this.onHeaders(e)}
              ></lit-material-key-value-editor>`
            : nothing}
          ${this.section === BODY
            ? hasBody
              ? html`<lit-material-code-field
                  label="Request body"
                  .value=${t.requestBody || ""}
                  @input=${(e) => this.set({ requestBody: e.target.value })}
                ></lit-material-code-field>`
              : html`<lit-material-empty-state size="compact" headline=${`${t.method} requests have no body`}>
                  Switch the method to POST, PUT or PATCH to send one.
                </lit-material-empty-state>`
            : nothing}
          ${this.section === PRE
            ? html`<lit-material-code-field label="Pre-request script" .value=${t.preRequestScript || ""} @input=${(e) => this.set({ preRequestScript: e.target.value })}></lit-material-code-field>
                ${scriptHelp(PRE_HELP)}`
            : nothing}
          ${this.section === POST
            ? html`<lit-material-code-field label="Post-request script" .value=${t.postRequestScript || ""} @input=${(e) => this.set({ postRequestScript: e.target.value })}></lit-material-code-field>
                ${scriptHelp(POST_HELP)}`
            : nothing}
        </div>
      </section>`;
  }

  renderResponse() {
    const t = this.tab;
    if (t.loading) {
      return html`<section class="response" aria-label="Response" aria-busy="true">
        <div class="skeleton bar"></div>
        <div class="skeleton block"></div>
      </section>`;
    }
    if (t.statusCode == null && (t.response == null || t.response === "")) {
      return html`<section class="response empty" aria-label="Response">
        <lit-material-empty-state size="compact" headline="Send a request to see the response">
          ${usesProxy(t.url || "https://example.com")
            ? "Requests to public APIs go through the HITOP proxy, so CORS doesn't block them. Requests to localhost go straight from your browser."
            : "The response, its headers and timing appear here."}
        </lit-material-empty-state>
      </section>`;
    }
    if (t.responseType === "error" || t.statusCode == null) {
      return html`<section class="response" aria-label="Response">
        <lit-material-status status="error">Request failed</lit-material-status>
        <p class="error-text">${String(t.response).replace(/^Error: /, "")}</p>
      </section>`;
    }
    const cls = t.statusCode < 300 ? "ok" : t.statusCode < 400 ? "info" : "bad";
    const headers = Object.entries(t.responseHeaders || {});
    const code = highlighted(t.response, t.responseType);
    const tests = t.tests || [];
    const passed = tests.filter((x) => x.pass).length;
    if (this.responseSection === 2 && !tests.length) this.responseSection = 0;
    return html`
      <section class="response" aria-label="Response">
        <div class="response-meta">
          <span class=${`status-chip ${cls}`}>${t.statusCode} ${getStatusText(t.statusCode) === "Unknown Status" ? "" : getStatusText(t.statusCode)}</span>
          ${t.durationMs != null ? html`<span>${t.durationMs} ms</span>` : nothing}
          <span>${bytes(t.response || "")}</span>
          ${t.truncated ? html`<span class="warn">Cut at 5 MB</span>` : nothing}
          ${tests.length ? html`<span class=${passed === tests.length ? "pass" : "warn"}>${passed} of ${tests.length} tests passed</span>` : nothing}
        </div>
        <lit-material-tabs aria-label="Response details" .selected=${this.responseSection} @change=${(e) => (this.responseSection = e.target.selected)}>
          <lit-material-tab>Body</lit-material-tab>
          <lit-material-tab>Headers (${headers.length})</lit-material-tab>
          ${tests.length ? html`<lit-material-tab>Tests (${passed}/${tests.length})</lit-material-tab>` : nothing}
        </lit-material-tabs>
        ${this.responseSection === 0
          ? t.response
            ? html`<lit-material-code-block class="response-body" label=${t.responseType.toUpperCase()}>${code ? unsafeHTML(code) : t.response}</lit-material-code-block>`
            : html`<p class="hint">No body${t.method === "HEAD" ? ": HEAD responses never have one" : ""}.</p>`
          : this.responseSection === 1
            ? html`<dl class="response-headers">
                ${headers.map(([k, v]) => html`<dt>${k}</dt><dd>${v}</dd>`)}
              </dl>`
            : testList(tests)}
      </section>`;
  }

  render() {
    const t = this.tab;
    if (!t) return nothing;
    return html`
      <form class="request-line" @submit=${(e) => this.send(e)}>
        <lit-material-select class="method" variant="outlined" label="Method" .value=${t.method} @change=${(e) => this.set({ method: e.target.value })}>
          ${METHODS.map((m) => html`<lit-material-select-option value=${m}>${m}</lit-material-select-option>`)}
        </lit-material-select>
        <lit-material-text-field
          class="url"
          variant="outlined"
          label="URL"
          type="url"
          placeholder="https://api.example.com/users or \${baseUrl}/users"
          .value=${t.url}
          @input=${(e) => this.set({ url: e.target.value })}
          @keydown=${(e) => e.key === "Enter" && this.send(e)}
        ></lit-material-text-field>
        <lit-material-button variant="filled" type="submit" ?disabled=${t.loading}>
          ${icon("paper-plane-tilt")} ${t.loading ? "Sending" : "Send"}
        </lit-material-button>
        <lit-material-icon-button aria-label="Save request" ?disabled=${!t.url} @click=${() => this.app.openDialog("save", this.store.saveTarget())}>
          ${icon("floppy-disk")}
        </lit-material-icon-button>
      </form>
      ${this.renderEditor()} ${this.renderResponse()}`;
  }
}

customElements.define("hitop-request", HitopRequest);
