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
import { quickOpenItems, rankItems } from "../quick-open.js";
import { requestTree } from "../collection-tree.js";
import { shortcutList } from "./shortcut-list.js";
import "./settings-view.js";
import "./history-view.js";

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

// A menu for a right click (or the context menu key). Where the menu opens
// while the right button is still down (Linux, macOS), the release would
// dismiss it at once (popover light dismiss): it opens after the release.
function showMenuFor(e, menu) {
  const anchor = e.currentTarget;
  if (!(e.buttons & 2)) return menu.show(anchor);
  addEventListener("pointerup", () => setTimeout(() => menu.show(anchor)), { once: true, capture: true });
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
    // Mod+P never prints from inside the app, even with a dialog open.
    if (action === "quickOpen") e.preventDefault();
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
      case "quickOpen":
        this.openDialog("quick-open", { query: "", active: 0 });
        break;
    }
  }

  // --- quick open (Mod+P) ---------------------------------------------------------

  quickResults(d = this.dialog) {
    const s = this.store;
    return rankItems(quickOpenItems({ collections: s.collections, tabs: s.tabs, methodLabel }), d?.query ?? "", 50);
  }

  quickPick(item) {
    if (!item) return;
    const s = this.store;
    this.closeDialog();
    if (item.kind === "tab") s.selectTab(item.tabIndex);
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
        placeholder="Find a request by name, collection or URL" aria-label="Find a request"
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
    if (d.kind === "quick-open") {
      const { results, active } = this.quickDialog(d);
      this.quickPick(results[active]);
      return;
    }
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
    if (d.kind === "rename-request") s.renameRequest(d.collectionId, d.request.id, this.field("name"));
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
      const replace = d.mode === "replace";
      const collections = async () => {
        JSON.parse(text.replace(/^﻿/, "")); // bad JSON says so before anything is asked
        // "Replace all" deletes every collection: ask first when that deletes something.
        if (replace && !(await this.confirmReplace())) return { cancelled: true };
        return { report: await s.importJson(text, { replace }) };
      };
      s.isOpenapi(text)
        .then(async (openapi) => (openapi ? { openapi: await s.importOpenapi({ text }) } : collections()))
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
      case "rename-request":
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
      case "import": {
        headline = "Import collections";
        const replace = d.mode === "replace";
        const mode = (value, label, help) => html`<label class="import-mode-option">
          <input type="radio" name="import-mode" value=${value} .checked=${(d.mode ?? "add") === value} @change=${() => (this.dialog = { ...this.dialog, mode: value })} />
          <span><strong>${label}</strong><span class="hint">${help}</span></span>
        </label>`;
        body = html`
          <p class="dialog-note">Choose a file, drop one here or paste HITOP, Postman v2.1 or Bruno JSON, or an OpenAPI 3 spec (YAML or JSON, always added as a new collection).</p>
          <fieldset class="import-mode">
            <legend>Imported collections</legend>
            ${mode("add", "Add to my collections", "A name you already have gets (2) added.")}
            ${mode("replace", "Replace all my collections", "Deletes the ones you have now (you're asked first).")}
          </fieldset>
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
        footer = replace ? actions("Replace and import", true) : actions("Import");
        break;
      }
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
      case "quick-open": {
        headline = "Quick open";
        ({ body } = this.quickDialog(d));
        footer = html`<div slot="actions" class="dialog-actions quick-actions">
          <span class="hint">↑ ↓ to choose, Enter to open, Esc to close</span>
          <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
        </div>`;
        break;
      }
      case "shortcuts":
        headline = "Keyboard shortcuts";
        body = html`${shortcutList()}<p class="dialog-note">They're on the Settings page too.</p>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="filled" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
    }
    return html`
      <lit-material-dialog id="dialog" class=${WIDE_DIALOGS.includes(d.kind) || d.kind === "quick-open" ? "wide" : ""} @close=${() => (this.dialog = null)}>
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

  // One level of a collection's requests, as a reorderable list (`sortable`):
  // `entries` are { request, index } with the index in the collection.
  requestList(c, entries, label, activeRequest, sortable = true) {
    const s = this.store;
    return html`<lit-material-sortable role="list" label=${label}
      @sort=${(e) => sortable && s.moveRequest(c.id, entries[e.detail.from].index, entries[e.detail.to].index)}>
      ${repeat(
        entries,
        (x) => x.request.id,
        ({ request: r }) => html`<div role="listitem" class="request-item" data-sort-id=${r.id} data-sort-label=${r.name}>
          <button type="button" class="tree-row leaf" aria-current=${activeRequest === r.id ? "page" : "false"}
            @click=${() => ((this.navOpen = false), s.openRequest(r, c.id))} @contextmenu=${(e) => this.openRequestMenu(e, r, c.id)}>
            <span class=${`method-tag m-${methodLabel(r)}`}>${methodLabel(r)}</span><span>${r.name}${s.isDirtyRequest(r.id, c.id) ? html`<span class="dirty-dot" aria-hidden="true">●</span><span class="visually-hidden">, unsaved changes</span>` : nothing}</span>
          </button>
          ${sortable ? html`<button type="button" class="sort-handle" data-sort-handle aria-label=${`Reorder ${r.name}`}>${icon("dots-six-vertical")}</button>` : nothing}
        </div>`,
      )}
    </lit-material-sortable>`;
  }

  // A collection's requests with their folders (Postman folders, OpenAPI
  // tags, folder collections' subfolders: the `file` path) as groups that
  // fold, in the collection's order (a folder where its first request is);
  // the requests between two folders are one reorderable list.
  requestTreeRows(c, node, activeRequest) {
    const runs = [];
    for (const item of node.items) {
      if (item.folder) runs.push(item);
      else if (Array.isArray(runs.at(-1))) runs.at(-1).push(item);
      else runs.push([item]);
    }
    const label = node.path ? `Requests in ${c.name}, ${node.path}` : `Requests in ${c.name}`;
    return html`${runs.map((run) => (Array.isArray(run) ? html`<li>${this.requestList(c, run, label, activeRequest)}</li>` : this.folderRow(c, run.folder, activeRequest)))}`;
  }

  folderRow(c, f, activeRequest) {
    const key = `${c.id}/${f.path}`;
    const open = !this.collapsed.includes(key);
    return html`<li class="tree-folder">
      <button type="button" class="tree-row folder" aria-expanded=${String(open)} title=${f.path} @click=${() => this.toggleCollection(key)}>
        ${icon(open ? "caret-down" : "caret-right")}${icon("folder-simple")}<span>${f.name}</span>
      </button>
      ${open ? html`<ul>${this.requestTreeRows(c, f, activeRequest)}</ul>` : nothing}
    </li>`;
  }

  // Right click (or the context menu key) on a request in the sidebar.
  openRequestMenu(e, request, collectionId) {
    e.preventDefault();
    this.menuRequest = { request, collectionId };
    showMenuFor(e, this.querySelector("#request-menu"));
  }

  renderRequestMenu() {
    const s = this.store;
    const m = () => this.menuRequest;
    return html`<lit-material-menu id="request-menu" aria-label="Request">
      <lit-material-list-item interactive @click=${() => ((this.navOpen = false), s.openRequest(m().request, m().collectionId))}>Open</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("rename-request", { ...m(), name: m().request.name })}>Rename</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => s.duplicateRequest(m().collectionId, m().request.id)}>Duplicate</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.deleteRequest(m().request, m().collectionId)}>Delete</lit-material-list-item>
    </lit-material-menu>`;
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
                        ${filter
                          ? requests.length
                            ? html`<li>${this.requestList(c, requests.map((request) => ({ request, index: -1 })), `Requests in ${c.name}`, activeRequest, false)}</li>`
                            : nothing
                          : this.requestTreeRows(c, requestTree(c.requests || []), activeRequest)}
                      </ul>`
                    : nothing}
                </li>`;
            })}
          </ul>
        </nav>
        <div class="sidebar-footer">
          <button type="button" class="tree-row" aria-current=${kind === "environments" ? "page" : "false"} @click=${() => this.go("environments")}>${icon("globe")}<span>Environments</span></button>
          <button type="button" class="tree-row" aria-current=${kind === "cookies" ? "page" : "false"} @click=${() => this.go("cookies")}>${icon("cookie")}<span>Cookies</span></button>
          <button type="button" class="tree-row" aria-current=${kind === "history" ? "page" : "false"} @click=${() => this.go("history")}>${icon("clock-counter-clockwise")}<span>History</span></button>
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
    showMenuFor(e, this.querySelector("#tab-menu"));
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
                : nothing}${s.tabs.length > 1
                ? html`<span class="tab-close" aria-hidden="true" title="Close tab (Alt+Shift+W)"
                    @pointerdown=${(e) => e.stopPropagation()} @mousedown=${(e) => e.stopPropagation()}
                    @click=${(e) => (e.stopPropagation(), e.preventDefault(), this.closeTab(i))}>${icon("x")}</span>`
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
    if (kind === "history") {
      return html`<hitop-history .store=${s} .version=${s.version} .app=${this}></hitop-history>`;
    }
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
      ${this.renderDialog()} ${this.renderRequestMenu()}
      <lit-material-snackbar id="toast" closable></lit-material-snackbar>`;
  }
}

customElements.define("hitop-app", HitopApp);
