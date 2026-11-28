// The Code and Compare dialogs: a request as code (hitop-core's snippet
// generator through hitop-wasm) and two responses side by side.
// hitop-app.js renders them in its one dialog; each returns
// { headline, body, footer }.
import { html, nothing } from "lit";
import { compareResponses, sideBySide } from "../diff.js";
import { loadFormat } from "../wasm/format.js";
import { pluginsWith } from "../plugin-hooks.js";

export const WIDE_DIALOGS = ["compare", "git"];
// UX2: the Code dialog is as wide as its code needs (up to the wide dialogs' width).
export const FIT_DIALOGS = ["code", "plugin-view"];
// A plugin language's id in the Code dialog (never one of hitop-core's language ids).
const PLUGIN_LANG = "plugin:";
// Rows a viewer's table shows: a valid answer may have 10 000 (decision 9), which the page
// doesn't draw at once.
const SHOWN_ROWS = 500;

let languages = null;

// Fills in the snippet for the dialog's language (async: the wasm module).
export async function refreshCode(app) {
  const d = app.dialog;
  if (d?.kind !== "code") return;
  // Snippet plugins are languages too, listed once per opening of the dialog.
  const plugins = d.pluginLanguages ?? (await pluginsWith("snippet")).map((p) => ({ id: PLUGIN_LANG + p.name, label: `${p.name} (plugin)` }));
  const all = () => [...(languages ?? []), ...plugins];
  try {
    languages ??= (await loadFormat()).snippetLanguages();
    const result = d.lang.startsWith(PLUGIN_LANG) ? await app.store.snippetWithPlugin(d.lang.slice(PLUGIN_LANG.length)) : await app.store.snippet(d.lang, d.showSecrets);
    if (app.dialog === d) app.dialog = { ...d, languages: all(), pluginLanguages: plugins, result, error: null };
  } catch (e) {
    if (app.dialog === d) app.dialog = { ...d, languages: all(), pluginLanguages: plugins, result: null, error: e?.message ?? String(e) };
  }
}

export function codeDialog(app, d) {
  const set = (changes) => {
    app.dialog = { ...app.dialog, ...changes };
    refreshCode(app);
  };
  const result = d.result;
  const ph = result?.placeholders ?? [];
  // A snippet plugin never gets secret values (decision 4): no "Include secret values" for it.
  const plugin = d.lang.startsWith(PLUGIN_LANG) ? d.lang.slice(PLUGIN_LANG.length) : null;
  return {
    headline: "Code",
    body: html`
      <div class="code-options">
        <lit-material-select id="snippet-lang" variant="outlined" label="Language" .value=${d.lang} @change=${(e) => set({ lang: e.target.value, ...(e.target.value.startsWith(PLUGIN_LANG) ? { showSecrets: false } : {}) })}>
          ${(d.languages ?? [{ id: d.lang, label: d.lang }]).map((l) => html`<lit-material-select-option value=${l.id}>${l.label}</lit-material-select-option>`)}
        </lit-material-select>
        <label class="check">
          <lit-material-checkbox id="snippet-secrets" .checked=${Boolean(d.showSecrets) && !plugin} ?disabled=${plugin} @change=${(e) => set({ showSecrets: e.target.checked })}></lit-material-checkbox>
          Include secret values
        </label>
      </div>
      ${d.error
        ? html`<p class="error-text">${d.error}</p>`
        : result
          ? html`<lit-material-code-block class="snippet" label=${(d.languages ?? []).find((l) => l.id === d.lang)?.label ?? d.lang} copyable>${result.code}</lit-material-code-block>`
          : html`<div class="skeleton block"></div>`}
      ${ph.length
        ? html`<p class="dialog-note snippet-placeholders">
            Replace ${ph.map((n, i) => html`${i ? ", " : ""}<code>{{${n}}}</code>`)} before running it: ${d.showSecrets ? "variables with no value" : "secrets and variables with no value"} are left as placeholders.
          </p>`
        : nothing}
      ${d.showSecrets ? html`<p class="dialog-note warn">This code contains secret values: don't share or commit it.</p>` : nothing}
      ${plugin ? html`<p class="dialog-note">Written by plugin ${plugin}, which gets the request with secrets as placeholders, never their values.</p>` : nothing}
      <p class="dialog-note">Scripts don't run for code. The CLI prints the same: <code>hitop snippet &lt;collection&gt; &lt;request&gt; ${plugin ? `--plugin ${plugin}` : `--lang ${d.lang}`}</code>.</p>`,
    footer: html`
      <div slot="actions" class="dialog-actions">
        <lit-material-button variant="filled" @click=${() => app.closeDialog()}>Close</lit-material-button>
      </div>`,
  };
}

const show = (v) => (v === undefined ? "" : typeof v === "string" ? JSON.stringify(v) : JSON.stringify(v));

