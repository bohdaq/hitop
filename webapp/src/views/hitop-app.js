// The app shell: sidebar with collections, the request tabs, the main view,
// and every dialog. Light DOM, so the global theme and app.css style it.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { repeat } from "lit/directives/repeat.js";
import { unsafeSVG } from "lit/directives/unsafe-svg.js";
import { mark } from "../../../site/src/brand/mark.js";
import { icon } from "../icons.js";
import { Store } from "../store.js";
import "./request-view.js";
import "./collection-views.js";

export class HitopApp extends LitElement {
  static properties = {
    navOpen: { state: true },
    dialog: { state: true },
    exportFormat: { state: true },
  };

  constructor() {
    super();
    this.store = new Store();
    this.navOpen = false;
    this.dialog = null; // { kind, ...data }
    this.exportFormat = "hitop";
    this.store.addEventListener("change", () => this.requestUpdate());
    this.store.addEventListener("notify", (e) => this.toast(e.detail));
  }

  createRenderRoot() {
    return this;
  }

  toast(message) {
    this.querySelector("#toast")?.show(message);
  }

  go(kind, collectionId = null) {
    this.navOpen = false;
    this.store.show(kind, collectionId);
  }

  // --- dialogs ----------------------------------------------------------------

  async openDialog(kind, data = {}) {
    this.dialog = { kind, ...data };
    await this.updateComplete;
    const dlg = this.querySelector("#dialog");
    dlg?.show();
    // The dialog focuses itself as it opens; move focus to the first field after.
    await dlg?.updateComplete;
    requestAnimationFrame(() => dlg?.querySelector("[autofocus]")?.focus());
  }

  closeDialog() {
    this.querySelector("#dialog")?.close();
  }

  field(id) {
    return this.querySelector(`#dialog #${id}`)?.value ?? "";
  }

  // The fields' inputs live in shadow DOM, so Enter doesn't submit the form
  // by itself. Enter in a textarea still adds a line.
  dialogKey(e) {
    if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
    if (e.composedPath().some((n) => n.localName === "textarea" || n.localName === "lit-material-select")) return;
    e.preventDefault();
    e.currentTarget.requestSubmit();
  }

  submitDialog(e) {
    e.preventDefault();
    const s = this.store;
    const d = this.dialog;
    // Enter can reach the form twice (the field's own implicit submission and
    // dialogKey); act once per opening.
    if (!d || d.submitted) return;
    d.submitted = true;
    if (d.kind === "add-collection") s.addCollection(this.field("name"));
    if (d.kind === "add-environment") s.addEnvironment(this.field("name"));
    if (d.kind === "rename-environment") s.renameEnvironment(d.id, this.field("name"));
    if (d.kind === "import-environments") {
      try {
        s.importEnvironments(this.field("json"));
      } catch (err) {
        this.dialog = { ...d, submitted: false, error: `That isn't an environment file HITOP can read: ${err.message}` };
        return;
      }
    }
    if (d.kind === "rename-collection") s.renameCollection(d.id, this.field("name"));
    if (d.kind === "rename-tab") s.renameTab(d.index, this.field("name"));
    if (d.kind === "save") {
      const collectionId = Number.isNaN(Number(this.field("collection"))) ? this.field("collection") : Number(this.field("collection"));
      const target = s.collections.find((c) => String(c.id) === String(collectionId));
      s.saveRequest(this.field("name"), target?.id);
    }
    if (d.kind === "import") {
      try {
        s.importJson(this.field("json"));
      } catch (err) {
        this.dialog = { ...d, submitted: false, error: `That isn't a collection file HITOP can read: ${err.message}` };
        return;
      }
    }
    this.closeDialog();
  }

  async deleteCollection(collection) {
    const n = collection.requests?.length ?? 0;
    const ok = await confirm({
      headline: `Delete ${collection.name}?`,
      message: `Its ${n} request${n === 1 ? "" : "s"} and variables are deleted too. Export first if you want a copy.`,
      confirmLabel: "Delete",
      destructive: true,
    });
    if (ok) this.store.deleteCollection(collection.id);
  }

  async deleteRequest(request, collectionId) {
    const ok = await confirm({ headline: `Delete ${request.name}?`, message: "The request is removed from its collection.", confirmLabel: "Delete", destructive: true });
    if (ok) this.store.deleteRequest(request, collectionId);
  }

  async deleteEnvironment(env) {
    const ok = await confirm({ headline: `Delete ${env.name}?`, message: "Its variables are deleted too.", confirmLabel: "Delete", destructive: true });
    if (ok) this.store.deleteEnvironment(env.id);
  }

