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
import { platform } from "../platform/index.js";
import { AUTH_TYPES, emptyAuth, parseQuery, withQuery } from "../features.js";
import { BODY_METHODS, METHODS } from "../store.js";
import "./code-editor.js";
import { SAVE_KEYS, SEND_KEYS } from "../shortcuts.js";
import { SPLIT, clampSplit, layoutFor, loadLayout, saveLayout, splitAt, splitForKey } from "../split.js";
import { socketHeaders, socketLine, socketLog, socketMessages } from "./websocket-view.js";
import { grpcLine, grpcLog, grpcMessage, grpcMetadataNote, grpcSchema } from "./grpc-view.js";

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

// The text split around every case-insensitive match of `query`: odd parts
// are the matches. At most 2,000 are marked.
const MAX_MARKS = 2000;
function findMarks(text, query) {
  const parts = [];
  const hay = text.toLowerCase();
  const needle = query.toLowerCase();
  let from = 0;
  let count = 0;
  for (let at = hay.indexOf(needle); at !== -1 && count < MAX_MARKS; at = hay.indexOf(needle, at + needle.length)) {
    parts.push(text.slice(from, at), text.slice(at, at + needle.length));
    from = at + needle.length;
    count++;
  }
  parts.push(text.slice(from));
  return { parts, count, capped: count === MAX_MARKS && hay.indexOf(needle, from) !== -1 };
}