function sourceSelect(id, label, value, sources, onChange) {
  return html`<lit-material-select id=${id} variant="outlined" label=${label} .value=${value ?? ""} @change=${(e) => onChange(e.target.value)}>
    ${sources.map((s) => html`<lit-material-select-option value=${s.id}>${s.label}</lit-material-select-option>`)}
  </lit-material-select>`;
}

// "Before" starts on a response that differs from "After": the newest earlier
// response of the same request, else any that differs. The newest history
// item is usually the current response itself, which compared "the same".
function defaultBefore(sources, right) {
  const differs = (s) => s.response?.status !== right.response?.status || s.response?.body !== right.response?.body;
  const others = sources.filter((s) => s !== right && differs(s));
  return others.find((s) => right.request && s.request === right.request) ?? others[0] ?? sources.find((s) => s !== right);
}

/** The Compare dialog; with `pick` it's the response pane's "Diff vs last" tab (selections go to `pick`). */
export function compareDialog(app, d, pick = null, only = null) {
  const sources = app.store.compareSources(only);
  const footer = html`
    <div slot="actions" class="dialog-actions">
      <lit-material-button variant="filled" @click=${() => app.closeDialog()}>Close</lit-material-button>
    </div>`;
  if (sources.length < 2) {
    return {
      headline: "Compare responses",
      body: html`<lit-material-empty-state size="compact" headline="Nothing to compare yet"
        >Send the request twice, or save a response as an example, then compare. History from before this version has no responses to compare.</lit-material-empty-state
      >`,
      footer,
    };
  }
  const right = sources.find((s) => s.id === d.right) ?? sources[0];
  const left = sources.find((s) => s.id === d.left) ?? defaultBefore(sources, right);
  const set = (changes) => (pick ? pick({ left: left.id, right: right.id, ...changes }) : (app.dialog = { ...app.dialog, left: left.id, right: right.id, ...changes }));
  const prefix = pick ? "diff" : "compare";
  const c = compareResponses(left.response, right.response);
  const changedHeaders = c.headers.filter((h) => h.kind !== "same");
  const rows = sideBySide(c.body.lines);
  return {
    headline: "Compare responses",
    body: html`
      <div class="compare-pick">
        ${sourceSelect(`${prefix}-left`, "Before", left.id, sources, (v) => set({ left: v }))}
        ${sourceSelect(`${prefix}-right`, "After", right.id, sources, (v) => set({ right: v }))}
      </div>
      <p class=${`compare-summary ${c.same ? "same" : ""}`}>${c.same ? "The responses are the same." : summary(c, changedHeaders)}</p>
      <section class="compare-status" aria-label="Status">
        <h3>Status</h3>
        <p>${c.status.same ? html`<span class="same">${c.status.before} in both</span>` : html`<del>${c.status.before ?? "none"}</del> → <ins>${c.status.after ?? "none"}</ins>`}</p>
      </section>
      <section class="compare-headers" aria-label="Headers">
        <h3>Headers</h3>
        ${changedHeaders.length
          ? html`<ul class="diff-list">
              ${changedHeaders.map(
                (h) => html`<li class=${h.kind}>
                  <code>${h.name}</code> <span class="kind">${h.kind}</span>
                  ${h.kind === "added" ? html`<ins>${h.after}</ins>` : h.kind === "removed" ? html`<del>${h.before}</del>` : html`<del>${h.before}</del> → <ins>${h.after}</ins>`}
                </li>`,
              )}
            </ul>`
          : html`<p class="hint">Same headers${c.headers.length ? ` (${c.headers.length})` : ""}.</p>`}
      </section>
      <section class="compare-body" aria-label="Body">
        <h3>Body ${c.body.kind === "json" ? html`<span class="hint">JSON, key order ignored</span>` : nothing}</h3>
        ${c.body.kind === "json" && c.body.changes.length
          ? html`<ul class="diff-list json-changes">
              ${c.body.changes.map(
                (x) => html`<li class=${x.kind}>
                  <code>${x.path}</code> <span class="kind">${x.kind}</span>
                  ${x.kind === "added" ? html`<ins>${show(x.after)}</ins>` : x.kind === "removed" ? html`<del>${show(x.before)}</del>` : html`<del>${show(x.before)}</del> → <ins>${show(x.after)}</ins>`}
                </li>`,
              )}
            </ul>`
          : nothing}
        ${c.body.kind === "binary"
          ? html`<p class=${c.body.same ? "hint" : ""}>${c.body.same
              ? `Same bytes (${kb(c.body.after?.size)}).`
              : `The binary bodies differ: ${c.body.before ? kb(c.body.before.size) : "none"} → ${c.body.after ? kb(c.body.after.size) : "none"}${c.body.before && c.body.after && c.body.before.size === c.body.after.size ? " (same size, other bytes)" : ""}.`}</p>`
          : c.body.same
          ? html`<p class="hint">Same body.</p>`
          : html`<div class="side-by-side" role="table" aria-label="Body, side by side">
              ${rows.map(
                (r) => html`<div class=${`row ${r.op}`} role="row">
                  <pre role="cell" class="left">${r.left ?? ""}</pre>
                  <pre role="cell" class="right">${r.right ?? ""}</pre>
                </div>`,
              )}
            </div>`}
      </section>`,
    footer,
  };
}

