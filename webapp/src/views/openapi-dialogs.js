// OpenAPI dialogs: import a spec (URL or pasted YAML/JSON), the import report,
// and sync of a collection imported from one (preview of the diff, then apply).
// The importer and sync are hitop-core's, through hitop-wasm (store.js).
import { html, nothing } from "lit";
import { icon } from "../icons.js";
import { issueText } from "../pm.js";

export const OPENAPI_DIALOGS = ["openapi-import", "openapi-report", "openapi-sync"];

const plural = (n, one) => `${n} ${one}${n === 1 ? "" : "s"}`;
const errorText = (e) => String(e?.message ?? e);

// The collection's record from its import: { source, version, ... } or null.
export const openapiRecord = (collection) => (collection?.openapi && typeof collection.openapi === "object" ? collection.openapi : null);

function actions(app, label, { back = null } = {}) {
  return html`
    <div slot="actions" class="dialog-actions">
      <lit-material-button variant="text" @click=${() => (back ? back() : app.closeDialog())}>${back ? "Back" : "Cancel"}</lit-material-button>
      <lit-material-button variant="filled" type="submit" form="dialog-form">${label}</lit-material-button>
    </div>`;
}

function sourceFields(d, { url = "" } = {}) {
  return html`
    <lit-material-text-field id="spec-url" variant="outlined" label="Spec URL" type="url" placeholder="https://api.example.com/openapi.yaml" .value=${d.url ?? url} autofocus
      ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-text-field>
    <lit-material-textarea id="spec-text" variant="outlined" label="Or paste the spec (YAML or JSON)" rows="8" .value=${d.text ?? ""}></lit-material-textarea>`;
}

function issueList(issues) {
  return html`<ul class="import-report">
    ${issues.map((i) => html`<li><span>${issueText(i)}</span>${i.text ? html`<code>${i.text}</code>` : nothing}</li>`)}
  </ul>`;
}

// The import report: what was mapped, what wasn't, what to check.
function reportBody(r) {
  const m = r.mapped;
  return html`
    <p class="dialog-note">
      ${r.name} is imported from OpenAPI ${m.openapi}: ${plural(m.operations, "operation")} became ${plural(m.requests, "request")}
      in ${plural(m.folders, "folder")}, with ${plural(m.examples, "saved example")}. Variables: ${m.variables.join(", ")}.
      ${m.auth.length ? html`Auth: ${m.auth.join("; ")}.` : nothing}
    </p>
    ${r.skipped.length ? html`<h3 class="report-heading">Not imported</h3>${issueList(r.skipped)}` : nothing}
    ${r.warnings.length ? html`<h3 class="report-heading">To check</h3>${issueList(r.warnings)}` : nothing}`;
}

function opLine(c) {
  return c.operationId ? `${c.operation} (${c.operationId})` : c.operation;
}

// A header or query row of a row-by-row merge (hitop-core sync RowChange),
// in the words of the CLI's `hitop openapi sync`.
export function rowText(r) {
  const what = r.field === "headers" ? "header" : "query parameter";
  if (r.outcome === "updated") return `${what} ${r.name} ${r.change} by the spec: updated`;
  if (r.outcome === "kept") return `${what} ${r.name} ${r.change} by the spec: kept yours`;
  return `${what} ${r.name} ${r.change} by you: kept`;
}

function rowSign(r) {
  if (r.outcome !== "updated") return "=";
  return r.change === "added" ? "+" : r.change === "removed" ? "-" : "~";
}

