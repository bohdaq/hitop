// A request's version history (sync-server library.rs `history`): the last 50 versions of a
// request in a HITOP Cloud collection, from the apps, GitHub or an AI assistant; Restore puts one
// back (a new save, so it's in the history too).
import { html, nothing } from "lit";

const SOURCES = { app: "Saved in HITOP", github: "From GitHub", mcp: "By an AI assistant" };

export async function loadRequestHistory(app, d) {
  const s = app.store;
  try {
    const r = await s.library.client.api("GET", `/v1/library/collection/${encodeURIComponent(d.collectionId)}/history?request=${encodeURIComponent(d.request.id)}`);
    if (app.dialog?.kind === "request-history") app.dialog = { ...app.dialog, history: r.history };
  } catch (e) {
    if (app.dialog?.kind === "request-history") app.dialog = { ...app.dialog, error: e.message || String(e), history: [] };
  }
}

function restore(app, d, entry) {
  const s = app.store;
  const { id: _id, ...fields } = entry.request;
  s.collections = s.collections.map((c) =>
    String(c.id) !== String(d.collectionId)
      ? c
      : { ...c, requests: c.requests.some((r) => r.id === d.request.id) ? c.requests.map((r) => (r.id === d.request.id ? { ...r, ...fields } : r)) : [...c.requests, { ...entry.request }] },
  );
  s.emit();
  app.closeDialog();
  app.toast(`Restored ${entry.request.name} as it was ${new Date(entry.at * 1000).toLocaleString()}.`);
}

export function historyDialog(app, d) {
  const list = d.history;
  const body = html`${d.error ? html`<p class="error-text" role="alert">${d.error}</p>` : nothing}
    ${!list
      ? html`<div class="skeleton block" aria-busy="true"></div>`
      : list.length
        ? html`<ul class="sync-list" aria-label="Versions">
            ${list.map((e, i) => html`<li class="sync-row">
              <span><b>${new Date(e.at * 1000).toLocaleString()}</b> <span class="hint">· ${SOURCES[e.source] ?? e.source}${e.deleted ? " · deleted" : ""}${i === 0 && !e.deleted ? " · current" : ""}</span><br />
                <code>${e.request.method ?? "GET"} ${e.request.url ?? ""}</code></span>
              ${i === 0 && !e.deleted ? nothing : html`<lit-material-button variant="outlined" @click=${() => restore(app, d, e)}>Restore</lit-material-button>`}
            </li>`)}
          </ul>`
        : html`<p class="hint">No versions yet: they're kept from now on, each time the request changes.</p>`}`;
  return {
    headline: `History of ${d.request.name}`,
    body,
    footer: html`<div slot="actions" class="dialog-actions"><lit-material-button variant="text" @click=${() => app.closeDialog()}>Close</lit-material-button></div>`,
  };
}
