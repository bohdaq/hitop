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
import { syncClient } from "./sync-view.js";
import { commentsPanel } from "./comments-view.js";
import { AUTH_TYPES, emptyAuth, parseQuery, withQuery } from "../features.js";
import { BODY_METHODS, METHODS } from "../store.js";
import "./code-editor.js";
import "@lit-material/timing-waterfall";
import "@lit-material/popover";
import { compareDialog } from "./tools-dialogs.js";
import { storageOf } from "../git.js";
import { loadRequestHistory } from "./history-dialog.js";
import { SAVE_KEYS, SEND_KEYS } from "../shortcuts.js";
import { bodyKind, checkJson } from "../request-facts.js";
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
  <ul class="tests" aria-label="Checks">
    ${tests.map(
      (x) => html`<li class=${x.pass ? "pass" : "fail"}>
        ${icon(x.pass ? "check-circle" : "warning-circle")}<span class="visually-hidden">${x.pass ? "Passed" : "Failed"}: </span>
        <span class="test-name">${x.name}</span>${x.detail ? html`<span class="test-detail">${x.detail}</span>` : nothing}
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
      <div class="kv-head" aria-hidden="true"><span></span><span>Parameter</span><span>Value</span><span></span></div>
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
      ${mode === "raw" ? this.bodyFacts() : nothing}
      ${mode === "graphql" ? this.renderGraphql() : mode === "raw"
        ? html`<hitop-code-editor label="Request body" language=${looksJson ? "json" : "text"} .value=${t.requestBody || ""}
            @input=${(e) => this.set({ requestBody: e.target.value })}></hitop-code-editor>`
        : html`
            <div class=${`kv-head ${mode}`} aria-hidden="true"><span></span><span>Field</span>${mode === "multipart" ? html`<span>Type</span>` : nothing}<span>Value</span><span></span></div>
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

  // A quiet line beside the body type (HITOP Next, Main board): the raw body's kind, whether its
  // JSON parses (with the first error's line and column) and the OpenAPI operation the saved
  // request came from. No schema check: the app keeps no schemas.
  bodyFacts() {
    const t = this.tab;
    const body = t.requestBody || "";
    const ct = (t.headers || []).find((h) => h.enabled !== false && h.name?.toLowerCase() === "content-type")?.value;
    const kind = bodyKind(body, ct);
    const json = kind === "JSON" && body.trim() ? checkJson(body) : null;
    const op = this.store.loadedRequest(t)?.openapi;
    const operation = op?.operationId || op?.operation;
    return html`<p class="body-facts">
      <span class="body-kind">${kind}</span>
      ${json?.ok ? html`<span class="body-valid">Valid JSON ✓</span>` : nothing}
      ${json && !json.ok ? html`<span class="body-invalid">JSON error · line ${json.line}, col ${json.col}</span>` : nothing}
      ${operation ? html`<span class="body-openapi">OpenAPI · ${operation}</span>` : nothing}
    </p>`;
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
          ? html`<lit-material-select variant="outlined" label="Grant" .value=${["password", "authorization_code"].includes(a.grant) ? a.grant : "client_credentials"} @change=${(e) => this.setAuth({ grant: e.target.value })}>
                <lit-material-select-option value="client_credentials">Client credentials</lit-material-select-option>
                <lit-material-select-option value="password">Password</lit-material-select-option>
                <lit-material-select-option value="authorization_code">Authorization code (PKCE)</lit-material-select-option>
              </lit-material-select>
              ${a.grant === "authorization_code"
                ? html`${field("Authorization URL", "authUrl", { help: "The provider's sign-in page: HITOP opens it in a pop-up when it needs a token." })}
                    ${field("Redirect URL (optional)", "redirectUri", { help: `Register this with the provider: ${location.origin}${import.meta.env.BASE_URL}oauth-callback.html (the CLI uses http://127.0.0.1:<port>/callback).` })}`
                : nothing}
              ${field("Token URL", "tokenUrl", { help: "HITOP gets a token here before the request and sends it as Authorization: Bearer. It stays in memory until it expires." })}
              ${field("Client ID", "clientId")} ${field("Client secret", "clientSecret", { type: "password", help: "Use ${variables}: keep the value in a secret environment variable." })}
              ${field("Scope", "scope")}
              ${a.grant === "password" ? html`${field("Username", "username")} ${field("Password", "password", { type: "password" })}` : nothing}
              <lit-material-select variant="outlined" label="Send the client ID and secret" .value=${a.clientAuth === "body" ? "body" : ""} @change=${(e) => this.setAuth({ clientAuth: e.target.value })}>
                <lit-material-select-option value="">As Basic auth header</lit-material-select-option>
                <lit-material-select-option value="body">In the request body</lit-material-select-option>
              </lit-material-select>`
          : nothing}
        ${a.type === "digest"
          ? html`${field("Username", "username")} ${field("Password", "password", { type: "password", help: "HITOP answers the server's Digest challenge (MD5 or SHA-256): the request goes out twice." })}`
          : nothing}
        ${a.type === "awsv4"
          ? html`${field("Access key", "accessKey")} ${field("Secret key", "secretKey", { type: "password", help: "Use ${variables}: keep the value in a secret environment variable." })}
              ${field("Region", "region", { help: "us-east-1, eu-west-1, …" })} ${field("Service", "service", { help: "execute-api, s3, lambda, …" })}
              ${field("Session token (optional)", "sessionToken", { type: "password" })}
              <p class="hint">The request is signed as it is sent, after the script before it runs.</p>`
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
    const counted = (label, count) => (count ? html`${label} <span class="count">${count}</span>` : label);
    const dotted = (label, on) => (on ? html`${label} <span class="tab-dot" aria-hidden="true">●</span>` : label);
    const labels = [
      grpc ? "Schema" : counted("Params", paramCount),
      dotted("Auth", authType),
      counted(grpc ? "Metadata" : "Headers", headerCount),
      ...(grpc
        ? ["Message"]
        : socket
          ? [counted("Messages", saved)]
          : ["Body", dotted("Script before", t.preRequestScript?.trim()), dotted("Script after", t.postRequestScript?.trim())]),
    ];
    // Comments: a saved request of a shared collection (sync.js).
    const sync = syncClient(this.store);
    const request = this.store.loadedRequest(t);
    const shared = Boolean(request && t.loadedCollectionId != null && sync.link(t.loadedCollectionId));
    const COMMENTS = labels.length;
    if (shared) {
      const n = sync.comments(t.loadedCollectionId, request).length;
      labels.push(counted("Comments", n));
    }
    if ((socket || grpc) && this.section > BODY && this.section !== COMMENTS) this.section = BODY;
    if (!shared && this.section >= COMMENTS) this.section = PARAMS;
    return html`
      <section class="editor" aria-label="Request details">
        <lit-material-tabs class="sub-tabs" aria-label="Request details" .selected=${this.section} @change=${(e) => (this.section = e.target.selected)}>
          ${labels.map((label) => html`<lit-material-tab>${label}</lit-material-tab>`)}
        </lit-material-tabs>
        <div class="editor-panel">
          ${this.section === PARAMS ? (grpc ? grpcSchema(this) : this.renderParams()) : nothing}
          ${this.section === AUTH ? this.renderAuth() : nothing}
          ${this.section === HEADERS
            ? html`<div class="kv-head headers" aria-hidden="true"><span>${grpc ? "Metadata" : "Header"}</span><span>Value</span><span></span></div>
              <lit-material-key-value-editor
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
          ${this.section === PRE && !socket && !grpc
            ? html`<hitop-code-editor label="Script that runs before the request is sent" language="javascript" .value=${t.preRequestScript || ""} @input=${(e) => this.set({ preRequestScript: e.target.value })}></hitop-code-editor>
                ${scriptHelp(PRE_HELP)}`
            : nothing}
          ${shared && this.section === COMMENTS ? commentsPanel(this, sync, t.loadedCollectionId, request) : nothing}
          ${this.section === POST && !socket && !grpc
            ? html`<hitop-code-editor label="Script that runs after the response arrives" language="javascript" .value=${t.postRequestScript || ""} @input=${(e) => this.set({ postRequestScript: e.target.value })}></hitop-code-editor>
                ${scriptHelp(POST_HELP)}`
            : nothing}
        </div>
      </section>`;
  }

  // Server-Sent Events as they arrive (store.events): newest last, the stream's state on top.
  renderEvents(live) {
    return html`<section class="response sse" data-state=${live.open ? "streaming" : "done"} aria-label="Events">
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
      return html`<section class="response" data-state="sending" aria-label="Response" aria-busy="true">
        <div class="skeleton bar"></div>
        <div class="skeleton block"></div>
      </section>`;
    }
    if (t.responseType === "cancelled") {
      return html`<section class="response" data-state="cancelled" aria-label="Response">
        <lit-material-status status="warning">Cancelled</lit-material-status>
        <p class="hint">You stopped the request before its response arrived. Nothing was recorded in History.</p>
      </section>`;
    }
    if (t.statusCode == null && (t.response == null || t.response === "")) {
      return html`<section class="response empty" data-state="idle" aria-label="Response">
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
      return html`<section class="response" data-state="error" aria-label="Response">
        <lit-material-status status="error">Request failed</lit-material-status>
        <p class="error-text">${String(t.response).replace(/^Error: /, "")}</p>
      </section>`;
    }
    const cls = t.statusCode < 300 ? "ok" : t.statusCode < 400 ? "info" : "bad";
    const headers = Object.entries(t.responseHeaders || {});
    const code = highlighted(t.response, t.responseType);
    const tests = t.tests || [];
    const passed = tests.filter((x) => x.pass).length;
    // Tabs: Body, Headers, Raw, then Tests, Events and Diff vs last when there are some.
    // Diff vs last: the current response against the one before it (or a saved example).
    // The current response is in History already: a diff needs one more (or a saved example).
    const canDiff = this.store.history.filter((h) => h?.response).length > 1 || (this.store.loadedRequest(t)?.examples || []).length > 0;
    const tabs = ["body", "headers", "raw", ...(tests.length ? ["tests"] : []), ...(live?.events.length ? ["events"] : []), ...(canDiff ? ["diff"] : [])];
    if (this.responseSection >= tabs.length) this.responseSection = 0;
    const shown = tabs[this.responseSection];
    return html`
      <section class="response" data-state=${t.responseType === "error" ? "error" : "done"} aria-label="Response">
        <div class="response-meta">
          <span class=${`status-chip ${cls}`}><span class="mono">${t.statusCode}</span>${getStatusText(t.statusCode) === "Unknown Status" ? "" : html` ${getStatusText(t.statusCode)}`}</span>
          ${t.durationMs != null ? html`<span class="mono response-num">${t.durationMs}<span class="unit"> ms</span></span>` : nothing}
          <span class="mono response-num">${bytes(t.response || "")}</span>
          ${t.truncated ? html`<span class="warn">Cut at 5 MB</span>` : nothing}
          ${t.timings?.reused ? html`<span class="hint">reused connection</span>` : nothing}
          ${tests.length ? html`<span class=${`checks-summary ${passed === tests.length ? "pass" : "warn"}`}>${icon(passed === tests.length ? "check-circle" : "warning-circle")}${passed} of ${tests.length} tests passed</span>` : nothing}
          <span class="response-actions">
            ${this.store.loadedRequest(t)
              ? html`<lit-material-icon-button aria-label="Save as example" title="Save as example" @click=${() => this.app.openDialog("save-example", { name: `${t.statusCode} response`, request: this.store.loadedRequest(t).name })}>${icon("bookmark-simple")}</lit-material-icon-button>`
              : nothing}
            <lit-material-icon-button aria-label="Compare" title="Compare responses" @click=${() => this.app.openDialog("compare")}>${icon("git-diff")}</lit-material-icon-button>
          </span>
        </div>
        ${this.timing()}
        <lit-material-tabs class="sub-tabs" aria-label="Response details" .selected=${this.responseSection} @change=${(e) => (this.responseSection = e.target.selected)}>
          <lit-material-tab>Body</lit-material-tab>
          <lit-material-tab>Headers <span class="count">${headers.length}</span></lit-material-tab>
          <lit-material-tab>Raw</lit-material-tab>
          ${tests.length ? html`<lit-material-tab>Tests <span class=${`count ${passed === tests.length ? "pass" : "warn"}`}>${passed}/${tests.length}</span></lit-material-tab>` : nothing}
          ${live?.events.length ? html`<lit-material-tab>Events <span class="count">${live.events.length}</span></lit-material-tab>` : nothing}
          ${canDiff ? html`<lit-material-tab>Diff vs last</lit-material-tab>` : nothing}
        </lit-material-tabs>
        ${!live && /text\/event-stream/i.test(t.responseHeaders?.["content-type"] ?? "") && platform().stream
          ? html`<p class="hint sse-hint">An event stream: add the header <code>Accept: text/event-stream</code> to see its events as they arrive.
              <lit-material-button variant="text" @click=${() => this.set({ headers: [...(t.headers || []).filter((h) => h.name || h.value), { name: "Accept", value: "text/event-stream" }] })}>Add it</lit-material-button></p>`
          : nothing}
        ${shown === "body"
          ? t.response
            ? html`${this.renderBodyView(code)}${tests.length ? testList(tests) : nothing}`
            : html`<p class="hint">No body${t.method === "HEAD" ? ": HEAD responses never have one" : ""}.</p>`
          : shown === "events"
            ? this.renderEvents(live)
          : shown === "diff"
            ? this.renderDiff()
          : shown === "headers"
            ? html`<dl class="response-headers">
                ${headers.map(([k, v]) => html`<dt>${k}</dt><dd>${v}</dd>`)}
              </dl>`
          : shown === "raw"
            ? this.renderRaw(headers)
            : testList(tests)}
      </section>`;
  }

  // The whole answer as curl -i prints it: the status line, every header, a blank line, the body
  // as received (the version is the proxy's or the engine's: HTTP/1.1 is what both speak today).
  renderRaw(headers) {
    const t = this.tab;
    const raw = this.store.rawBodies.get(t.id);
    const body = typeof raw === "string" ? raw : typeof t.response === "string" ? t.response : JSON.stringify(t.response ?? "");
    const reason = getStatusText(t.statusCode);
    const text = [`HTTP/1.1 ${t.statusCode}${reason === "Unknown Status" ? "" : ` ${reason}`}`, ...headers.map(([k, v]) => `${k}: ${v}`), "", body].join("\n");
    return html`<div class="response-scroll" tabindex="0" role="region" aria-label="Raw response">
      <lit-material-icon-button class="raw-copy" aria-label="Copy the raw response" title="Copy" @click=${() => navigator.clipboard?.writeText(text)}>${icon("copy")}</lit-material-icon-button>
      <pre class="response-text response-raw">${text}</pre>
    </div>`;
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
    this.syncUrlMirror();
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
    const current = t.kind === "websocket" ? "WS" : t.kind === "grpc" ? "GRPC" : t.bodyMode === "graphql" ? "GQL" : t.method;
    return html`<lit-material-select class=${`method m-${current}`} variant="outlined" label="Method" .value=${t.kind === "websocket" ? "WS" : t.kind === "grpc" ? "GRPC" : t.method} @change=${(e) => pick(e.target.value)}>
      ${METHODS.map((m) => html`<lit-material-select-option value=${m}>${m}</lit-material-select-option>`)}
      <lit-material-select-option value="WS">WebSocket</lit-material-select-option>
      <lit-material-select-option value="GRPC">gRPC</lit-material-select-option>
    </lit-material-select>`;
  }

  // The "⋯" menu next to Send: Save to… (another collection), Code, Ask AI, Version history
  // (a cloud or GitHub collection) and Move to… (a saved request).
  lineMenu() {
    const t = this.tab;
    const s = this.store;
    const request = s.loadedRequest(t);
    const c = request ? s.collection(t.loadedCollectionId) : null;
    const where = c ? storageOf(s, c) : null;
    const history = Boolean(where && ["cloud", "github"].includes(where.kind));
    const item = (label, run, extra = {}) => html`<lit-material-list-item interactive ?disabled=${Boolean(extra.disabled)} @click=${run}>${label}</lit-material-list-item>`;
    return html`<lit-material-menu id="line-menu" aria-label="More options">
      ${item(request ? "Save to…" : "Save to a collection…", () => this.app.openDialog("save", { ...s.saveTarget(), overwrite: false }))}
      ${item("Code", () => this.app.openDialog("code", { lang: "curl", showSecrets: false }), { disabled: !t.url })}
      ${item("Ask AI", () => this.app.openAi())}
      ${history
        ? item("Version history…", () => {
            const d = { request, collectionId: t.loadedCollectionId, history: null };
            this.app.openDialog("request-history", d);
            loadRequestHistory(this.app, d);
          })
        : nothing}
      ${request ? item("Move to…", () => this.app.openDialog("move-request", { request, collectionId: t.loadedCollectionId })) : nothing}
    </lit-material-menu>`;
  }

  // What the URL says about the transport, or after a response the HTTP version it came over
  // (desktop and web proxy; nothing is made up: a direct browser fetch can't see it, nor TLS).
  protocolHint() {
    if (this.tab.httpVersion) return html`<span class="url-hint" title="HTTP version of the last response">${this.tab.httpVersion}</span>`;
    const url = String(this.tab.url ?? "").trim();
    const m = /^([a-z][a-z0-9+.-]*):\/\//i.exec(url);
    const scheme = m ? m[1].toLowerCase() : null;
    const text = scheme === "https" ? "HTTPS" : scheme === "http" ? "HTTP" : scheme === "wss" ? "WSS" : scheme === "ws" ? "WS" : null;
    return text ? html`<span class="url-hint" aria-hidden="true">${text}</span>` : nothing;
  }

  // The URL as the field shows it, drawn over the transparent input (HITOP Next): every
  // ${variable} is an amber chip; hovering, focusing or clicking one peeks at its value and
  // where it comes from (secrets masked); a variable nothing defines is marked. The mirror
  // follows the input's box and scroll (updated()). Chips take the pointer; the text does not.
  urlMirror() {
    const t = this.tab;
    const text = String(t.url ?? "");
    const parts = text.split(/(\$\{[^}]*\})/);
    const open = (n, el, pin) => {
      if (!n) return;
      // A pointer down outside the popover dismisses it (popover="auto") a tick before the click
      // arrives, while its `open` still says true: the state is reset first, then it is shown.
      const pv = this.querySelector(".var-peek");
      if (pv) pv.open = false;
      this.peek = n;
      this.peekPinned = pin;
      this.requestUpdate();
      this.updateComplete.then(() => setTimeout(() => this.querySelector(".var-peek")?.show(el), 0));
    };
    const leave = () => {
      if (this.peekPinned) return;
      this.querySelector(".var-peek")?.close();
    };
    return html`<div class="url-mirror url-vars" style=${this.mirrorStyle ?? ""} role="group" aria-label="Request variables">
      <span class="url-mirror-text" style=${`transform: translateX(-${this.urlScroll || 0}px)`}>${parts.map((part, i) => {
        if (i % 2 === 0) return html`<span class="url-text" aria-hidden="true">${part}</span>`;
        const n = part.slice(2, -1).trim();
        const found = n ? this.store.variableSource(t.loadedCollectionId, n) : null;
        return html`<span class=${`var-chip ${found ? "" : "missing"}`} role="button" tabindex="0" aria-haspopup="dialog"
          title=${found ? `${found.value} (${found.scope})` : "Not defined"}
          @pointerenter=${(e) => open(n, e.currentTarget, false)} @pointerleave=${leave}
          @focus=${(e) => open(n, e.currentTarget, false)} @blur=${leave}
          @click=${(e) => open(n, e.currentTarget, true)}
          @keydown=${(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), open(n, e.currentTarget, true))}>${part}</span>`;
      })}</span>
    </div>`;
  }

  varPeek() {
    const t = this.tab;
    const peek = this.peek ?? "";
    const src = peek ? this.store.variableSource(t.loadedCollectionId, peek) : null;
    return html`<lit-material-popover class="var-peek" @close=${() => (this.peekPinned = false)} @pointerenter=${() => (this.peekPinned = true)}>
      <span slot="header" class="mono">\${${peek}}</span>
      ${src
        ? html`<p class="var-value mono">${src.value || html`<span class="hint">empty</span>`}</p><p class="hint">From the ${src.scope}.</p>`
        : html`<p class="hint">Nothing defines it: the request won't be sent until an environment, the collection or globals do.</p>`}
      <lit-material-button slot="footer" variant="text" @click=${() => this.app.go(src?.scope === "collection" ? "variables" : "environments", t.loadedCollectionId)}>Edit</lit-material-button>
    </lit-material-popover>`;
  }

  // Keeps the URL mirror on the input's box and scroll position.
  syncUrlMirror() {
    const field = this.querySelector(".url-field");
    const input = field?.querySelector(".url")?.shadowRoot?.querySelector("input");
    if (!field || !input) return;
    if (this.urlInput !== input) {
      this.urlInput = input;
      const follow = () => {
        if (this.urlScroll !== input.scrollLeft) {
          this.urlScroll = input.scrollLeft;
          this.requestUpdate();
        }
      };
      input.addEventListener("scroll", follow);
      input.addEventListener("input", () => requestAnimationFrame(follow));
      input.addEventListener("keyup", follow);
    }
    const a = input.getBoundingClientRect();
    const b = field.getBoundingClientRect();
    const style = `left: ${a.left - b.left}px; top: ${a.top - b.top}px; width: ${a.width}px; height: ${a.height}px`;
    if (style !== this.mirrorStyle || this.urlScroll !== input.scrollLeft) {
      this.mirrorStyle = style;
      this.urlScroll = input.scrollLeft;
      this.requestUpdate();
    }
  }

  // Where the request's time went (desktop, and the web proxy for public hosts).
  timing() {
    const t = this.tab;
    if (!t.timings) return nothing;
    return html`<lit-material-timing-waterfall class="timing" .timings=${t.timings}></lit-material-timing-waterfall>`;
  }

  renderDiff() {
    return compareDialog(this.app, this.diffPick ?? {}, (pick) => ((this.diffPick = pick), this.requestUpdate())).body;
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
        <div class="url-bar">
          ${this.methodSelect()}
          <div class="url-field">
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
            ${this.urlMirror()}
          </div>
          ${this.protocolHint()}
        </div>
        ${t.loading
          ? html`<lit-material-button class="cancel-send" variant="outlined" type="button" title="Stop waiting for the response" @click=${() => this.store.cancel(t.id)}>
              ${icon("x")} Cancel
            </lit-material-button>`
          : html`<lit-material-button class="send" variant="filled" type="submit" title=${`Send (${SEND_KEYS})`}>Send <lit-material-kbd class="send-kbd" aria-hidden="true" keys="Mod+Enter"></lit-material-kbd></lit-material-button>`}
        <span class="line-tools">
          ${this.store.loadedRequest(t)
            ? html`<lit-material-icon-button aria-label="Save request" title=${`Save (${SAVE_KEYS})`} @click=${() => this.app.openDialog("save", this.store.saveTarget())}>
                ${icon("floppy-disk")}
              </lit-material-icon-button>`
            : html`<lit-material-button variant="outlined" class="save-draft" title=${`Save to a collection (${SAVE_KEYS})`} @click=${() => this.app.openDialog("save", this.store.saveTarget())}>
                ${icon("floppy-disk")} <span class="visually-hidden">Save request, </span>Save to…
              </lit-material-button>`}
          <lit-material-icon-button class="more-send" aria-label="More options" title="More: Save to…, Code, Ask AI, Version history, Move to…" aria-haspopup="menu"
            @click=${(e) => this.querySelector("#line-menu")?.show(e.currentTarget)}>
            ${icon("dots-three")}
          </lit-material-icon-button>
        </span>
      </form>
      ${this.lineMenu()}
      ${this.varPeek()}
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
      this.syncUrlMirror();
    });
    this.resizer.observe(this);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.resizer?.disconnect();
  }
}

customElements.define("hitop-request", HitopRequest);
