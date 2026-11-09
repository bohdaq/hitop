// The Code and Compare dialogs: a request as code (hitop-core's snippet
// generator through hitop-wasm) and two responses side by side.
// hitop-app.js renders them in its one dialog; each returns
// { headline, body, footer }.
import { html, nothing } from "lit";
import { compareResponses, sideBySide } from "../diff.js";
import { loadFormat } from "../wasm/format.js";

export const WIDE_DIALOGS = ["code", "compare", "git"];

let languages = null;

// Fills in the snippet for the dialog's language (async: the wasm module).
export async function refreshCode(app) {
  const d = app.dialog;
  if (d?.kind !== "code") return;
  try {
    languages ??= (await loadFormat()).snippetLanguages();
    const result = await app.store.snippet(d.lang, d.showSecrets);
    if (app.dialog === d) app.dialog = { ...d, languages, result, error: null };
  } catch (e) {
    if (app.dialog === d) app.dialog = { ...d, languages: languages ?? [], result: null, error: e?.message ?? String(e) };
  }
}

export function codeDialog(app, d) {
  const set = (changes) => {
    app.dialog = { ...app.dialog, ...changes };
    refreshCode(app);
  };
  const result = d.result;
  const ph = result?.placeholders ?? [];
  return {
    headline: "Code",
    body: html`
      <div class="code-options">
        <lit-material-select id="snippet-lang" variant="outlined" label="Language" .value=${d.lang} @change=${(e) => set({ lang: e.target.value })}>
          ${(d.languages ?? [{ id: d.lang, label: d.lang }]).map((l) => html`<lit-material-select-option value=${l.id}>${l.label}</lit-material-select-option>`)}
        </lit-material-select>
        <label class="check">
          <lit-material-checkbox id="snippet-secrets" .checked=${Boolean(d.showSecrets)} @change=${(e) => set({ showSecrets: e.target.checked })}></lit-material-checkbox>
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
      <p class="dialog-note">Scripts don't run for code. The CLI prints the same: <code>hitop snippet &lt;collection&gt; &lt;request&gt; --lang ${d.lang}</code>.</p>`,
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
export function compareDialog(app, d, pick = null) {
  const sources = app.store.compareSources();
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
        ${c.body.same
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

function summary(c, headers) {
  const parts = [];
  if (!c.status.same) parts.push("status");
  if (headers.length) parts.push(`${headers.length} header${headers.length === 1 ? "" : "s"}`);
  if (!c.body.same) parts.push(c.body.kind === "json" ? `${c.body.changes.length} body path${c.body.changes.length === 1 ? "" : "s"}` : "body");
  return `Differs: ${parts.join(", ")}.`;
}