function diffBody(p) {
  const d = p.diff;
  const row = (sign, c, text) => html`<li class=${`diff-${sign === "+" ? "add" : sign === "-" ? "del" : "chg"}`}><span><b>${sign}</b> ${opLine(c)}</span><code>${text}</code></li>`;
  const changedText = (c) =>
    [c.updated?.length ? `updated ${c.updated.join(", ")}` : "", c.kept?.length ? `kept your ${c.kept.join(", ")} (the spec changed it too)` : ""].filter(Boolean).join("; ") || "operation renamed";
  const cc = d.collection;
  const nothingToDo = !d.added.length && !d.changed.length && !d.removed.length && !cc.updated?.length && !cc.variablesAdded?.length;
  return html`
    <p class="dialog-note openapi-summary">${d.added.length} added, ${d.changed.length} changed, ${d.removed.length} removed, ${d.unchanged} unchanged.
      Your names, scripts, examples and edited fields stay.</p>
    ${nothingToDo
      ? html`<p class="dialog-note">The collection is up to date.</p>`
      : html`<ul class="import-report openapi-diff">
          ${d.added.map((c) => row("+", c, `${c.name} (new)`))}
          ${d.changed.map(
            (c) => html`${row("~", c, `${c.name}: ${changedText(c)}`)}${(c.rows ?? []).map(
              (r) => html`<li class=${`diff-row diff-${r.outcome === "updated" ? (r.change === "added" ? "add" : r.change === "removed" ? "del" : "chg") : "keep"}`}><span><b>${rowSign(r)}</b> ${r.field === "headers" ? "Header" : "Query"} ${r.name}</span><code>${rowText(r)}</code></li>`,
            )}`,
          )}
          ${d.removed.map((c) => row("-", c, `${c.name}: gone from the spec, ${d.pruned ? "deleted" : "kept"}`))}
          ${cc.updated?.length ? html`<li><span>Collection</span><code>updated ${cc.updated.join(", ")}</code></li>` : nothing}
          ${cc.kept?.length ? html`<li><span>Collection</span><code>kept your ${cc.kept.join(", ")}</code></li>` : nothing}
          ${cc.variablesAdded?.length ? html`<li><span>Collection</span><code>new variables ${cc.variablesAdded.join(", ")}</code></li>` : nothing}
        </ul>`}`;
}

// { headline, body, footer } for one of OPENAPI_DIALOGS.
export function openapiDialog(app, d) {
  const s = app.store;
  switch (d.kind) {
    case "openapi-import":
      return {
        headline: "Import OpenAPI",
        body: html`
          <p class="dialog-note">An OpenAPI 3.0 or 3.1 spec becomes a new collection: a folder per tag, a request per operation, the server as the baseUrl variable.</p>
          ${sourceFields(d)}`,
        footer: actions(app, "Import"),
      };
    case "openapi-report":
      return { headline: "Check after import", body: reportBody(d.report), footer: actions(app, "Done") };
    case "openapi-sync": {
      const col = s.collection(d.id);
      const rec = openapiRecord(col);
      if (d.preview) {
        return {
          headline: `Sync ${col?.name ?? ""}`,
          body: diffBody(d.preview),
          footer: actions(app, d.preview.diff.pruned && d.preview.diff.removed.length ? "Apply and delete" : "Apply", { back: () => (app.dialog = { ...d, preview: null, submitted: false }) }),
        };
      }
      return {
        headline: `Sync ${col?.name ?? ""} with OpenAPI`,
        body: html`
          <p class="dialog-note">New operations are added; changed ones are updated where you didn't edit them. You see the changes before anything is saved.</p>
          ${sourceFields(d, { url: rec?.source ?? "" })}
          <label class="check">
            <lit-material-checkbox id="spec-prune" .checked=${Boolean(d.prune)}></lit-material-checkbox>
            Delete requests whose operation is gone from the spec
          </label>`,
        footer: actions(app, "Preview"),
      };
    }
  }
  return { headline: "", body: nothing, footer: nothing };
}

// Submit for OPENAPI_DIALOGS (the form's submit, from HitopApp.submitDialog).
export async function submitOpenapi(app, d) {
  const s = app.store;
  if (d.kind === "openapi-report") return app.closeDialog();
  const url = app.field("spec-url").trim();
  const text = app.field("spec-text");
  if (d.kind === "openapi-sync" && d.preview) {
    s.applyOpenapiSync(d.id, d.preview);
    return app.closeDialog();
  }
  if (!url && !text.trim()) {
    app.dialog = { ...d, submitted: false, error: "Enter the spec's URL, or paste the spec" };
    return;
  }
  const prune = Boolean(app.querySelector("#dialog #spec-prune")?.checked);
  app.dialog = { ...d, url, text, prune, busy: true, error: null };
  try {
    if (d.kind === "openapi-import") {
      const report = await s.importOpenapi({ url, text: url ? "" : text });
      app.dialog = { kind: "openapi-report", report };
    } else {
      const preview = await s.previewOpenapiSync(d.id, { url, text: url ? "" : text }, prune);
      app.dialog = { ...d, url, text, prune, preview, submitted: false };
    }
  } catch (e) {
    app.dialog = { ...d, url, text, prune, submitted: false, error: `Couldn't read the spec: ${errorText(e)}` };
  }
}

// The button on a collection page imported from a spec.
export function syncButton(app, collection) {
  if (!openapiRecord(collection)) return nothing;
  return html`<lit-material-button variant="outlined" @click=${() => app.openDialog("openapi-sync", { id: collection.id })}>${icon("arrows-clockwise")} Sync with OpenAPI</lit-material-button>`;
}
