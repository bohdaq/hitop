// A part of the app shell (HitopApp, ../hitop-app.js).
import { html, nothing } from "lit";

import { methodLabel } from "../../store.js";
import { quickOpenItems, rankItems } from "../../quick-open.js";

export const QuickOpenPart = (Base) =>
  class extends Base {
  // --- quick open (Mod+P) ---------------------------------------------------------

  quickResults(d = this.dialog) {
    const s = this.store;
    // Collections, environments and history only once something is typed.
    const q = String(d?.query ?? "").trim();
    const environments = q ? s.visibleEnvironments.map((e) => ({ ...e, label: s.environmentLabel(e) })) : [];
    const history = q ? s.history : [];
    const liveName = (r, collectionId) => s.autoNameTab(r, collectionId)?.title ?? null;
    return rankItems(quickOpenItems({ collections: s.visibleCollections, tabs: s.tabs, methodLabel, environments, history, liveName }), d?.query ?? "", 50);
  }

  quickPick(item) {
    if (!item) return;
    const s = this.store;
    this.closeDialog();
    if (item.kind === "tab") s.selectTab(item.tabIndex);
    else if (item.kind === "collection") this.go("collection", item.collectionId);
    else if (item.kind === "environment") s.show("environments", null, item.environmentId);
    else if (item.kind === "history") s.openHistoryItem(item.entry);
    else s.openRequest(item.request, item.collectionId);
  }

  quickKey(e) {
    const d = this.dialog;
    const n = this.quickResults(d).length;
    if (!n || !["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    e.preventDefault();
    const active = e.key === "Home" ? 0 : e.key === "End" ? n - 1 : (d.active + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    this.dialog = { ...d, active };
    this.updateComplete.then(() => this.querySelector(`#quick-opt-${active}`)?.scrollIntoView({ block: "nearest" }));
  }

  quickDialog(d) {
    const results = this.quickResults(d);
    const active = Math.min(d.active ?? 0, Math.max(0, results.length - 1));
    const title = (item) => {
      if (!item.match.length) return item.title;
      const at = new Set(item.match);
      return [...item.title].map((ch, i) => (at.has(i) ? html`<mark>${ch}</mark>` : ch));
    };
    const body = html`
      <input id="quick-input" class="quick-input" type="text" autofocus autocomplete="off" spellcheck="false"
        placeholder="Find a request, collection, environment or URL from history" aria-label="Find a request"
        role="combobox" aria-expanded="true" aria-controls="quick-list" aria-autocomplete="list"
        aria-activedescendant=${results.length ? `quick-opt-${active}` : ""}
        .value=${d.query ?? ""} @input=${(e) => (this.dialog = { ...this.dialog, query: e.target.value, active: 0 })} @keydown=${(e) => this.quickKey(e)} />
      ${results.length
        ? html`<ul id="quick-list" class="quick-list" role="listbox" aria-label="Requests">
            ${results.map(
              (item, i) => html`<li id=${`quick-opt-${i}`} role="option" aria-selected=${String(i === active)} class=${i === active ? "active" : ""}
                @mousemove=${() => i !== this.dialog.active && (this.dialog = { ...this.dialog, active: i })} @click=${() => this.quickPick(item)}>
                <span class=${`method-tag m-${item.method}`}>${item.method}</span>
                <span class="quick-text"><span class="quick-title">${title(item)}</span><span class="quick-detail">${item.detail}${item.url ? html` · <span class="quick-url">${item.url}</span>` : nothing}</span></span>
                ${item.open ? html`<span class="quick-open-tag">open</span>` : nothing}
              </li>`,
            )}
          </ul>`
        : html`<p class="hint" id="quick-list" role="status">No request matches “${d.query}”.</p>`}`;
    return { body, results, active };
  }

  go(kind, collectionId = null) {
    this.navOpen = false;
    // Signed out, the Account page is the Sign in dialog, over what's on screen.
    if (kind === "sync" && !this.account()) {
      this.openSignIn();
      return;
    }
    this.store.show(kind, collectionId);
  }
  };
