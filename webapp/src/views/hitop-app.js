// The app shell: sidebar with collections, the request tabs, the main view,
// and every dialog. Light DOM, so the global theme and app.css style it.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { repeat } from "lit/directives/repeat.js";
import { unsafeSVG } from "lit/directives/unsafe-svg.js";
import { mark } from "../../../site/src/brand/mark.js";
import { icon } from "../icons.js";
import { methodLabel, Store } from "../store.js";
import { IS_TAURI, platform } from "../platform/index.js";
import { issueText } from "../pm.js";
import "./request-view.js";
import "./collection-views.js";
import { WIDE_DIALOGS, codeDialog, compareDialog, refreshCode } from "./tools-dialogs.js";
import { OPENAPI_DIALOGS, openapiDialog, submitOpenapi } from "./openapi-dialogs.js";
import { shortcutFor } from "../shortcuts.js";
import { shortcutList } from "./shortcut-list.js";
import "./settings-view.js";

// Collections folded in the sidebar, by id (its own key: nothing else reads it).
const COLLAPSED_KEY = "hitop_collapsed_collections";
function loadCollapsed() {
  try {
    const v = JSON.parse(localStorage.getItem(COLLAPSED_KEY) || "[]");
    return Array.isArray(v) ? v.map(String) : [];
  } catch {
    return [];
  }
}
function saveCollapsed(ids) {
  try {
    localStorage.setItem(COLLAPSED_KEY, JSON.stringify(ids));
  } catch {
    // storage full or blocked: folding lasts for this page
  }
}

// Import errors in words: a parser's message ("Unexpected token } in JSON at
// position 41") tells most people nothing.
function readableError(err, what) {
  if (err instanceof SyntaxError) return `That isn't valid JSON. Paste the whole of ${what}, from the first { or [ to the last.`;
  return `That isn't ${what} HITOP can read: ${err.message}`;
}

export class HitopApp extends LitElement {
  static properties = {
    navOpen: { state: true },
    dialog: { state: true },
    filter: { state: true },
    collapsed: { state: true },
    menuTab: { state: true },
    exportFormat: { state: true },
  };

  constructor() {
    super();
    this.store = new Store();
    this.navOpen = false;
    this.dialog = null; // { kind, ...data }
    this.exportFormat = "hitop";
    this.filter = "";
    this.collapsed = loadCollapsed();
    this.store.addEventListener("change", () => this.requestUpdate());
    this.store.addEventListener("notify", (e) => this.toast(e.detail));
  }

  createRenderRoot() {
    return this;
  }

  toast(message) {
    this.querySelector("#toast")?.show(message);
  }

  firstUpdated() {
    platform().onStart?.(this);
    document.addEventListener("keydown", (e) => this.onShortcut(e), true);
  }

  updated() {
    // The selected request tab scrolls into view (the strip scrolls sideways).
    const s = this.store;
    if (s.view.kind !== "request" || (this.shownTab === s.currentTab && this.shownTabs === s.tabs.length)) return;
    this.shownTab = s.currentTab;
    this.shownTabs = s.tabs.length;
    // After the tabs element has laid out its new tab.
    requestAnimationFrame(() => this.querySelector(`#tab-${s.currentTab}`)?.scrollIntoView({ block: "nearest", inline: "nearest" }));
  }

