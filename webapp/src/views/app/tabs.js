// A part of the app shell (HitopApp, ../hitop-app.js).
import { METHODS } from "../../store.js";
import { confirm } from "@lit-material/confirm";
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { showMenuFor, tagOf } from "./shared.js";

export const TabsPart = (Base) =>
  class extends Base {
  // --- tabs -----------------------------------------------------------------------

  openTabMenu(e, index) {
    e.preventDefault();
    this.menuTab = index;
    showMenuFor(e, this.querySelector("#tab-menu"));
  }

  renderTabs() {
    const s = this.store;
    return html`
      <div class="tab-strip">
        <lit-material-tabs
          aria-label="Open requests"
          scrollable
          reorderable
          .selected=${s.currentTab}
          @change=${(e) => s.selectTab(e.target.selected)}
          @reorder=${(e) => s.moveTab(e.detail.from, e.detail.to)}
        >
          ${s.tabs.map(
            (t, i) => html`<lit-material-tab id=${`tab-${i}`} title=${t.historyPreview ? `${t.title} (preview from History: opening another entry replaces it; edit to keep it)` : t.title} ?data-pinned=${Boolean(t.pinned)} ?data-preview=${Boolean(t.historyPreview)} @contextmenu=${(e) => this.openTabMenu(e, i)}
              @auxclick=${(e) => e.button === 1 && !t.pinned && (e.preventDefault(), this.closeTab(i))}>${t.pinned ? html`<span class="tab-pin" aria-hidden="true">${icon("push-pin")}</span><span class="visually-hidden">Pinned: </span>` : nothing}${this.tabLabel(t)}${s.isDirty(t)
                ? html`<span class="dirty-dot" aria-hidden="true">●</span><span class="visually-hidden">, unsaved changes</span>`
                : nothing}${s.tabs.length > 1 && !t.pinned
                ? html`<span class="tab-close" aria-hidden="true" title="Close tab (Alt+Shift+W)"
                    @pointerdown=${(e) => e.stopPropagation()} @mousedown=${(e) => e.stopPropagation()}
                    @click=${(e) => (e.stopPropagation(), e.preventDefault(), this.closeTab(i))}>${icon("x")}</span>`
                : nothing}</lit-material-tab>`,
          )}
        </lit-material-tabs>
        <lit-material-icon-button aria-label="All open tabs" title="All open tabs" id="tab-list-button" aria-haspopup="menu" @click=${(e) => this.querySelector("#tab-list").show(e.currentTarget)}>
          ${icon("list")}
        </lit-material-icon-button>
        <lit-material-icon-button aria-label="Request actions" id="tab-actions" @click=${(e) => ((this.menuTab = s.currentTab), this.querySelector("#tab-menu").show(e.currentTarget))}>
          ${icon("dots-three-vertical")}
        </lit-material-icon-button>
        <lit-material-icon-button aria-label="New request" title="New request in this collection" @click=${() => this.newRequest()}>${icon("plus")}</lit-material-icon-button>
      </div>
      <lit-material-menu id="tab-list" aria-label="Open tabs">
        ${s.tabs.map((t, i) => html`<lit-material-list-item interactive ?selected=${i === s.currentTab} @click=${() => s.selectTab(i)}>${this.tabLabel(t)}</lit-material-list-item>`)}
      </lit-material-menu>
      <lit-material-menu id="tab-menu">
        <lit-material-list-item interactive @click=${() => this.openDialog("rename-tab", { index: this.menuTab, name: s.tabs[this.menuTab]?.title ?? "" })}>Rename</lit-material-list-item>
        <lit-material-list-item interactive @click=${() => s.duplicateTab(this.menuTab)}>Duplicate</lit-material-list-item>
        <lit-material-list-item interactive @click=${() => s.togglePin(this.menuTab ?? s.currentTab)}>${s.tabs[this.menuTab ?? s.currentTab]?.pinned ? "Unpin tab" : "Pin tab"}</lit-material-list-item>
        <lit-material-list-item interactive ?disabled=${s.tabs.length < 2} @click=${() => this.closeTab(this.menuTab)}>Close tab</lit-material-list-item>
        <lit-material-list-item interactive ?disabled=${s.tabs.length < 2} @click=${() => this.closeTabsFrom(this.menuTab ?? s.currentTab, "others")}>Close other tabs</lit-material-list-item>
        <lit-material-list-item interactive ?disabled=${(this.menuTab ?? s.currentTab) >= s.tabs.length - 1} @click=${() => this.closeTabsFrom(this.menuTab ?? s.currentTab, "right")}>Close tabs to the right</lit-material-list-item>
        ${s.tabs[this.menuTab ?? s.currentTab] && s.isDirty(s.tabs[this.menuTab ?? s.currentTab])
          ? html`<lit-material-list-item interactive ?disabled=${s.tabs.length < 2} @click=${() => this.saveAndClose(this.menuTab)}>Save and close</lit-material-list-item>`
          : nothing}
        <lit-material-list-item interactive @click=${() => this.deleteTabRequest(this.menuTab)}>Delete request</lit-material-list-item>
      </lit-material-menu>`;
  }

  // UX2: a tab's coloured method tag (always, auto-named tabs too) and its title without the method
  // the tag already says ("GET /users" → GET + "/users"; GraphQL's "POST /graphql" → GQL + "/graphql").
  // The tag is drawn by CSS; the text has the method once for innerText and screen readers.
  tabLabel(t) {
    const tag = tagOf(t);
    const m = /^([A-Z]+) (.+)$/.exec(t.title || "");
    const label = m && (m[1] === tag || (t.autoName && METHODS.includes(m[1]))) ? m[2] : t.title;
    return html`<span class=${`method-tag tab-method m-${tag}`} data-tag=${tag} aria-hidden="true"></span><span class="visually-hidden">${tag} </span><span class="tab-title">${label}</span>`;
  }

  async deleteTabRequest(index) {
    const tab = this.store.tabs[index];
    if (!tab) return;
    const ok = await confirm({
      headline: `Delete ${tab.title}?`,
      message: tab.loadedRequestId ? "The tab closes and the request is removed from its collection." : "The tab closes.",
      confirmLabel: "Delete",
      destructive: true,
    });
    if (ok) this.store.deleteTabRequest(index);
  }
  };
