// History as a page (it was a dialog until 1.2): newest first, grouped by
// day, a search over method, URL, status and run names, open an entry in the
// current tab, delete one, clear all, Compare responses. HITOP keeps the
// last 50 (the CLI's MAX_HISTORY; the desktop shares history.json with it).
import { LitElement, html, nothing } from "lit";
import { icon } from "../icons.js";

const dayOf = (iso) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toDateString();
};

function dayLabel(day) {
  const today = new Date();
  const yesterday = new Date(today.getTime() - 86_400_000);
  if (day === today.toDateString()) return "Today";
  if (day === yesterday.toDateString()) return "Yesterday";
  return new Date(day).toLocaleDateString(undefined, { dateStyle: "medium" });
}

class HitopHistory extends LitElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    app: { attribute: false },
    query: { state: true },
  };

  constructor() {
    super();
    this.query = "";
  }

  createRenderRoot() {
    return this;
  }

  text(item) {
    const what = item.isCollectionRun ? `${item.collectionName} ${item.requestName}` : "";
    return `${item.method} ${item.url} ${item.statusCode ?? "error"} ${what}`.toLowerCase();
  }

  render() {
    const s = this.store;
    const q = this.query.trim().toLowerCase();
    const items = [...s.history].reverse().filter((item) => !q || q.split(/\s+/).every((w) => this.text(item).includes(w)));
    const groups = [];
    for (const item of items) {
      const day = dayOf(item.timestamp);
      if (groups.at(-1)?.day !== day) groups.push({ day, items: [] });
      groups.at(-1).items.push(item);
    }
    return html`
      <div class="page history-page">
        <header class="page-header">
          <h1>History</h1>
          <div class="page-actions">
            ${s.history.some((h) => h.response) ? html`<lit-material-button variant="text" @click=${() => this.app.openDialog("compare")}>${icon("git-diff")} Compare responses</lit-material-button>` : nothing}
            ${s.history.length ? html`<lit-material-button variant="text" @click=${() => this.app.clearHistory()}>${icon("trash")} Clear history</lit-material-button>` : nothing}
          </div>
        </header>
        <p class="page-lead">Requests you send and collection runs, newest first. HITOP keeps the last 50. Open one to get it back as a new request in My requests, with the response it had.</p>
        ${s.history.length
          ? html`<input type="search" class="tree-filter history-filter" placeholder="Search history: method, URL, status" aria-label="Search history" .value=${this.query}
              @input=${(e) => (this.query = e.target.value)} />`
          : nothing}
        ${!s.history.length
          ? html`<lit-material-empty-state size="compact" headline="No requests yet">Requests you send appear here, newest first. HITOP keeps the last 50.</lit-material-empty-state>`
          : !items.length
            ? html`<p class="hint" role="status">Nothing in History matches “${this.query}”.</p>`
            : groups.map(
                (g) => html`<section class="history-day" aria-label=${dayLabel(g.day)}>
                  <h2>${dayLabel(g.day)}</h2>
                  <ol class="history-list">
                    ${g.items.map(
                      (item) => html`<li class="history-row">
                        <button type="button" class="history-item" @click=${() => s.openHistoryItem(item)}>
                          <span class=${`status-chip ${item.success ? "ok" : "bad"}`}>${item.statusCode || "Error"}</span>
                          <span class="method">${item.method}</span>
                          <span class="history-url">${item.isCollectionRun ? `${item.collectionName} → ${item.requestName}` : item.url}</span>
                          <time datetime=${item.timestamp}>${new Date(item.timestamp).toLocaleTimeString(undefined, { timeStyle: "short" })}</time>
                        </button>
                        <lit-material-icon-button aria-label=${`Delete ${item.method} ${item.url} from History`} title="Delete from History" @click=${() => s.deleteHistoryItem(item)}>${icon("trash")}</lit-material-icon-button>
                      </li>`,
                    )}
                  </ol>
                </section>`,
              )}
      </div>`;
  }
}

customElements.define("hitop-history", HitopHistory);