const size = (n) => (n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);

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
    bodyMode: { state: true },
    wrap: { state: true },
    find: { state: true },
    findAt: { state: true },
    layout: { state: true },
    layoutKind: { state: true },
  };

  constructor() {
    super();
    this.section = 0;
    this.responseSection = 0;
    this.bodyMode = "pretty";
    this.wrap = false;
    this.find = "";
    this.findAt = 0;
    this.layout = loadLayout();
  }

  createRenderRoot() {
    return this;
  }

  update(changed) {
    // A different tab starts on Headers, or on Body when it has one.
    if (changed.has("tab") && changed.get("tab")?.id !== this.tab?.id) {
      this.section = this.tab?.kind === "websocket" || this.tab?.kind === "grpc" ? BODY : this.tab?.requestBody && BODY_METHODS.includes(this.tab.method) ? BODY : this.tab?.url?.includes("?") ? PARAMS : HEADERS;
      this.responseSection = 0;
    } else if (changed.has("tab") && (this.tab?.kind === "grpc" || this.tab?.kind === "websocket") && changed.get("tab")?.kind !== this.tab.kind) {
      // Switched to gRPC or WebSocket: the message (and service, method) or the messages come first.
      this.section = BODY;
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
    super.update(changed);
  }

  // Adds an empty parameter or form field and focuses its name.
  async addRow(kind) {
    if (kind === "param") this.paramDrafts = [...(this.paramDrafts ?? []), { key: "", value: "" }];
    else this.set({ formFields: [...(this.tab.formFields || []), { id: crypto.randomUUID(), key: "", value: "", type: "text", enabled: true }] });
    this.requestUpdate();
    await this.updateComplete;
    const rows = this.querySelectorAll(kind === "param" ? ".param-row" : ".form-row");
    rows[rows.length - 1]?.querySelector("lit-material-text-field")?.focus();
  }

  // Parameters: rows that are on live in the URL; rows turned off are kept
  // on the tab (disabledParams); new rows stay local until they get a key.
  renderParams() {
    const t = this.tab;
    const on = parseQuery(t.url || "");
    const off = t.disabledParams || [];
    this.paramDrafts ??= [];
    const setOn = (rows, extra = {}) => this.set({ url: withQuery(t.url || "", rows), ...extra });
    const row = (p, kind, i) => {
      const update = (field, value) => {
        if (kind === "on") setOn(on.map((r, j) => (j === i ? { ...r, [field]: value } : r)));
        else if (kind === "off") this.set({ disabledParams: off.map((r, j) => (j === i ? { ...r, [field]: value } : r)) });
        else {
          const draft = { ...this.paramDrafts[i], [field]: value };
          if (draft.key) {
            this.paramDrafts = this.paramDrafts.filter((_, j) => j !== i);
            setOn([...on, draft]);
          } else {
            this.paramDrafts = this.paramDrafts.map((r, j) => (j === i ? draft : r));
            this.requestUpdate();
          }
        }
      };
      const toggle = () => {
        if (kind === "on") setOn(on.filter((_, j) => j !== i), { disabledParams: [...off, p] });
        else if (kind === "off") setOn([...on, p], { disabledParams: off.filter((_, j) => j !== i) });
      };
      const remove = () => {
        if (kind === "on") setOn(on.filter((_, j) => j !== i));
        else if (kind === "off") this.set({ disabledParams: off.filter((_, j) => j !== i) });
        else {
          this.paramDrafts = this.paramDrafts.filter((_, j) => j !== i);
          this.requestUpdate();
        }
      };
      return html`
        <li class=${`param-row ${kind === "off" ? "off" : ""}`}>
          <lit-material-checkbox aria-label=${`Send ${p.key || "this parameter"}`} ?checked=${kind !== "off"} ?disabled=${kind === "draft"} @change=${toggle}></lit-material-checkbox>
          <lit-material-text-field variant="outlined" label="Parameter" .value=${p.key} @input=${(e) => update("key", e.target.value)}></lit-material-text-field>
          <lit-material-text-field variant="outlined" label="Value" .value=${p.value} @input=${(e) => update("value", e.target.value)}></lit-material-text-field>
          <lit-material-icon-button aria-label=${`Remove ${p.key || "parameter"}`} @click=${remove}>${icon("trash")}</lit-material-icon-button>
        </li>`;
    };
    return html`
      <ul class="param-rows">
        ${[
          // One list, drafts right after the live rows: a draft that gets its
          // first character becomes the last live row in the same place, so
          // the field being typed in keeps focus.
          ...on.map((p, i) => row(p, "on", i)),
          ...this.paramDrafts.map((p, i) => row(p, "draft", i)),
          ...off.map((p, i) => row(p, "off", i)),
        ]}
      </ul>
      <lit-material-button variant="text" @click=${() => this.addRow("param")}>
        ${icon("plus")} Add parameter
      </lit-material-button>
      <p class="hint">Kept in sync with the URL; unchecked rows are kept but not sent. Values are sent as typed, so encode special characters yourself or use \${variables}.</p>`;
  }

  // Body: raw text, multipart form data (with files) or a URL encoded form.
  renderBody() {
    const t = this.tab;
    const mode = t.bodyMode || "raw";
    const fields = t.formFields || [];
    const setFields = (next) => this.set({ formFields: next });
    const looksJson = /^\s*[[{]/.test(t.requestBody || "") || /json/i.test((t.headers || []).find((h) => h.name?.toLowerCase() === "content-type")?.value || "");
    return html`
      <div class="body-mode">
        <lit-material-select variant="outlined" label="Body type" .value=${mode} @change=${(e) => this.set({ bodyMode: e.target.value })}>
          <lit-material-select-option value="raw">Raw (JSON, XML, text)</lit-material-select-option>
          <lit-material-select-option value="multipart">Form data (multipart, files)</lit-material-select-option>
          <lit-material-select-option value="urlencoded">URL encoded form</lit-material-select-option>
          <lit-material-select-option value="graphql">GraphQL</lit-material-select-option>
        </lit-material-select>
      </div>
      ${mode === "graphql" ? this.renderGraphql() : mode === "raw"
        ? html`<hitop-code-editor label="Request body" language=${looksJson ? "json" : "text"} .value=${t.requestBody || ""}
            @input=${(e) => this.set({ requestBody: e.target.value })}></hitop-code-editor>`
        : html`
            <ul class=${`form-rows ${mode}`}>
              ${fields.map((f, i) => {
                const update = (changes) => setFields(fields.map((x, j) => (j === i ? { ...x, ...changes } : x)));
                const file = this.store.files.get(f.id);
                return html`
                  <li class=${`form-row ${f.enabled === false ? "off" : ""}`}>
                    <lit-material-checkbox aria-label=${`Send ${f.key || "this field"}`} ?checked=${f.enabled !== false} @change=${(e) => update({ enabled: e.target.checked })}></lit-material-checkbox>
                    <lit-material-text-field variant="outlined" label="Field" .value=${f.key} @input=${(e) => update({ key: e.target.value })}></lit-material-text-field>
                    ${mode === "multipart"
                      ? html`<lit-material-select variant="outlined" label="Type" .value=${f.type || "text"} @change=${(e) => update({ type: e.target.value })}>
                          <lit-material-select-option value="text">Text</lit-material-select-option>
                          <lit-material-select-option value="file">File</lit-material-select-option>
                        </lit-material-select>`
                      : nothing}
                    ${f.type === "file" && mode === "multipart"
                      ? html`<label class="file-pick">
                          <input type="file" class="visually-hidden" @change=${(e) => {
                            const picked = e.target.files?.[0] ?? null;
                            this.store.setFile(f.id, picked);
                            update({ fileName: picked?.name ?? "" });
                          }} />
                          <span class="file-button">${icon("file-arrow-up")} ${file ? "Change file" : "Choose file"}</span>
                          <span class="file-name">${file ? `${file.name} (${size(file.size)})` : f.fileName ? `${f.fileName}: choose it again after a reload` : "No file"}</span>
                        </label>`
                      : html`<lit-material-text-field variant="outlined" label="Value" .value=${f.value || ""} @input=${(e) => update({ value: e.target.value })}></lit-material-text-field>`}
                    <lit-material-icon-button aria-label=${`Remove ${f.key || "field"}`} @click=${() => { this.store.setFile(f.id, null); setFields(fields.filter((_, j) => j !== i)); }}>${icon("trash")}</lit-material-icon-button>
                  </li>`;
              })}
            </ul>
            <lit-material-button variant="text" @click=${() => this.addRow("field")}>${icon("plus")} Add field</lit-material-button>
            <p class="hint">${mode === "multipart"
              ? "Sent as multipart/form-data. Files stay in this page only: they aren't saved, and a collection run sends file fields empty."
              : "Sent as application/x-www-form-urlencoded."} \${variables} work in names and values.</p>`}`;
  }

  // GraphQL: the query (completion from the fetched schema), its variables
  // as JSON and the operation name; sent as one JSON POST.
  renderGraphql() {
    const t = this.tab;
    const entry = this.store.schemas.get(t.id);
    const status = entry?.loading
      ? html`<span class="hint" role="status">Fetching the schema…</span>`
      : entry?.error
        ? html`<span class="hint graphql-error" role="status">Couldn't fetch the schema: ${entry.error}</span>`
        : entry?.schema
          ? html`<span class="hint" role="status">Schema: ${entry.types} types from ${entry.url}. Completion is on in the query.</span>`
          : html`<span class="hint">Fetch the schema (introspection) for field completion.</span>`;
    return html`
      <div class="graphql-tools">
        <lit-material-button variant="outlined" ?disabled=${entry?.loading} @click=${() => this.store.fetchSchema()}>
          ${icon("arrows-clockwise")} ${entry?.schema ? "Fetch schema again" : "Fetch schema"}
        </lit-material-button>
        ${status}
      </div>
      <div class="graphql-panes">
        <hitop-code-editor class="graphql-query" label="Query" language="graphql" placeholder="query { ... }" .schema=${entry?.schema ?? null} .value=${t.requestBody || ""}
          @input=${(e) => this.set({ requestBody: e.target.value })}></hitop-code-editor>
        <div class="graphql-side">
          <hitop-code-editor class="graphql-variables" label="Variables (JSON)" language="json" placeholder='{"id": "\${userId}"}' .value=${t.graphqlVariables || ""}
            @input=${(e) => this.set({ graphqlVariables: e.target.value })}></hitop-code-editor>
          <lit-material-text-field variant="outlined" label="Operation name" supporting-text="Only needed when the query has more than one operation."
            .value=${t.graphqlOperationName || ""} @input=${(e) => this.set({ graphqlOperationName: e.target.value })}></lit-material-text-field>
        </div>
      </div>
      <p class="hint">Sent as application/json: {"query", "variables", "operationName"}. \${variables} work in all three.</p>`;
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
        ${a.type === "oauth2"
          ? html`<lit-material-select variant="outlined" label="Grant" .value=${a.grant === "password" ? "password" : "client_credentials"} @change=${(e) => this.setAuth({ grant: e.target.value })}>
                <lit-material-select-option value="client_credentials">Client credentials</lit-material-select-option>
                <lit-material-select-option value="password">Password</lit-material-select-option>
              </lit-material-select>
              ${field("Token URL", "tokenUrl", { help: "HITOP gets a token here before the request and sends it as Authorization: Bearer. It stays in memory until it expires." })}
              ${field("Client ID", "clientId")} ${field("Client secret", "clientSecret", { type: "password", help: "Use ${variables}: keep the value in a secret environment variable." })}
              ${field("Scope", "scope")}
              ${a.grant === "password" ? html`${field("Username", "username")} ${field("Password", "password", { type: "password" })}` : nothing}
              <lit-material-select variant="outlined" label="Send the client ID and secret" .value=${a.clientAuth === "body" ? "body" : ""} @change=${(e) => this.setAuth({ clientAuth: e.target.value })}>
                <lit-material-select-option value="">As Basic auth header</lit-material-select-option>
                <lit-material-select-option value="body">In the request body</lit-material-select-option>
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
    const paramCount = parseQuery(t.url || "").length + (t.disabledParams?.length ?? 0);
    const authType = t.auth?.type && t.auth.type !== "none" ? AUTH_TYPES.find(([v]) => v === t.auth.type)?.[1] : null;
    const socket = t.kind === "websocket";
    const grpc = t.kind === "grpc";
    const saved = t.wsMessages?.length ?? 0;
    // gRPC has no query: the first tab is the schema source instead.
    const labels = [
      grpc ? "Schema" : paramCount ? `Params (${paramCount})` : "Params",
      authType ? `Auth ●` : "Auth",
      grpc ? (headerCount ? `Metadata (${headerCount})` : "Metadata") : headerCount ? `Headers (${headerCount})` : "Headers",
      ...(grpc
        ? ["Message"]
        : socket
          ? [saved ? `Messages (${saved})` : "Messages"]
          : ["Body", t.preRequestScript?.trim() ? "Script before ●" : "Script before", t.postRequestScript?.trim() ? "Script after ●" : "Script after"]),
    ];
    if ((socket || grpc) && this.section > BODY) this.section = BODY;
    return html`
      <section class="editor" aria-label="Request details">
        <lit-material-tabs aria-label="Request details" .selected=${this.section} @change=${(e) => (this.section = e.target.selected)}>
          ${labels.map((label) => html`<lit-material-tab>${label}</lit-material-tab>`)}
        </lit-material-tabs>
        <div class="editor-panel">
          ${this.section === PARAMS ? (grpc ? grpcSchema(this) : this.renderParams()) : nothing}
          ${this.section === AUTH ? this.renderAuth() : nothing}
          ${this.section === HEADERS
            ? html`<lit-material-key-value-editor
                key-label="Header"
                value-label="Value"
                add-label="Add header"
                .entries=${guard([this.headerRowsVersion], () => this.headerRows)}
                @change=${(e) => this.onHeaders(e)}
              ></lit-material-key-value-editor>
              ${this.store.autoContentType(t)
                ? html`<p class="hint auto-header">Content-Type: <code>${this.store.autoContentType(t)}</code> is added when sending, from the body. Add your own Content-Type header to send another.</p>`
                : nothing}
              ${socket ? socketHeaders(this) : nothing} ${grpc ? grpcMetadataNote() : nothing}`
            : nothing}
          ${this.section === BODY && socket ? socketMessages(this) : nothing}
          ${this.section === BODY && grpc ? grpcMessage(this) : nothing}
          ${this.section === BODY && !socket && !grpc
            ? hasBody
              ? this.renderBody()
              : html`<lit-material-empty-state size="compact" headline=${`${t.method} requests have no body`}>
                  Switch the method to POST, PUT or PATCH to send one.
                </lit-material-empty-state>`
            : nothing}
          ${this.section === PRE
            ? html`<hitop-code-editor label="Script that runs before the request is sent" language="javascript" .value=${t.preRequestScript || ""} @input=${(e) => this.set({ preRequestScript: e.target.value })}></hitop-code-editor>
                ${scriptHelp(PRE_HELP)}`
            : nothing}
          ${this.section === POST
            ? html`<hitop-code-editor label="Script that runs after the response arrives" language="javascript" .value=${t.postRequestScript || ""} @input=${(e) => this.set({ postRequestScript: e.target.value })}></hitop-code-editor>
                ${scriptHelp(POST_HELP)}`
            : nothing}
        </div>
      </section>`;
  }

  // Server-Sent Events as they arrive (store.events): newest last, the stream's state on top.
  renderEvents(live) {
    return html`<section class="response sse" aria-label="Events">
      <div class="response-meta">
        ${live.open ? html`<lit-material-status status="info">Live</lit-material-status>` : html`<span class="hint">Stream ended</span>`}
        <span>${live.events.length} ${live.events.length === 1 ? "event" : "events"}</span>
        ${live.open ? html`<lit-material-button variant="outlined" @click=${() => this.store.cancel(this.tab.id)}>${icon("x")} Stop</lit-material-button>` : nothing}
      </div>
      <ol class="sse-events" aria-label="Events received">
        ${live.events.map((e) => html`<li><span class="hint">${new Date(e.at).toLocaleTimeString()}${e.event !== "message" ? html` · <b>${e.event}</b>` : nothing}${e.id ? ` · id ${e.id}` : ""}</span><pre>${e.data}</pre></li>`)}
      </ol>
    </section>`;
  }

  renderResponse() {
    const t = this.tab;
    const live = this.store.events?.get(t.id);
    if (live && (live.open || t.responseType === "cancelled")) return this.renderEvents(live);
    if (t.loading) {
      return html`<section class="response" aria-label="Response" aria-busy="true">
        <div class="skeleton bar"></div>
        <div class="skeleton block"></div>
      </section>`;
    }
    if (t.responseType === "cancelled") {
      return html`<section class="response" aria-label="Response">
        <lit-material-status status="warning">Cancelled</lit-material-status>
        <p class="hint">You stopped the request before its response arrived. Nothing was recorded in History.</p>
      </section>`;
    }
    if (t.statusCode == null && (t.response == null || t.response === "")) {
      return html`<section class="response empty" aria-label="Response">
        <lit-material-empty-state size="compact" headline="Send a request to see the response">
          ${usesProxy(t.url || "https://example.com")
            ? "Requests to public APIs go through the HITOP proxy, so CORS doesn't block them. Requests to localhost go straight from your browser."
            : "The response, its headers and timing appear here."}
          ${this.store.isFresh
            ? html`<lit-material-button slot="actions" variant="outlined" @click=${() => this.store.addSampleCollection()}>Try a sample collection</lit-material-button>
                <lit-material-button slot="actions" variant="text" @click=${() => this.app.openDialog("import")}>Import from Postman</lit-material-button>`
            : nothing}
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
    // Tabs: Body, Headers, then Tests and Events when there are some.
    const testsAt = tests.length ? 2 : -1;
    const eventsAt = live?.events.length ? (tests.length ? 3 : 2) : -1;
    if (this.responseSection >= 2 && this.responseSection !== testsAt && this.responseSection !== eventsAt) this.responseSection = 0;
    return html`
      <section class="response" aria-label="Response">
        <div class="response-meta">
          <span class=${`status-chip ${cls}`}>${t.statusCode} ${getStatusText(t.statusCode) === "Unknown Status" ? "" : getStatusText(t.statusCode)}</span>
          ${t.durationMs != null ? html`<span>${t.durationMs} ms</span>` : nothing}
          <span>${bytes(t.response || "")}</span>
          ${t.truncated ? html`<span class="warn">Cut at 5 MB</span>` : nothing}
          ${tests.length ? html`<span class=${passed === tests.length ? "pass" : "warn"}>${passed} of ${tests.length} tests passed</span>` : nothing}
          <span class="response-actions">
            ${this.store.loadedRequest(t)
              ? html`<lit-material-button variant="text" @click=${() => this.app.openDialog("save-example", { name: `${t.statusCode} response`, request: this.store.loadedRequest(t).name })}>${icon("bookmark-simple")} Save as example</lit-material-button>`
              : nothing}
            <lit-material-button variant="text" @click=${() => this.app.openDialog("compare")}>${icon("git-diff")} Compare</lit-material-button>
          </span>
        </div>
        <lit-material-tabs aria-label="Response details" .selected=${this.responseSection} @change=${(e) => (this.responseSection = e.target.selected)}>
          <lit-material-tab>Body</lit-material-tab>
          <lit-material-tab>Headers (${headers.length})</lit-material-tab>
          ${tests.length ? html`<lit-material-tab>Tests (${passed}/${tests.length})</lit-material-tab>` : nothing}
          ${live?.events.length ? html`<lit-material-tab>Events (${live.events.length})</lit-material-tab>` : nothing}
        </lit-material-tabs>
        ${!live && /text\/event-stream/i.test(t.responseHeaders?.["content-type"] ?? "") && platform().stream
          ? html`<p class="hint sse-hint">An event stream: add the header <code>Accept: text/event-stream</code> to see its events as they arrive.
              <lit-material-button variant="text" @click=${() => this.set({ headers: [...(t.headers || []).filter((h) => h.name || h.value), { name: "Accept", value: "text/event-stream" }] })}>Add it</lit-material-button></p>`
          : nothing}
        ${this.responseSection === 0
          ? t.response
            ? this.renderBodyView(code)
            : html`<p class="hint">No body${t.method === "HEAD" ? ": HEAD responses never have one" : ""}.</p>`
          : this.responseSection === eventsAt
            ? this.renderEvents(live)
          : this.responseSection === 1
            ? html`<dl class="response-headers">
                ${headers.map(([k, v]) => html`<dt>${k}</dt><dd>${v}</dd>`)}
              </dl>`
            : testList(tests)}
      </section>`;
  }

  // The response body: Pretty (formatted, highlighted) | Raw (as received) |
  // Preview (HTML, sandboxed: no scripts, no requests of its own), wrap,
  // find (marks every match; Enter / Shift+Enter step through) and download.
  renderBodyView(code) {
    const t = this.tab;
    const html_ = t.responseType === "html";
    const mode = this.bodyMode === "preview" && !html_ ? "pretty" : this.bodyMode;
    const raw = this.store.rawBodies.get(t.id);
    const text = mode === "raw" && raw !== undefined ? raw : t.response;
    const query = this.find ?? "";
    const marks = query && mode !== "preview" ? findMarks(text, query) : null;
    if (marks && this.findAt >= marks.count) this.findAt = 0;
    const seg = (value, label, title) => html`<button type="button" class="seg" aria-pressed=${String(mode === value)} title=${title}
      @click=${() => (this.bodyMode = value)}>${label}</button>`;
    const step = (by) => {
      if (!marks?.count) return;
      this.findAt = (this.findAt + by + marks.count) % marks.count;
      this.requestUpdate();
    };
    const ext = t.responseType === "json" ? "json" : t.responseType === "html" ? "html" : t.responseType === "xml" ? "xml" : "txt";
    return html`
      ${t.responseCut
        ? html`<lit-material-alert variant="info" class="response-note">${t.responseStored
            ? `Only the first ${size(t.response.length)} of this response were found after the reload: the rest is no longer in this browser's storage. Send the request again to see all of it.`
            : "This response was over 256 KB, so only its first 64 KB were kept when the page was saved. Send the request again to see all of it."}</lit-material-alert>`
        : nothing}
      ${t.jsonError
        ? html`<lit-material-alert variant="warning" class="response-note">The server says JSON, but the body doesn't parse: ${t.jsonError}. It's shown as text.</lit-material-alert>`
        : nothing}
      <div class="body-tools">
        <span class="segmented" role="group" aria-label="Show the body">
          ${seg("pretty", "Pretty", "Formatted and highlighted")}${seg("raw", "Raw", "As received")}${html_ ? seg("preview", "Preview", "The page, rendered without scripts") : nothing}
        </span>
        <button type="button" class="seg wrap-toggle" aria-pressed=${String(this.wrap)} title="Wrap long lines" @click=${() => (this.wrap = !this.wrap)}>
          ${icon("text-align-left")} Wrap
        </button>
        ${mode !== "preview"
          ? html`<label class="find">
              ${icon("magnifying-glass")}
              <input type="search" class="find-input" placeholder="Find in body" aria-label="Find in body" .value=${query}
                @input=${(e) => ((this.find = e.target.value), (this.findAt = 0))}
                @keydown=${(e) => e.key === "Enter" && (e.preventDefault(), step(e.shiftKey ? -1 : 1))} />
              ${query ? html`<span class="find-count" role="status">${marks.count ? `${this.findAt + 1} of ${marks.count}${marks.capped ? "+" : ""}` : "No matches"}</span>` : nothing}
            </label>`
          : nothing}
        <lit-material-icon-button aria-label="Download the body" title="Download the body" @click=${() => this.app.download(`response.${ext}`, text, ext === "json" ? "application/json" : "text/plain")}>
          ${icon("file-arrow-down")}
        </lit-material-icon-button>
      </div>
      ${mode === "preview"
        ? html`<iframe class="response-preview" title="Preview of the HTML response" sandbox="" referrerpolicy="no-referrer" srcdoc=${t.response}></iframe>`
        : html`<div class=${`response-scroll ${this.wrap ? "wrap" : ""}`} tabindex="0" role="region" aria-label="Response body">
            ${marks
              ? html`<pre class="response-text">${marks.parts.map((p, i) => (i % 2 ? html`<mark class=${(i - 1) / 2 === this.findAt ? "current" : ""}>${p}</mark>` : p))}</pre>`
              : html`<lit-material-code-block class="response-body" label=${mode === "raw" ? "RAW" : t.responseType.toUpperCase()}>${mode === "pretty" && code ? unsafeHTML(code) : text}</lit-material-code-block>`}
          </div>`}`;
  }

  updated() {
    // Brings the current match into view when the search or the match changes.
    const key = this.find ? `${this.find}|${this.findAt}` : "";
    if (key === this.shownMatch) return;
    this.shownMatch = key;
    this.querySelector(".response-text mark.current")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }

  // Method, WebSocket or gRPC (request kinds of their own in hitop-core).
  methodSelect() {
    const t = this.tab;
    const pick = (v) => (v === "WS" ? this.set({ kind: "websocket" }) : v === "GRPC" ? this.set({ kind: "grpc" }) : this.set({ method: v, kind: undefined }));
    return html`<lit-material-select class="method" variant="outlined" label="Method" .value=${t.kind === "websocket" ? "WS" : t.kind === "grpc" ? "GRPC" : t.method} @change=${(e) => pick(e.target.value)}>
      ${METHODS.map((m) => html`<lit-material-select-option value=${m}>${m}</lit-material-select-option>`)}
      <lit-material-select-option value="WS">WebSocket</lit-material-select-option>
      <lit-material-select-option value="GRPC">gRPC</lit-material-select-option>
    </lit-material-select>`;
  }

  render() {
    const t = this.tab;
    if (!t) return nothing;
    // The page's heading for screen readers: the tab strip shows the title.
    const heading = html`<h1 class="visually-hidden">Request: ${t.title}</h1>`;
    if (t.kind === "websocket") return html`${heading}${socketLine(this, this.methodSelect())} ${this.panes(this.renderEditor(), () => socketLog(this), "Messages log")}`;
    if (t.kind === "grpc") return html`${heading}${grpcLine(this, this.methodSelect())} ${this.panes(this.renderEditor(), () => grpcLog(this), "Call log")}`;
    // A plain text field: the browser's own URL check refuses \${baseUrl}/x and
    // shows its own message; the store checks the URL as it is sent instead.
    const urlError = this.store.urlErrors.get(t.id);
    return html`
      ${heading}
      <form class="request-line" novalidate @submit=${(e) => this.send(e)}>
        ${this.methodSelect()}
        <lit-material-text-field
          class="url"
          variant="outlined"
          label="URL"
          placeholder="https://api.example.com/users or \${baseUrl}/users"
          ?error=${Boolean(urlError)}
          error-text=${urlError ?? ""}
          .value=${t.url}
          @input=${(e) => this.set({ url: e.target.value })}
          @keydown=${(e) => e.key === "Enter" && this.send(e)}
        ></lit-material-text-field>
        ${this.app.envSelect()}
        ${t.loading
          ? html`<lit-material-button class="cancel-send" variant="outlined" type="button" title="Stop waiting for the response" @click=${() => this.store.cancel(t.id)}>
              ${icon("x")} Cancel
            </lit-material-button>`
          : html`<lit-material-button variant="filled" type="submit" title=${`Send (${SEND_KEYS})`}>${icon("paper-plane-tilt")} Send</lit-material-button>`}
        <lit-material-icon-button aria-label="Save request" title=${`Save (${SAVE_KEYS})`} ?disabled=${!t.url} @click=${() => this.app.openDialog("save", this.store.saveTarget())}>
          ${icon("floppy-disk")}
        </lit-material-icon-button>
        <lit-material-icon-button aria-label="Code" title="Code: curl, fetch, Python, Go, Java, Rust" ?disabled=${!t.url} @click=${() => this.app.openDialog("code", { lang: "curl", showSecrets: false })}>
          ${icon("code")}
        </lit-material-icon-button>
      </form>
      ${this.panes(this.renderEditor(), () => this.renderResponse(), "Response")}`;
  }

  // --- the split between the editor and the response ----------------------------

  // Side by side (wide), one above the other with a handle (narrower), or
  // plain stacked (phones), by this view's width; the split and a folded
  // response are remembered per viewer (split.js). A folded response stays
  // folded when a new one arrives: its bar shows the new status.
  panes(editor, response, title) {
    const layout = this.layoutKind ?? "stack";
    const folded = this.layout.responseCollapsed;
    const kind = layout === "columns" ? "columns" : "rows";
    const value = this.layout[kind];
    const style = layout === "columns" ? `--split: ${value}%` : layout === "rows" ? `--editor-height: ${value}px` : "";
    const { min, max } = SPLIT[kind];
    const handle =
      layout !== "stack" && !folded
        ? html`<div class="split-handle" role="separator" tabindex="0" aria-orientation=${layout === "columns" ? "vertical" : "horizontal"}
            aria-label="Resize the request and response panes" aria-valuemin=${min} aria-valuemax=${max} aria-valuenow=${value}
            aria-valuetext=${layout === "columns" ? `Request ${value}% of the width` : `Request ${value} pixels high`}
            title="Drag to resize; arrow keys too, Enter resets"
            @pointerdown=${(e) => this.startDrag(e, kind)} @keydown=${(e) => this.splitKey(e, kind)} @dblclick=${() => this.setSplit(kind, SPLIT[kind].initial)}></div>`
        : nothing;
    return html`<div class=${`panes ${layout}${folded ? " folded" : ""}`} style=${style}>
      <div class="pane pane-editor">${editor}</div>
      ${handle}
      <div class="pane pane-response">
        ${folded
          ? html`<div class="pane-bar folded-bar">
              <span class="pane-title">${title} hidden</span>${this.foldedStatus()}
              <lit-material-button variant="text" aria-expanded="false" @click=${() => this.foldResponse(false)}>${icon("caret-up")} Show ${title.toLowerCase()}</lit-material-button>
            </div>`
          : html`<div class="pane-bar">
                <span class="pane-title">${title}</span>
                <lit-material-icon-button class="fold-response" aria-label=${`Hide the ${title.toLowerCase()}`} title=${`Hide the ${title.toLowerCase()}`} aria-expanded="true" @click=${() => this.foldResponse(true)}>${icon("caret-down")}</lit-material-icon-button>
              </div>
              ${response()}`}
      </div>
    </div>`;
  }

  // What the folded response's bar says: the status of the last response.
  foldedStatus() {
    const t = this.tab;
    if (t.kind) return nothing;
    if (t.loading) return html`<span class="hint">Waiting for the response…</span>`;
    if (t.responseType === "cancelled") return html`<span class="hint">Cancelled</span>`;
    if (t.responseType === "error") return html`<span class="status-chip bad">Failed</span>`;
    if (t.statusCode == null) return nothing;
    return html`<span class=${`status-chip ${t.statusCode < 300 ? "ok" : t.statusCode < 400 ? "info" : "bad"}`}>${t.statusCode}</span>`;
  }

  foldResponse(folded) {
    this.layout = { ...this.layout, responseCollapsed: folded };
    saveLayout(this.layout);
    // Focus stays with the control that brings it back.
    this.updateComplete.then(() => this.querySelector(folded ? ".folded-bar lit-material-button" : ".fold-response")?.focus());
  }

  setSplit(kind, value) {
    this.layout = { ...this.layout, [kind]: clampSplit(kind, value) };
    saveLayout(this.layout);
  }

  splitKey(e, kind) {
    const next = splitForKey(kind, this.layout[kind], e.key, e.shiftKey);
    if (next == null) return;
    e.preventDefault();
    this.setSplit(kind, next);
  }

  startDrag(e, kind) {
    if (e.button !== 0) return;
    const handle = e.currentTarget;
    const box = (kind === "columns" ? this.querySelector(".panes") : this.querySelector(".pane-editor"))?.getBoundingClientRect();
    if (!box) return;
    e.preventDefault();
    handle.setPointerCapture(e.pointerId);
    handle.classList.add("dragging");
    const move = (m) => {
      this.layout = { ...this.layout, [kind]: kind === "columns" ? splitAt(kind, m.clientX, box.left, box.width) : splitAt(kind, m.clientY, box.top, 0) };
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
      handle.classList.remove("dragging");
      saveLayout(this.layout);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  }

  connectedCallback() {
    super.connectedCallback();
    this.resizer = new ResizeObserver(([entry]) => {
      const kind = layoutFor(entry.contentRect.width);
      if (kind !== this.layoutKind) this.layoutKind = kind;
    });
    this.resizer.observe(this);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.resizer?.disconnect();
  }
}

customElements.define("hitop-request", HitopRequest);