  // Keyboard shortcuts (shortcuts.js); none while a dialog is open.
  onShortcut(e) {
    const action = shortcutFor(e);
    if (!action) return;
    if (this.dialog || document.querySelector("lit-material-confirm-dialog[open]")) return;
    const s = this.store;
    const onRequest = s.view.kind === "request";
    if (["send", "save"].includes(action) && !onRequest) {
      if (action === "save") e.preventDefault(); // never the browser's "Save page"
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    switch (action) {
      case "send":
        if (!s.tab?.loading) s.send();
        break;
      case "save":
        this.saveCurrent();
        break;
      case "focusUrl":
        if (!onRequest) s.selectTab(s.currentTab);
        this.updateComplete.then(() => this.querySelector("hitop-request .request-line .url")?.focus());
        break;
      case "newTab":
        s.newTab();
        break;
      case "closeTab":
        if (onRequest) this.closeTab(s.currentTab);
        break;
      case "nextTab":
      case "prevTab":
        s.selectTab((s.currentTab + (action === "nextTab" ? 1 : s.tabs.length - 1)) % s.tabs.length);
        break;
      case "help":
        this.openDialog("shortcuts");
        break;
    }
  }

  go(kind, collectionId = null) {
    this.navOpen = false;
    this.store.show(kind, collectionId);
  }

  // --- dialogs ----------------------------------------------------------------

  async openDialog(kind, data = {}) {
    this.navOpen = false;
    this.dialog = { kind, ...data };
    if (kind === "code") refreshCode(this);
    await this.updateComplete;
    const dlg = this.querySelector("#dialog");
    dlg?.show();
    // The dialog focuses itself as it opens; move focus to the first field after.
    await dlg?.updateComplete;
    requestAnimationFrame(() => dlg?.querySelector("[autofocus]")?.focus());
  }

  // The form is novalidate: empty required fields get their message on the
  // field, not in the browser's bubble.
  validateDialog() {
    let first = null;
    for (const el of this.querySelectorAll("#dialog-form [required]")) {
      const empty = !String(el.value ?? "").trim();
      el.error = empty;
      const label = el.getAttribute("label") ?? "";
      el.errorText = empty ? `Enter the ${label.charAt(0).toLowerCase()}${label.slice(1)}` : "";
      if (empty) first ??= el;
    }
    first?.focus();
    return !first;
  }

  clearFieldError(el) {
    if (!el?.error) return;
    el.error = false;
    el.errorText = "";
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
    if (!this.validateDialog()) return;
    d.submitted = true;
    if (OPENAPI_DIALOGS.includes(d.kind)) {
      submitOpenapi(this, d);
      return;
    }
    if (d.kind === "add-collection") s.addCollection(this.field("name"));
    if (d.kind === "add-environment") s.addEnvironment(this.field("name"));
    if (d.kind === "rename-environment") s.renameEnvironment(d.id, this.field("name"));
    if (d.kind === "import-environments") {
      try {
        s.importEnvironments(this.field("json"));
      } catch (err) {
        this.dialog = { ...d, submitted: false, error: readableError(err, "an environment file") };
        return;
      }
    }
    if (d.kind === "rename-collection") s.renameCollection(d.id, this.field("name"));
    if (d.kind === "rename-tab") s.renameTab(d.index, this.field("name"));
    if (d.kind === "save-example") s.saveExample(this.field("name"));
    if (d.kind === "save") {
      const collectionId = Number.isNaN(Number(this.field("collection"))) ? this.field("collection") : Number(this.field("collection"));
      const target = s.collections.find((c) => String(c.id) === String(collectionId));
      s.saveRequest(this.field("name"), target?.id);
    }
    if (d.kind === "import") {
      // A Postman import may come back with things to check: show them. An
      // OpenAPI spec is added as a new collection, with its report.
      const text = this.field("json");
      const replace = async () => {
        JSON.parse(text.replace(/^﻿/, "")); // bad JSON says so before anything is asked
        // A collection import replaces every collection: ask first when that deletes something.
        if (!(await this.confirmReplace())) return { cancelled: true };
        return { report: await s.importJson(text) };
      };
      s.isOpenapi(text)
        .then(async (openapi) => (openapi ? { openapi: await s.importOpenapi({ text }) } : replace()))
        .then(
        ({ openapi, report, cancelled }) => {
          if (cancelled) this.dialog = { ...d, submitted: false };
          else if (openapi) this.dialog = { kind: "openapi-report", report: openapi };
          else if (report?.issues.length) this.dialog = { kind: "import-report", report };
          else this.closeDialog();
        },
        (err) => (this.dialog = { ...d, submitted: false, error: readableError(err, "a collection file") }),
      );
      return;
    }
    this.closeDialog();
  }

  async confirmReplace() {
    const stake = this.store.collectionsAtStake();
    if (!stake.length) return true;
    const list = stake.map((c) => `${c.name} (${c.requests} request${c.requests === 1 ? "" : "s"})`).join(", ");
    return confirm({
      headline: "Replace your collections?",
      message: `Importing deletes ${stake.length === 1 ? "your collection" : `all ${stake.length} of your collections`}: ${list}. Export first if you want a copy.`,
      confirmLabel: "Replace",
      destructive: true,
    });
  }

  // The file picked (or dropped) in the Import dialog fills its text area.
  async importFile(file) {
    if (!file) return;
    const field = this.querySelector("#dialog #json");
    if (!field) return;
    try {
      field.value = await file.text();
      this.clearFieldError(field);
      this.dialog = { ...this.dialog, file: file.name, error: null };
    } catch (e) {
      this.dialog = { ...this.dialog, error: `Couldn't read ${file.name}: ${e?.message ?? e}` };
    }
  }

  // Closing a tab whose edits aren't saved asks first.
  async closeTab(index) {
    const s = this.store;
    const tab = s.tabs[index];
    if (!tab || s.tabs.length < 2) return;
    if (s.isDirty(tab) || s.hasDraftContent(tab)) {
      const ok = await confirm({
        headline: `Discard changes to ${tab.title}?`,
        message: tab.loadedRequestId ? "The tab has edits that aren't saved; the saved request stays as it was." : "This request isn't saved in a collection.",
        confirmLabel: "Discard",
        destructive: true,
      });
      if (!ok) return;
    }
    s.closeTab(s.tabs.indexOf(tab));
  }

  saveAndClose(index) {
    const s = this.store;
    const tab = s.tabs[index];
    if (!tab || !s.loadedRequest(tab)) return;
    s.selectTab(index);
    s.saveRequest(s.loadedRequest(tab).name, tab.loadedCollectionId);
    s.closeTab(index);
  }

  // Save: a saved request is overwritten in place, a draft asks where.
  saveCurrent() {
    const s = this.store;
    const request = s.loadedRequest();
    if (request) s.saveRequest(request.name, s.tab.loadedCollectionId);
    else if (s.tab?.url) this.openDialog("save", s.saveTarget());
  }

  async deleteCollection(collection) {
    const folder = this.store.folderOf(collection.id);
    if (folder) {
      const ok = await confirm({
        headline: `Close ${collection.name}?`,
        message: `It leaves HITOP. The folder ${folder.label} and its files stay where they are; open it again any time.`,
        confirmLabel: "Close",
      });
      if (ok) this.store.deleteCollection(collection.id);
      return;
    }
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
          <p class="dialog-note">Choose a file, drop one here or paste HITOP, Postman v2.1 or Bruno JSON. Importing replaces every
            collection you have now (you're asked first). An OpenAPI 3 spec (YAML or JSON) is added as a new collection instead.</p>
          <label class="file-pick">
            <input type="file" class="visually-hidden" accept=".json,.yaml,.yml,application/json,application/yaml,text/yaml"
              @change=${(e) => this.importFile(e.target.files?.[0])} />
            <span class="file-button">${icon("file-arrow-up")} Choose a file</span>
            <span class="file-name">${d.file ?? "No file chosen"}</span>
          </label>
          <div class="drop-zone" @dragover=${(e) => e.preventDefault()} @drop=${(e) => (e.preventDefault(), this.importFile(e.dataTransfer?.files?.[0]))}>
            <lit-material-textarea id="json" variant="outlined" label="Collection JSON or OpenAPI spec" rows="10" required autofocus
              ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-textarea>
          </div>
          <lit-material-button variant="text" class="dialog-link" @click=${() => this.openDialog("openapi-import")}>${icon("globe")} Load an OpenAPI spec from a URL</lit-material-button>`;
        footer = actions("Replace and import", true);
        break;
      case "import-report": {
        const { name, issues } = d.report;
        const scripts = issues.filter((i) => i.line).length;
        headline = "Check after import";
        body = html`
          <p class="dialog-note">
            ${name} is imported. ${issues.length === 1 ? "One thing" : `${issues.length} things`} didn't come over as they were${scripts
              ? html`; ${scripts === 1 ? "one script line uses" : `${scripts} script lines use`} Postman calls HITOP doesn't run yet`
              : ""}.
          </p>
          <ul class="import-report">
            ${issues.map((i) => html`<li><span>${issueText(i)}</span>${i.text ? html`<code>${i.text}</code>` : nothing}</li>`)}
          </ul>`;
        footer = actions("Done");
        break;
      }
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
      case "save-example":
        headline = "Save as example";
        body = html`
          <p class="dialog-note">The response is saved with ${d.request}, as an example to compare later (and in its folder file).</p>
          <lit-material-text-field id="name" variant="outlined" label="Example name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Save");
        break;
      case "openapi-import":
      case "openapi-report":
      case "openapi-sync":
        ({ headline, body, footer } = openapiDialog(this, d));
        break;
      case "code":
      case "compare":
        ({ headline, body, footer } = (d.kind === "code" ? codeDialog : compareDialog)(this, d));
        break;
      case "shortcuts":
        headline = "Keyboard shortcuts";
        body = html`${shortcutList()}<p class="dialog-note">They're on the Settings page too.</p>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="filled" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
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
                      <time datetime=${item.timestamp}>${new Date(item.timestamp).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}</time>
                    </button>
                  </li>`,
              )}
            </ol>`
          : html`<lit-material-empty-state size="compact" headline="No requests yet">Requests you send appear here, newest first. HITOP keeps the last 50.</lit-material-empty-state>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            ${s.history.length ? html`<lit-material-button variant="text" @click=${() => (this.closeDialog(), this.clearHistory())}>Clear history</lit-material-button>` : nothing}
            ${s.history.some((h) => h.response) ? html`<lit-material-button variant="text" @click=${() => this.openDialog("compare")}>Compare responses</lit-material-button>` : nothing}
            <lit-material-button variant="filled" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
    }
    return html`
      <lit-material-dialog id="dialog" class=${WIDE_DIALOGS.includes(d.kind) ? "wide" : ""} @close=${() => (this.dialog = null)}>
        <span slot="headline">${headline}</span>
        <form id="dialog-form" class="dialog-form" novalidate @submit=${(e) => this.submitDialog(e)} @input=${(e) => this.clearFieldError(e.target)} @keydown=${(e) => this.dialogKey(e)}>${body}</form>
        ${footer}
      </lit-material-dialog>`;
  }

  // The browser downloads the file; the desktop asks where to save it.
  async download(name, text, type = "application/json") {
    try {
      if (await platform().saveFile(name, text, type)) this.toast(IS_TAURI ? `Saved ${name}` : `Downloaded ${name}`);
    } catch (e) {
      this.toast(`Couldn't save ${name}: ${e?.message ?? e}`);
    }
  }

  // --- sidebar ------------------------------------------------------------------

  // The environment picker: in the top bar, and in the drawer on phones.
  envSelect() {
    const s = this.store;
    return html`<lit-material-select class="env-select" variant="outlined" label="Environment" .value=${s.activeEnvironmentId ?? "none"}
      @change=${(e) => s.setActiveEnvironment(e.target.value === "none" ? null : e.target.value)}>
      <lit-material-select-option value="none">No environment</lit-material-select-option>
      ${s.environments.map((env) => html`<lit-material-select-option value=${env.id}>${s.environmentLabel(env)}</lit-material-select-option>`)}
    </lit-material-select>`;
  }

  // Environment picker, a link to its page and the theme: the top bar's
  // right end, or the top of the drawer on phones (one shows at a time).
  tools(where) {
    return html`<div class=${`app-tools ${where}`}>
      ${this.envSelect()}
      <lit-material-icon-button aria-label="Manage environments" title="Manage environments" @click=${() => this.go("environments")}>${icon("sliders-horizontal")}</lit-material-icon-button>
      <lit-material-icon-button aria-label="Keyboard shortcuts" title="Keyboard shortcuts (?)" @click=${() => this.openDialog("shortcuts")}>${icon("keyboard")}</lit-material-icon-button>
      <lit-material-color-scheme-toggle storage-key="hitop-theme" cycle>
        ${icon("sun", { slot: "light-icon" })}${icon("moon", { slot: "dark-icon" })}
      </lit-material-color-scheme-toggle>
    </div>`;
  }

  toggleCollection(id) {
    const key = String(id);
    this.collapsed = this.collapsed.includes(key) ? this.collapsed.filter((k) => k !== key) : [...this.collapsed, key];
    saveCollapsed(this.collapsed);
  }

  renderSidebar() {
    const s = this.store;
    const { kind, collectionId } = s.view;
    const activeRequest = kind === "request" ? s.tab?.loadedRequestId : null;
    const filter = this.filter.trim().toLowerCase();
    const matches = (r) => !filter || `${methodLabel(r)} ${r.name} ${r.url ?? ""}`.toLowerCase().includes(filter);
    return html`
      <aside class=${`sidebar ${this.navOpen ? "open" : ""}`} aria-label="Collections">
        ${this.tools("in-drawer")}
        <lit-material-button variant="filled" class="new-request" @click=${() => (this.navOpen = false, s.newRequestInFirstCollection())}>
          ${icon("plus")} New request
        </lit-material-button>
        <input type="search" class="tree-filter" placeholder="Filter requests" aria-label="Filter requests" .value=${this.filter}
          @input=${(e) => (this.filter = e.target.value)} />
        <nav class="tree" aria-label="Collections">
          <button type="button" class="tree-row heading" aria-current=${kind === "collections" ? "page" : "false"} @click=${() => this.go("collections")}>
            ${icon("folders")}<span>Collections</span>
          </button>
          <ul>
            ${s.collections.map((c) => {
              const requests = (c.requests || []).filter(matches);
              if (filter && !requests.length) return nothing;
              const open = Boolean(filter) || !this.collapsed.includes(String(c.id));
              return html`
                <li>
                  <div class="collection-line">
                    <button type="button" class="tree-toggle" aria-expanded=${String(open)} aria-label="Requests" title=${`Show or hide the requests in ${c.name}`}
                      ?disabled=${Boolean(filter)} @click=${() => this.toggleCollection(c.id)}>${icon(open ? "caret-down" : "caret-right")}</button>
                    <button type="button" class="tree-row collection" aria-current=${kind === "collection" && collectionId === c.id ? "page" : "false"} @click=${() => this.go("collection", c.id)}>
                      ${icon("folder-simple")}<span>${c.name}</span>
                    </button>
                  </div>
                  ${open
                    ? html`<ul>
                        ${filter
                          ? nothing
                          : html`<li>
                              <button type="button" class="tree-row leaf" aria-current=${kind === "variables" && collectionId === c.id ? "page" : "false"} @click=${() => this.go("variables", c.id)}>
                                ${icon("sliders-horizontal")}<span>Variables</span>
                              </button>
                            </li>`}
                        ${requests.length
                          ? html`<li>
                              <lit-material-sortable role="list" label=${`Requests in ${c.name}`} @sort=${(e) => s.moveRequest(c.id, e.detail.from, e.detail.to)}>
                                ${repeat(
                                  requests,
                                  (r) => r.id,
                                  (r) => html`<div role="listitem" class="request-item" data-sort-id=${r.id} data-sort-label=${r.name}>
                                    <button type="button" class="tree-row leaf" aria-current=${activeRequest === r.id ? "page" : "false"} @click=${() => ((this.navOpen = false), s.openRequest(r, c.id))}>
                                      <span class=${`method-tag m-${methodLabel(r)}`}>${methodLabel(r)}</span><span>${r.name}${s.isDirtyRequest(r.id, c.id) ? html`<span class="dirty-dot" aria-hidden="true">●</span><span class="visually-hidden">, unsaved changes</span>` : nothing}</span>
                                    </button>
                                    ${filter ? nothing : html`<button type="button" class="sort-handle" data-sort-handle aria-label=${`Reorder ${r.name}`}>${icon("dots-six-vertical")}</button>`}
                                  </div>`,
                                )}
                              </lit-material-sortable>
                            </li>`
                          : nothing}
                      </ul>`
                    : nothing}
                </li>`;
            })}
          </ul>
        </nav>
        <div class="sidebar-footer">
          <button type="button" class="tree-row" aria-current=${kind === "environments" ? "page" : "false"} @click=${() => this.go("environments")}>${icon("globe")}<span>Environments</span></button>
          <button type="button" class="tree-row" aria-current=${kind === "cookies" ? "page" : "false"} @click=${() => this.go("cookies")}>${icon("cookie")}<span>Cookies</span></button>
          <button type="button" class="tree-row" @click=${() => this.openDialog("history")}>${icon("clock-counter-clockwise")}<span>History</span></button>
          <button type="button" class="tree-row" @click=${() => this.openDialog("export")}>${icon("file-arrow-down")}<span>Export</span></button>
          <button type="button" class="tree-row" @click=${() => this.openDialog("import")}>${icon("file-arrow-up")}<span>Import</span></button>          ${(platform().pages ?? []).map(
            (page) => html`<button type="button" class="tree-row" aria-current=${kind === page.kind ? "page" : "false"} @click=${() => this.go(page.kind)}>${icon(page.icon)}<span>${page.label}</span></button>`,
          )}
          <button type="button" class="tree-row" aria-current=${kind === "settings" ? "page" : "false"} @click=${() => this.go("settings")}>${icon("gear-six")}<span>Settings</span></button>
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
            (t, i) => html`<lit-material-tab id=${`tab-${i}`} title=${t.title} @contextmenu=${(e) => this.openTabMenu(e, i)}
              @auxclick=${(e) => e.button === 1 && (e.preventDefault(), this.closeTab(i))}>${t.title}${s.isDirty(t)
                ? html`<span class="dirty-dot" aria-hidden="true">●</span><span class="visually-hidden">, unsaved changes</span>`
                : nothing}</lit-material-tab>`,
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
        <lit-material-list-item interactive ?disabled=${s.tabs.length < 2} @click=${() => this.closeTab(this.menuTab)}>Close tab</lit-material-list-item>
        ${s.tabs[this.menuTab ?? s.currentTab] && s.isDirty(s.tabs[this.menuTab ?? s.currentTab])
          ? html`<lit-material-list-item interactive ?disabled=${s.tabs.length < 2} @click=${() => this.saveAndClose(this.menuTab)}>Save and close</lit-material-list-item>`
          : nothing}
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
    const page = platform().pages?.find((p) => p.kind === kind);
    if (page) return page.render(s, this);
    if (kind === "settings") {
      return html`<hitop-app-settings .store=${s} .version=${s.version} .app=${this}></hitop-app-settings>`;
    }
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
        ${IS_TAURI
          ? html`<span class="brand">${unsafeSVG(mark())}<span>HITOP</span></span>`
          : html`<a class="brand" href="/" aria-label="HITOP website">${unsafeSVG(mark())}<span>HITOP</span></a>`}
        <span class="topbar-spacer"></span>
        ${this.tools("in-topbar")}
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