const kb = (n) => (n == null ? "?" : n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);

// UX2 (6): what differs, in words: "Different: the status, 3 headers and 2 values in the body."
function summary(c, headers) {
  const parts = [];
  if (!c.status.same) parts.push("the status");
  if (headers.length) parts.push(`${headers.length} header${headers.length === 1 ? "" : "s"}`);
  if (!c.body.same) parts.push(c.body.kind === "json" ? `${c.body.changes.length} value${c.body.changes.length === 1 ? "" : "s"} in the body` : "the body");
  const list = parts.length > 1 ? `${parts.slice(0, -1).join(", ")} and ${parts.at(-1)}` : parts[0];
  return `Different: ${list}.`;
}

// View with plugin (docs/PLUGINS_THREAT_MODEL.md decisions 3, 8, 9): an installed viewer plugin's
// text or table for the tab's response. The answer lives in this dialog only (never on the tab, in
// history or in storage) and is dropped when the response changed while the plugin ran.
export async function refreshPluginView(app) {
  const d = app.dialog;
  if (d?.kind !== "plugin-view") return;
  const viewers = d.viewers ?? (await pluginsWith("viewer"));
  const plugin = d.plugin ?? viewers[0]?.name ?? null;
  const tab = app.store.tabs.find((t) => t.id === d.tabId);
  if (app.dialog !== d) return;
  if (!plugin || !tab) {
    app.dialog = { ...d, viewers, plugin, view: null, error: null, running: false };
    return;
  }
  const response = tab.response;
  app.dialog = { ...d, viewers, plugin, view: null, error: null, running: true };
  const running = app.dialog;
  let view = null;
  let error = null;
  try {
    view = await app.store.viewWithPlugin(plugin, tab);
  } catch (e) {
    error = e?.message ?? String(e);
  }
  if (app.dialog !== running) return;
  const now = app.store.tabs.find((t) => t.id === d.tabId);
  if (now?.response !== response) error = "The response changed while the plugin ran: view it again";
  app.dialog = { ...running, view: error ? null : view, error, running: false };
}

export function pluginViewDialog(app, d) {
  const pick = (name) => {
    app.dialog = { ...app.dialog, plugin: name, view: null };
    refreshPluginView(app);
  };
  const viewers = d.viewers;
  const shown = d.view?.shown;
  const table = shown && typeof shown === "object" ? shown : null;
  const rows = table ? table.rows.slice(0, SHOWN_ROWS) : [];
  return {
    headline: "View with plugin",
    body: html`
      ${viewers && !viewers.length
        ? html`<p class="dialog-note">No installed plugin has a viewer hook. Plugins are installed in Settings → Plugins.</p>`
        : html`<div class="code-options">
            <lit-material-select id="plugin-viewer" variant="outlined" label="Viewer" .value=${d.plugin ?? ""} @change=${(e) => pick(e.target.value)}>
              ${(viewers ?? []).map((p) => html`<lit-material-select-option value=${p.name}>${p.name}</lit-material-select-option>`)}
            </lit-material-select>
          </div>`}
      ${d.error
        ? html`<p class="error-text plugin-view-error">${d.error}</p>`
        : d.running || !viewers
          ? html`<div class="skeleton block"></div>`
          : typeof shown === "string"
            ? html`<pre class="plugin-view-text" data-testid="plugin-view-text">${shown}</pre>`
            : table
              ? html`<div class="plugin-view-table">
                  <table data-testid="plugin-view-table">
                    <thead><tr>${table.columns.map((c) => html`<th scope="col">${c}</th>`)}</tr></thead>
                    <tbody>${rows.map((r) => html`<tr>${r.map((c) => html`<td>${c}</td>`)}</tr>`)}</tbody>
                  </table>
                </div>
                ${table.rows.length > rows.length ? html`<p class="dialog-note">Showing the first ${rows.length} of ${table.rows.length} rows.</p>` : nothing}`
              : nothing}
      ${d.view?.truncated ? html`<p class="dialog-note">The plugin got the first 1 MB of the body.</p>` : nothing}
      <p class="dialog-note">The plugin gets the response with secret values hidden, and its answer isn't kept. The CLI shows the same: <code>hitop send … --view ${d.plugin ?? "&lt;plugin&gt;"}</code>.</p>`,
    footer: html`
      <div slot="actions" class="dialog-actions">
        <lit-material-button variant="filled" @click=${() => app.closeDialog()}>Close</lit-material-button>
      </div>`,
  };
}
