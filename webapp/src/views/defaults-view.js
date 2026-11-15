// The collection page's "Auth and headers" section: what every request of the collection, or of
// one folder, gets unless it sets its own (store.setDefaults; hitop-core Collection::effective).
import { html, nothing } from "lit";
import { guard } from "lit/directives/guard.js";
import { icon } from "../icons.js";
import { AUTH_TYPES, emptyAuth } from "../features.js";
import { allFolders } from "../tree-moves.js";
import { folderTitle } from "../collection-tree.js";

// Fields per auth type, as [label, key, password?].
const FIELDS = {
  bearer: [["Token", "token", true]],
  basic: [["Username", "username"], ["Password", "password", true]],
  apikey: [["Name", "key"], ["Value", "value", true]],
  digest: [["Username", "username"], ["Password", "password", true]],
  awsv4: [["Access key", "accessKey"], ["Secret key", "secretKey", true], ["Region", "region"], ["Service", "service"], ["Session token (optional)", "sessionToken", true]],
  oauth2: [["Token URL", "tokenUrl"], ["Client ID", "clientId"], ["Client secret", "clientSecret", true], ["Scope", "scope"]],
};

/** The section for collection `c`; `view` keeps the picked scope (`defaultsScope`). */
export function defaultsSection(view, store, c) {
  const folders = [...allFolders(c)].sort();
  const scope = folders.includes(view.defaultsScope) ? view.defaultsScope : "";
  const current = scope ? c.folderDefaults?.[scope] ?? {} : { auth: c.auth, headers: c.headers };
  const auth = { ...emptyAuth(), type: scope ? "inherit" : "none", ...(current.auth || {}) };
  const headers = current.headers || [];
  const save = (changes) => store.setDefaults(c.id, scope, { auth, headers, ...changes });
  const setAuth = (updates) => save({ auth: { ...auth, ...updates } });
  // The header editor rebuilds its rows when `entries` is set: only hand rows over when they
  // changed outside it (another scope or collection).
  const key = `${c.id}|${scope}|${JSON.stringify(headers)}`;
  if (key !== view.defaultsKey) {
    view.defaultsKey = key;
    view.defaultsRows = headers.map((h) => ({ key: h.name, value: h.value }));
    view.defaultsVersion = (view.defaultsVersion ?? 0) + 1;
  }
  const types = AUTH_TYPES.filter(([v]) => scope || v !== "inherit");
  return html`<section class="defaults" aria-labelledby="defaults-title">
    <h2 id="defaults-title">Auth and headers</h2>
    <p class="page-lead">Every request ${scope ? html`in <b>${folderTitle(scope.split("/").pop())}</b>` : "in this collection"} uses these unless it sets its own:
      a request's auth set to <i>Inherit from parent</i> gets ${scope ? "this folder's (or the collection's)" : "this"}, and these headers go out before its own (one of the same name replaces them).</p>
    ${folders.length
      ? html`<lit-material-select variant="outlined" label="For" class="defaults-scope" .value=${scope} @change=${(e) => ((view.defaultsScope = e.target.value), view.requestUpdate())}>
          <lit-material-select-option value="">The whole collection</lit-material-select-option>
          ${folders.map((f) => html`<lit-material-select-option value=${f}>Folder ${f.split("/").map(folderTitle).join(" / ")}</lit-material-select-option>`)}
        </lit-material-select>`
      : nothing}
    <div class="auth defaults-auth">
      <lit-material-select variant="outlined" label="Auth" .value=${auth.type} @change=${(e) => setAuth({ type: e.target.value })}>
        ${types.map(([v, l]) => html`<lit-material-select-option value=${v}>${v === "inherit" ? "Inherit from the collection" : l}</lit-material-select-option>`)}
      </lit-material-select>
      ${(FIELDS[auth.type] || []).map(([label, k, secret]) => html`<lit-material-text-field variant="outlined" label=${label} type=${secret ? "password" : "text"} .value=${auth[k] || ""}
        supporting-text=${secret ? "Use ${variables}: keep the value in a secret environment variable." : ""} @change=${(e) => setAuth({ [k]: e.target.value })}></lit-material-text-field>`)}
      ${auth.type === "apikey"
        ? html`<lit-material-select variant="outlined" label="Send as" .value=${auth.in || "header"} @change=${(e) => setAuth({ in: e.target.value })}>
            <lit-material-select-option value="header">Header</lit-material-select-option>
            <lit-material-select-option value="query">Query parameter</lit-material-select-option>
          </lit-material-select>`
        : nothing}
    </div>
    <div class="kv-head headers" aria-hidden="true"><span>Header</span><span>Value</span><span></span></div>
    <lit-material-key-value-editor key-label="Header" value-label="Value" add-label="Add header"
      .entries=${guard([view.defaultsVersion], () => view.defaultsRows)}
      @change=${(e) => {
        const rows = e.detail.entries.filter((x) => x.key).map((x) => ({ name: x.key, value: x.value }));
        view.defaultsKey = `${c.id}|${scope}|${JSON.stringify(rows)}`;
        save({ headers: rows });
      }}></lit-material-key-value-editor>
    <p class="hint">${icon("info")} Saved with the collection (<code>hitop.yaml</code>: <code>auth</code>, <code>headers</code>, <code>folderDefaults</code>); <code>hitop run</code> sends them too.</p>
  </section>`;
}