  async clearHistory() {
    const ok = await confirm({ headline: "Clear history?", message: "All saved history entries are removed.", confirmLabel: "Clear", destructive: true });
    if (ok) this.store.clearHistory();
  }

  renderDialog() {
    const d = this.dialog;
    if (!d) return html`<lit-material-dialog id="dialog"></lit-material-dialog>`;
    const s = this.store;
    const actions = (label, destructive = false) => html`
      <div slot="actions" class="dialog-actions">
        <lit-material-button variant="text" @click=${() => this.closeDialog()}>Cancel</lit-material-button>
        <lit-material-button variant="filled" type="submit" form="dialog-form" class=${destructive ? "destructive" : ""}>${label}</lit-material-button>
      </div>`;
    let headline = "";
    let body = nothing;
    let footer = nothing;
    switch (d.kind) {
      case "add-collection":
        headline = "New collection";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Collection name" required autofocus></lit-material-text-field>`;
        footer = actions("Create");
        break;
      case "add-environment":
        headline = "New environment";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Environment name" placeholder="Staging" required autofocus></lit-material-text-field>`;
        footer = actions("Create");
        break;
      case "rename-environment":
        headline = "Rename environment";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Environment name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "import-environments":
        headline = "Import environments";
        body = html`
          <p class="dialog-note">Paste a HITOP environments export or a Postman environment. They are added to the ones you have.</p>
          <lit-material-textarea id="json" variant="outlined" label="Environment JSON" rows="10" required autofocus
            ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-textarea>`;
        footer = actions("Import");
        break;
      case "rename-collection":
        headline = "Rename collection";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Collection name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "rename-tab":
        headline = "Rename request";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Request name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "save":
        headline = d.overwrite ? "Save request" : "Save to a collection";
        body = html`
          <lit-material-text-field id="name" variant="outlined" label="Request name" .value=${d.name} required autofocus></lit-material-text-field>
          <lit-material-select id="collection" variant="outlined" label="Collection" .value=${String(d.collectionId ?? "")} required>
            ${s.collections.map((c) => html`<lit-material-select-option value=${String(c.id)}>${c.name}</lit-material-select-option>`)}
          </lit-material-select>`;
        footer = actions("Save");
        break;
      case "import":
        headline = "Import collections";
        body = html`
          <p class="dialog-note">Paste HITOP, Postman v2.1 or Bruno JSON. Importing replaces every collection you have now.</p>
          <lit-material-textarea id="json" variant="outlined" label="Collection JSON" rows="10" required autofocus
            ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-textarea>`;
        footer = actions("Replace and import", true);
        break;
      case "export": {
        headline = "Export collections";
        const json = s.exportJson(this.exportFormat);
        const name = this.exportFormat === "postman" ? "hitop-postman.json" : "hitop-collections.json";
        body = html`
          <lit-material-select variant="outlined" label="Format" .value=${this.exportFormat} @change=${(e) => (this.exportFormat = e.target.value)}>
            <lit-material-select-option value="hitop">HITOP</lit-material-select-option>
            <lit-material-select-option value="postman">Postman v2.1</lit-material-select-option>
          </lit-material-select>
          <lit-material-code-block label=${name} expandable>${json}</lit-material-code-block>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
            <lit-material-button variant="filled" @click=${() => this.download(name, json)}>${icon("file-arrow-down")} Download</lit-material-button>
          </div>`;
        break;
      }
      case "history":
        headline = "History";
        body = s.history.length
          ? html`<ol class="history-list">
              ${[...s.history].reverse().map(
                (item) => html`
                  <li>
                    <button type="button" class="history-item" @click=${() => (s.openHistoryItem(item), this.closeDialog())}>
                      <span class=${`status-chip ${item.success ? "ok" : "bad"}`}>${item.statusCode || "Error"}</span>
                      <span class="method">${item.method}</span>
                      <span class="history-url">${item.isCollectionRun ? `${item.collectionName} → ${item.requestName}` : item.url}</span>
                      <time datetime=${item.timestamp}>${new Date(item.timestamp).toLocaleTimeString()}</time>
                    </button>
                  </li>`,
              )}
            </ol>`
          : html`<lit-material-empty-state size="compact" headline="No requests yet">Requests you send appear here, newest first. HITOP keeps the last 50.</lit-material-empty-state>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            ${s.history.length ? html`<lit-material-button variant="text" @click=${() => (this.closeDialog(), this.clearHistory())}>Clear history</lit-material-button>` : nothing}
            <lit-material-button variant="filled" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
    }
    return html`
      <lit-material-dialog id="dialog" @close=${() => (this.dialog = null)}>
        <span slot="headline">${headline}</span>
        <form id="dialog-form" class="dialog-form" @submit=${(e) => this.submitDialog(e)} @keydown=${(e) => this.dialogKey(e)}>${body}</form>
        ${footer}
      </lit-material-dialog>`;
  }

  download(name, text) {
    const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
    const a = Object.assign(document.createElement("a"), { href: url, download: name });
    a.click();
    URL.revokeObjectURL(url);
    this.toast(`Downloaded ${name}`);
  }

  // --- sidebar ------------------------------------------------------------------

  renderSidebar() {
    const s = this.store;
    const { kind, collectionId } = s.view;
    const activeRequest = kind === "request" ? s.tab?.loadedRequestId : null;
    return html`
      <aside class=${`sidebar ${this.navOpen ? "open" : ""}`} aria-label="Collections">
        <a class="brand" href="/" aria-label="HITOP home">
          ${unsafeSVG(mark())}
          <span>HITOP</span>
        </a>
        <lit-material-button variant="tonal" class="new-request" @click=${() => (this.navOpen = false, s.newRequestInFirstCollection())}>
          ${icon("plus")} New request
        </lit-material-button>
        <lit-material-select class="env-select" variant="outlined" label="Environment" .value=${s.activeEnvironmentId ?? "none"}
          @change=${(e) => {
            if (e.target.value !== "__manage") return s.setActiveEnvironment(e.target.value === "none" ? null : e.target.value);
            e.target.value = s.activeEnvironmentId ?? "none"; // "Manage" is an action, not a choice
            this.go("environments");
          }}>
          <lit-material-select-option value="none">No environment</lit-material-select-option>
          ${s.environments.map((env) => html`<lit-material-select-option value=${env.id}>${env.name}</lit-material-select-option>`)}
          <lit-material-select-option value="__manage">Manage environments…</lit-material-select-option>
        </lit-material-select>
        <nav class="tree" aria-label="Collections">
          <button type="button" class="tree-row heading" aria-current=${kind === "collections" ? "page" : "false"} @click=${() => this.go("collections")}>
            ${icon("folders")}<span>Collections</span>
          </button>
          <ul>
            ${s.collections.map(
              (c) => html`
                <li>
                  <button type="button" class="tree-row collection" aria-current=${kind === "collection" && collectionId === c.id ? "page" : "false"} @click=${() => this.go("collection", c.id)}>
                    ${icon("folder-simple")}<span>${c.name}</span>
                  </button>
                  <ul>
                    <li>
                      <button type="button" class="tree-row leaf" aria-current=${kind === "variables" && collectionId === c.id ? "page" : "false"} @click=${() => this.go("variables", c.id)}>
                        ${icon("sliders-horizontal")}<span>Variables</span>
                      </button>
                    </li>
                    ${c.requests?.length
                      ? html`<li>
                          <lit-material-sortable role="list" label=${`Requests in ${c.name}`} @sort=${(e) => s.moveRequest(c.id, e.detail.from, e.detail.to)}>
                            ${repeat(
                              c.requests,
                              (r) => r.id,
                              (r) => html`<div role="listitem" class="request-item" data-sort-id=${r.id} data-sort-label=${r.name}>
                                <button type="button" class="tree-row leaf" aria-current=${activeRequest === r.id ? "page" : "false"} @click=${() => ((this.navOpen = false), s.openRequest(r, c.id))}>
                                  <span class=${`method-tag m-${r.method}`}>${r.method}</span><span>${r.name}</span>
                                </button>
                                <button type="button" class="sort-handle" data-sort-handle aria-label=${`Reorder ${r.name}`}>${icon("dots-six-vertical")}</button>
                              </div>`,
                            )}
                          </lit-material-sortable>
                        </li>`
                      : nothing}
                  </ul>
                </li>`,
            )}
          </ul>
        </nav>
        <div class="sidebar-footer">
          <button type="button" class="tree-row" aria-current=${kind === "environments" ? "page" : "false"} @click=${() => this.go("environments")}>${icon("globe")}<span>Environments</span></button>
          <button type="button" class="tree-row" aria-current=${kind === "cookies" ? "page" : "false"} @click=${() => this.go("cookies")}>${icon("cookie")}<span>Cookies</span></button>
          <button type="button" class="tree-row" @click=${() => this.openDialog("history")}>${icon("clock-counter-clockwise")}<span>History</span></button>
          <button type="button" class="tree-row" @click=${() => this.openDialog("export")}>${icon("file-arrow-down")}<span>Export</span></button>
          <button type="button" class="tree-row" @click=${() => this.openDialog("import")}>${icon("file-arrow-up")}<span>Import</span></button>
        </div>
      </aside>`;
  }

  // --- tabs -----------------------------------------------------------------------

  openTabMenu(e, index) {
    e.preventDefault();
    this.menuTab = index;
    this.querySelector("#tab-menu").show(e.currentTarget);
  }

  renderTabs() {
    const s = this.store;
    return html`
      <div class="tab-strip">
        <lit-material-tabs
          aria-label="Open requests"
          .selected=${s.currentTab}
          @change=${(e) => s.selectTab(e.target.selected)}
        >
          ${s.tabs.map(
            (t, i) => html`<lit-material-tab id=${`tab-${i}`} @contextmenu=${(e) => this.openTabMenu(e, i)}>${t.title}</lit-material-tab>`,
          )}
        </lit-material-tabs>
        <lit-material-icon-button aria-label="Request actions" id="tab-actions" @click=${(e) => ((this.menuTab = s.currentTab), this.querySelector("#tab-menu").show(e.currentTarget))}>
          ${icon("dots-three-vertical")}
        </lit-material-icon-button>
        <lit-material-icon-button aria-label="New tab" @click=${() => s.newTab()}>${icon("plus")}</lit-material-icon-button>
      </div>
      <lit-material-menu id="tab-menu">
        <lit-material-list-item interactive @click=${() => this.openDialog("rename-tab", { index: this.menuTab, name: s.tabs[this.menuTab]?.title ?? "" })}>Rename</lit-material-list-item>
        <lit-material-list-item interactive @click=${() => s.duplicateTab(this.menuTab)}>Duplicate</lit-material-list-item>
        <lit-material-list-item interactive ?disabled=${s.tabs.length < 2} @click=${() => s.closeTab(this.menuTab)}>Close tab</lit-material-list-item>
        <lit-material-list-item interactive @click=${() => this.deleteTabRequest(this.menuTab)}>Delete request</lit-material-list-item>
      </lit-material-menu>`;
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

  // --- main -------------------------------------------------------------------------

  renderMain() {
    const s = this.store;
    const { kind, collectionId } = s.view;
    if (kind === "cookies") {
      return html`<hitop-cookies .store=${s} .version=${s.version} .app=${this}></hitop-cookies>`;
    }
    if (kind === "environments") {
      return html`<hitop-environments .store=${s} .version=${s.version} .app=${this}></hitop-environments>`;
    }
    if (kind === "collections") {
      return html`<hitop-collections .store=${s} .version=${s.version} .app=${this}></hitop-collections>`;
    }
    if (kind === "collection" || kind === "variables") {
      const col = s.collection(collectionId);
      if (!col) return html`<hitop-collections .store=${s} .version=${s.version} .app=${this}></hitop-collections>`;
      return kind === "variables"
        ? html`<hitop-variables .store=${s} .version=${s.version} .collection=${col}></hitop-variables>`
        : html`<hitop-collection .store=${s} .version=${s.version} .app=${this} .collection=${col}></hitop-collection>`;
    }
    return html`${this.renderTabs()}<hitop-request .store=${s} .version=${s.version} .app=${this} .tab=${s.tab}></hitop-request>`;
  }

  render() {
    const s = this.store;
    return html`
      <a class="skip-link" href="#main">Skip to content</a>
      <header class="topbar">
        <lit-material-icon-button class="nav-toggle" aria-label=${this.navOpen ? "Close collections" : "Open collections"} aria-expanded=${String(this.navOpen)} @click=${() => (this.navOpen = !this.navOpen)}>
          ${icon(this.navOpen ? "x" : "list")}
        </lit-material-icon-button>
        <span class="topbar-title">HITOP</span>
        <span class="topbar-spacer"></span>
        <a class="topbar-link" href="/">${icon("house")}<span>hitop.rws8.tech</span></a>
        <lit-material-color-scheme-toggle storage-key="hitop-theme">
          ${icon("sun", { slot: "light-icon" })}${icon("moon", { slot: "dark-icon" })}
        </lit-material-color-scheme-toggle>
      </header>
      <div class="layout">
        ${this.renderSidebar()}
        <div class="scrim" ?hidden=${!this.navOpen} @click=${() => (this.navOpen = false)}></div>
        <main id="main" class="main">${this.renderMain()}</main>
      </div>
      ${this.renderDialog()}
      <lit-material-snackbar id="toast" closable></lit-material-snackbar>`;
  }
}

customElements.define("hitop-app", HitopApp);
