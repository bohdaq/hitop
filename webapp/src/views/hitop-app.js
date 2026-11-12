// The app shell: sidebar with collections, the request tabs, the main view,
// and every dialog. Light DOM, so the global theme and app.css style it.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { repeat } from "lit/directives/repeat.js";
import { unsafeSVG } from "lit/directives/unsafe-svg.js";
import { mark } from "../../../site/src/brand/mark.js";
import { icon } from "../icons.js";
import { methodLabel, Store } from "../store.js";
import { isBlankRequest, isMyRequests, UNTITLED } from "../new-request.js";
import { IS_TAURI, platform } from "../platform/index.js";
import { issueText } from "../pm.js";
import "@lit-material/navigation";
import "@lit-material/command-palette";
import { PALETTE_HINTS, PREFIXES, actions as paletteActions, storeItems } from "../commands.js";
import "./request-view.js";
import "./collection-views.js";
import { WIDE_DIALOGS, codeDialog, compareDialog, refreshCode } from "./tools-dialogs.js";
import { OPENAPI_DIALOGS, openapiDialog, submitOpenapi } from "./openapi-dialogs.js";
import { ACTION_KEYS, SAVE_KEYS, SEND_KEYS, shortcutFor } from "../shortcuts.js";
import { quickOpenItems, rankItems } from "../quick-open.js";
import { protocolLabel } from "../request-facts.js";
import { folderOf, requestTree, slugName, withFolders } from "../collection-tree.js";
import { shortcutList } from "./shortcut-list.js";
import "./settings-view.js";
import "./history-view.js";
import "./github-view.js";
import { storageOf } from "../git.js";
import { signInDialog } from "./sign-in-dialog.js";
import { syncClient } from "./sync-view.js";
import "./limit-note.js";
import { orgLabel } from "../orgs.js";
import { agentPanel, runAi } from "./ai-view.js";
import "./org-view.js";
import { limitInfo, parseOrgLink, sectionsFor, usageWarnings } from "../org.js";
import { asksForEvents } from "../features.js";
import { isCurl, parseCurl } from "../curl.js";
import { gettingStartedSteps, hideGettingStarted, SAMPLE_NAME, savedOnce, showGettingStarted } from "../getting-started.js";

/** Email addresses typed in the invite dialog: comma, space or line separated. */
function inviteEmails(text) {
  return [...new Set(String(text).split(/[\s,;]+/).map((e) => e.trim()).filter((e) => /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(e)))];
}
import "./mock-view.js";
import "./monitor-view.js";
import { historyDialog, loadRequestHistory } from "./history-dialog.js";
import { aiSettings } from "../ai.js";

// The HITOP Cloud portal (plan, organizations, billing): web only, never in the desktop build (App
// Store: no purchase links).
const PORTAL_URL = "https://cloud.hitop.rws8.tech/";
// The org switcher's last option: opens the New organization dialog.
const NEW_ORG = "__new_org";
// The environment picker's "Add environment…" (FV1, shown when there is none).
const ADD_ENVIRONMENT = "__add_environment";

// Collections folded in the sidebar, by id (its own key: nothing else reads it).
const COLLAPSED_KEY = "hitop_collapsed_collections";
// The save dialog's collection picker: this value makes a new collection from the name typed next to it.
const NEW_COLLECTION = "__new__";
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
// The method tag of an open tab: WS, GRPC, GQL (a GraphQL body) or the HTTP method.
const tagOf = (t) => (t.kind === "websocket" ? "WS" : t.kind === "grpc" ? "GRPC" : t.bodyMode === "graphql" ? "GQL" : (t.method || "GET") === "GET" && asksForEvents(t.headers) ? "SSE" : t.method || "GET");

function showMenuFor(e, menu) {
  const anchor = e.currentTarget;
  if (!(e.buttons & 2)) return menu.show(anchor);
  addEventListener("pointerup", () => setTimeout(() => menu.show(anchor)), { once: true, capture: true });
}


// The palette preview renders inside the palette's shadow root, so it brings its own styles.
const PREVIEW_CSS = `
  .palette-preview { display: grid; gap: 10px; font-size: 12.5px; color: #c8c8cf; }
  .mono { font-family: var(--font-mono, "Geist Mono", ui-monospace, monospace); }
  .pp-line { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .pp-url { color: #ededf0; overflow-wrap: anywhere; }
  .pp-place { color: #8a8a94; font-size: 11.5px; }
  .pp-label { margin-top: 4px; font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #8a8a94; }
  .pp-headers { display: grid; grid-template-columns: max-content 1fr; gap: 2px 12px; margin: 0; }
  .pp-headers dt { color: #a1a1aa; }
  .pp-headers dd { margin: 0; overflow-wrap: anywhere; }
  .pp-body { margin: 0; padding: 8px 10px; border: 1px solid #26262c; border-radius: 8px; background: #0f0f12; white-space: pre-wrap; overflow-wrap: anywhere; line-height: 19px; }
`;

export class HitopApp extends LitElement {
  static properties = {
    navOpen: { state: true },
    dialog: { state: true },
    filter: { state: true },
    collapsed: { state: true },
    menuTab: { state: true },
    menuCollection: { state: true },
    menuFolder: { state: true },
    emptyFolders: { state: true },
    exportFormat: { state: true },
    agent: { state: true },
  };

  constructor() {
    super();
    this.store = new Store();
    this.navOpen = false;
    this.emptyFolders = {}; // collection id → folder paths made in the sidebar that hold no request yet
    this.menuCollection = null;
    this.menuFolder = null;
    this.dialog = null; // { kind, ...data }
    this.agent = null; // the agent side sheet (ai-view.js): { action, prompt, text, patch, ... }
    this.exportFormat = "hitop";
    this.filter = "";
    // Desktop: `git status` per folder collection ({branch, ahead, behind, files}; gitStatus()).
    this.gitStatus = new Map();
    this.gitSeen = "";
    this.collapsed = loadCollapsed();
    this.store.addEventListener("change", () => this.requestUpdate());
    this.store.addEventListener("notify", (e) => this.toast(e.detail));
  }

  createRenderRoot() {
    return this;
  }

  // `action`: {label, run} for a button on the toast (UX2: "Imported from curl · Undo").
  async toast(message, action = null) {
    this.toastAction = action;
    this.requestUpdate();
    await this.updateComplete;
    this.querySelector("#toast")?.show(message);
  }

  firstUpdated() {
    // HITOP Next: dense controls where there is a precise pointer; touch keeps the 40-48 px targets.
    if (window.matchMedia?.("(pointer: fine)").matches) document.documentElement.setAttribute("density", "compact");
    platform().onStart?.(this);
    document.addEventListener("keydown", (e) => this.onShortcut(e), true);
    this.resumeSignIn();
    // A link into the Organization area (the portal's pages send the browser here, 7b).
    const openLink = () => {
      const link = parseOrgLink(location.hash);
      if (!link) return;
      history.replaceState(null, "", location.pathname + location.search);
      this.orgLink = link;
      // UX2 (10): signed out, the link asks to sign in first and continues to its section.
      if (!this.sync().account()) {
        const label = sectionsFor(null).find(([k]) => k === link.section)?.[1] ?? "the organization";
        this.openSignIn(() => (this.closeDialog(), this.applyOrgLink()), `Sign in to open ${label}`);
        return;
      }
      this.applyOrgLink();
    };
    openLink();
    window.addEventListener("hashchange", openLink);
    // FV1: a first visit starts on the sample's "List posts" with the URL field focused.
    if (this.store.firstVisit && this.store.view.kind === "request" && !this.dialog) this.focusUrl();
    // The org comes once the orgs are loaded (after the account is read).
    this.store.addEventListener("change", () => this.orgLink?.org && this.sync().account() && this.applyOrgLink());
  }

  applyOrgLink() {
    const link = this.orgLink;
    if (!link) return;
    const s = this.store;
    if (link.org) {
      if (!s.orgs?.some((o) => o.id === link.org)) {
        // Not loaded yet (or not this account's): show the page once, pick the org when it comes.
        if (!link.opened) {
          link.opened = true;
          this.openOrg(link.section);
        }
        return;
      }
      if (s.currentOrg !== link.org) s.setCurrentOrg(link.org);
    }
    this.orgLink = null;
    this.openOrg(link.section);
  }

  updated() {
    this.scheduleGitStatus();
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
    if (e.key === "Escape") return this.onEscape(e);
    const action = shortcutFor(e);
    if (!action) return;
    // Mod+P never prints from inside the app, even with a dialog open; Mod+K and Mod+L never reach
    // the browser's address bar or search.
    if (["quickOpen", "palette", "focusUrl"].includes(action)) e.preventDefault();
    if (this.querySelector("lit-material-command-palette")?.open) return;
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
        this.focusUrl();
        break;
      case "palette":
        this.openCommand();
        break;
      case "newTab":
        this.newRequest();
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
    // Collections, environments and history only once something is typed.
    const q = String(d?.query ?? "").trim();
    const environments = q ? s.visibleEnvironments.map((e) => ({ ...e, label: s.environmentLabel(e) })) : [];
    const history = q ? s.history : [];
    return rankItems(quickOpenItems({ collections: s.visibleCollections, tabs: s.tabs, methodLabel, environments, history }), d?.query ?? "", 50);
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

  // --- sign in ----------------------------------------------------------------------

  /** The hitop sync client of this app (views and the e2e helpers sign in through it). */
  sync() {
    return syncClient(this.store);
  }

  /** The Sign in dialog (sign-in-dialog.js). `then` runs after signing in (a Share dialog reopens). */
  openSignIn(then = null, reason = null) {
    this.signInThen = then;
    // UX2: why it is asked ("Sign in to open Plan & billing"), the dialog's headline.
    this.signInReason = reason;
    this.openDialog("sign-in", { step: "loading", title: reason || "Sign in to HITOP" });
  }

  /**
   * Just signed in (the dialog, or a test through `sync()`): the library links and pulls, the dialog
   * closes, a toast says who, focus goes to the account button. `r` = finishBrowserSignIn's answer.
   */
  async signedIn(r = {}, fromDialog = false) {
    const s = this.store;
    const sync = this.sync();
    const a = sync.account();
    const then = fromDialog ? this.signInThen : null;
    this.signInThen = null;
    this.signInReason = null;
    // With somewhere to go back to (a Share dialog), the dialog turns into it; else it closes.
    if (then) then();
    else if (this.dialog?.kind === "sign-in") this.closeDialog();
    await s.library?.checkAccount().catch(() => {});
    // The organizations: main.js refreshOrgs loads them once per account, and can run mid sign-in
    // (account saved, session not yet), fail and not retry; so load them here, signed in for sure.
    await import("../orgs.js").then(({ loadOrgs }) => loadOrgs(s, sync)).catch(() => {});
    this.requestUpdate();
    if (!a) return;
    // How many collections now sync to HITOP Cloud: the library's own record (git.js storageOf),
    // only when the library is on and holds some.
    const n = s.library?.status.on ? s.collections.filter((c) => ["cloud", "github"].includes(storageOf(s, c).kind)).length : 0;
    let text = `Signed in as ${a.email}${n ? ` · ${n === 1 ? "1 collection now syncs" : `${n} collections now sync`}` : ""}`;
    // A new device of an account with shared collections: their keys open with the recovery
    // phrase, the password or another device (Account, Password & recovery), not with Google or GitHub.
    if (r.newDevice) {
      const shared = await sync.workspaces().catch(() => []);
      if (shared?.length) text += ". To open your shared collections here, unlock this device (Account, Password & recovery)";
    }
    // The Account page, if it is on screen under the dialog, shows the new account.
    for (const el of this.querySelectorAll("hitop-sync")) el.requestUpdate();
    this.toast(text);
    await this.updateComplete;
    if (!then) this.querySelector("header.topbar .account-button")?.focus();
  }

  // Back from "Continue in this tab": the provider sent this tab to the app with `#sso=<code>`
  // (`?sso=<code>` on a loopback origin such as http://localhost).
  resumeSignIn() {
    const m = /^#sso=(.+)$/.exec(location.hash);
    const q = new URLSearchParams(location.search);
    const code = m ? decodeURIComponent(m[1]) : q.get("sso");
    if (!code) return;
    q.delete("sso");
    const rest = q.toString();
    history.replaceState(null, "", location.pathname + (rest ? `?${rest}` : ""));
    this.openDialog("sign-in", { step: "finishing", title: "Sign in to HITOP", resume: code });
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
    // The Sign in dialog's buttons and links take Enter themselves; only its fields submit.
    if (this.dialog?.kind === "sign-in" && !e.composedPath().some((n) => n.localName === "lit-material-text-field")) return;
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
    if (d?.kind === "share") {
      // Enter in a field: only the GitHub tab's form has one thing to do.
      if (d.tab === "github") this.querySelector("hitop-git-panel")?.submit();
      return;
    }
    if (d?.kind === "ai" || d?.kind === "request-history") return;
    if (d?.kind === "sign-in") {
      // Enter in Work email / Server: Continue with SSO; in the code field: Sign in.
      this.querySelector("#dialog hitop-sign-in")?.submitStep();
      return;
    }
    if (d?.kind === "join") {
      [...this.querySelectorAll("#dialog hitop-sync lit-material-button")].find((b) => b.textContent.includes("Accept invite"))?.click();
      return;
    }
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
    if (d.kind === "new-org" || d.kind === "org-invite") {
      this.submitOrg(d);
      return;
    }
    if (d.kind === "add-collection") {
      // The new collection opens, so the next step (its "New request") is on screen.
      const col = s.addCollection(this.field("name"));
      if (col) this.go("collection", col.id);
    }
    if (d.kind === "new-folder") this.addFolder(d.collectionId, d.parent, this.field("name"));
    if (d.kind === "rename-folder") {
      const newPath = s.renameFolder(d.collectionId, d.path, this.field("name"));
      if (newPath && newPath !== d.path) this.folderRenamed(d.collectionId, d.path, newPath);
    }
    if (d.kind === "move-request") {
      const [to, folder = ""] = String(this.field("target")).split("|");
      const target = s.collections.find((c) => String(c.id) === to);
      if (target && s.moveRequestTo(d.collectionId, d.request.id, target.id, folder)) {
        this.expand(target.id, folder);
        this.toast(`Moved "${d.request.name}" to ${folder ? `${target.name} › ${folder}` : target.name}`);
      }
    }
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
      // "New collection…" in the picker: made here, the request goes straight into it.
      const picked = String(this.field("collection"));
      const target = picked === NEW_COLLECTION ? s.addCollection(this.field("new-collection")) : s.collections.find((c) => String(c.id) === picked);
      if (!target) {
        this.dialog = { ...d, submitted: false };
        return;
      }
      s.saveRequest(this.field("name"), target.id, target.id === d.collectionId ? d.folder || "" : "");
      this.expand(target.id, target.id === d.collectionId ? d.folder || "" : "");
    }
    if (d.kind === "import") {
      // A Postman import may come back with things to check: show them. An
      // OpenAPI spec is added as a new collection, with its report.
      const text = this.field("json");
      // UX2 (2): a curl command is one new request in the collection on screen.
      if (isCurl(text)) {
        let parsed;
        try {
          parsed = parseCurl(text);
        } catch (err) {
          this.dialog = { ...d, submitted: false, error: `That curl command can't be read: ${err.message}` };
          return;
        }
        this.closeDialog();
        this.newRequest();
        this.applyCurl(parsed, true);
        return;
      }
      const replace = d.mode === "replace";
      const collections = async () => {
        JSON.parse(text.replace(/^\uFEFF/, "")); // bad JSON says so before anything is asked
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
      const saved = s.loadedRequest(tab);
      const ok = await confirm({
        headline: `Discard changes to ${tab.title}?`,
        message: !saved
          ? "This request isn't saved in a collection."
          : tab.autoName && isBlankRequest(saved) && saved.name === UNTITLED
            ? `This new request was never saved, so it comes out of ${s.collection(tab.loadedCollectionId)?.name ?? "its collection"} again.`
            : "The tab has edits that aren't saved; the saved request stays as it was.",
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
    s.saveRequest(s.savedName(tab), tab.loadedCollectionId);
    s.closeTab(index);
  }

  // Save: a saved request is overwritten in place (a new one under its automatic name), a draft
  // asks where (even without a URL yet: a named, empty request in a collection is a fine start).
  saveCurrent() {
    const s = this.store;
    const request = s.loadedRequest();
    if (request) s.saveRequest(s.savedName(s.tab, request), s.tab.loadedCollectionId);
    else if (s.tab) this.openDialog("save", s.saveTarget());
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
      case "new-org":
        headline = "New organization";
        body = html`<p>A shared space for your team's collections, environments and runs. Your personal space stays private. You own it; invite people next.</p>
          <lit-material-text-field id="name" variant="outlined" label="Organization name" required autofocus ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-text-field>
          <fieldset class="org-start">
            <legend>Start on</legend>
            <label class="org-choice"><input type="radio" name="org-start" value="trial" checked /> <span><b>Team trial</b>, free for 14 days, no card: unlimited shared collections, 5 free guest viewers, mocks and monitors.</span></label>
            <label class="org-choice"><input type="radio" name="org-start" value="free" /> <span><b>Free</b>: 3 members, 3 shared collections, 100 MB.</span></label>
          </fieldset>`;
        footer = actions("Create and invite people");
        break;
      case "org-invite": {
        const usage = d.seats;
        const seatsMax = usage?.limits?.seats;
        const seatsUsed = usage?.usage?.seats ?? 0;
        const pending = inviteEmails(d.draft ?? "");
        const domain = (s.syncClient?.account?.()?.email ?? "").split("@")[1] ?? "";
        const outside = domain ? pending.filter((e) => !e.endsWith(`@${domain}`)) : [];
        headline = `Invite people to ${s.currentOrgInfo?.name ?? "the organization"}`;
        body = html`<p>They get an email with a link; once they sign in with that address they are members${d.added?.length ? html`. Invited so far: ${d.added.join(", ")}` : ""}.</p>
          <lit-material-textarea id="email" variant="outlined" rows="2" label="Email addresses (comma or space separated)" required autofocus .value=${d.draft ?? ""} ?error=${Boolean(d.error)} error-text=${d.error ?? ""}
            @input=${(e) => (this.dialog = { ...this.dialog, draft: e.target.value })}></lit-material-textarea>
          ${outside.length ? html`<p class="hint warn-text">${outside.join(", ")} ${outside.length === 1 ? "is" : "are"} outside ${domain}: a guest viewer (read only, free on Team) may fit better, from a collection's Share dialog.</p>` : nothing}
          <lit-material-select id="role" variant="outlined" label="Role" .value=${"member"}>
            <lit-material-select-option value="member">Member: edit and run</lit-material-select-option>
            <lit-material-select-option value="admin">Admin: also invite and manage the plan</lit-material-select-option>
          </lit-material-select>
          ${seatsMax != null
            ? html`<p class=${`hint tabular ${seatsUsed + pending.length > seatsMax ? "warn-text" : ""}`} role="status">Seats after these invites: ${seatsUsed + pending.length} of ${seatsMax}${seatsUsed + pending.length > seatsMax ? ". Add seats on Plan & billing first." : ""}</p>`
            : nothing}`;
        footer = actions(pending.length > 1 ? `Send ${pending.length} invites` : "Invite");
        break;
      }
      case "limit": {
        const l = d.limit;
        headline = l.headline;
        body = html`<p>${l.message}</p>
          <ul class="limit-list">
            <li class="ok-text">${icon("check-circle")} What you have keeps syncing and running</li>
            <li class="ok-text">${icon("check-circle")} Everything local, the CLI and CI keep working</li>
            <li class="warn-text">${icon("warning-circle")} New ones need a bigger plan or a free slot</li>
          </ul>
          <hitop-limit-note bare .limit=${l} .store=${s} .client=${s.library?.client ?? syncClient(s)}></hitop-limit-note>`;
        footer = html`<div slot="actions" class="dialog-actions">
          <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
          ${d.admin
            ? html`<lit-material-button variant="filled" @click=${() => (this.closeDialog(), this.openOrg("billing"))}>Plan &amp; billing</lit-material-button>`
            : html`<lit-material-button variant="filled" @click=${() => (this.closeDialog(), this.openOrg("members"))}>See who can upgrade</lit-material-button>`}
        </div>`;
        break;
      }
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
      case "save": {
        // A draft with no collection to go to (none yet) starts on "New collection…".
        const picked = d.picked ?? (s.visibleCollections.some((c) => c.id === d.collectionId) ? String(d.collectionId) : NEW_COLLECTION);
        headline = d.overwrite ? "Save request" : "Save to a collection";
        body = html`
          <lit-material-text-field id="name" variant="outlined" label="Request name" .value=${d.name} required autofocus></lit-material-text-field>
          <lit-material-select id="collection" variant="outlined" label="Collection" .value=${picked} required
            @change=${(e) => (this.dialog = { ...this.dialog, picked: e.target.value })}>
            ${s.visibleCollections.map((c) => html`<lit-material-select-option value=${String(c.id)}>${c.name}</lit-material-select-option>`)}
            <lit-material-select-option value=${NEW_COLLECTION}>${icon("plus")} New collection…</lit-material-select-option>
          </lit-material-select>
          ${picked === NEW_COLLECTION
            ? html`<lit-material-text-field id="new-collection" variant="outlined" label="Collection name" placeholder="My API" required></lit-material-text-field>`
            : nothing}
          ${d.folder && picked === String(d.collectionId) ? html`<p class="hint">${icon("folder-simple")} In the folder ${d.folder}</p>` : nothing}`;
        footer = actions("Save");
        break;
      }
      case "move-request": {
        const targets = s.moveTargets();
        const here = `${d.collectionId}|${folderOf(d.request)}`;
        headline = `Move ${d.request.name}`;
        body = html`
          <lit-material-select id="target" variant="outlined" label="To" .value=${here} required autofocus>
            ${targets.map((t) => html`<lit-material-select-option value=${`${t.collectionId}|${t.folder}`}>${t.label}</lit-material-select-option>`)}
          </lit-material-select>
          <p class="hint">Collections and their folders. You can also drag a request onto a collection or folder in the sidebar.</p>`;
        footer = actions("Move");
        break;
      }
      case "new-folder":
        headline = d.parent ? `New folder in ${d.parent}` : "New folder";
        body = html`
          <lit-material-text-field id="name" variant="outlined" label="Folder name" placeholder="users" required autofocus></lit-material-text-field>
          <p class="hint">A folder is the path of its requests' files (<code>requests/${d.parent ? `${d.parent}/` : ""}&lt;folder&gt;/&lt;request&gt;.yaml</code>); it stays once a request is saved in it.</p>`;
        footer = actions("Create");
        break;
      case "rename-folder":
        headline = "Rename folder";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Folder name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "import": {
        headline = "Import collections";
        const replace = d.mode === "replace";
        const mode = (value, label, help) => html`<label class="import-mode-option">
          <input type="radio" name="import-mode" value=${value} .checked=${(d.mode ?? "add") === value} @change=${() => (this.dialog = { ...this.dialog, mode: value })} />
          <span><strong>${label}</strong><span class="hint">${help}</span></span>
        </label>`;
        body = html`
          <p class="dialog-note">Choose a file, drop one here or paste HITOP, Postman v2.1 or Bruno JSON, an OpenAPI 3 spec (YAML or JSON, always added as a new collection) or a curl command (one new request).</p>
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
      case "request-history":
        ({ headline, body, footer } = historyDialog(this, d));
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
      case "share": {
        const c = s.collections.find((x) => x.id === d.collectionId);
        headline = `Share ${c?.name ?? "the collection"}`;
        body = c ? this.shareBody(c, d.tab) : nothing;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
      }
      case "join":
        headline = "Join a shared collection";
        body = html`<p>Paste the invite code someone sent you.</p><hitop-sync .store=${s} .version=${s.version} .app=${this} .join=${true}></hitop-sync>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
      case "sign-in":
        ({ headline, body, footer } = signInDialog(this, d));
        break;
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
      <lit-material-dialog id="dialog" class=${WIDE_DIALOGS.includes(d.kind) || d.kind === "quick-open" || d.kind === "ai" ? "wide" : d.kind === "sign-in" ? "sign-in-dialog" : ""} @close=${() => (this.dialog = null)}>
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

  // The environment picker, next to Send (request-view.js): the current org's environments.
  envSelect() {
    const s = this.store;
    const env = s.activeEnvironment;
    const envs = s.visibleEnvironments;
    // A pill with a coloured dot (HITOP Next): amber with an environment, grey without. FV1: with
    // none at all it says so and offers to add one (the Environments page).
    return html`<lit-material-select class=${`env-select pill-select ${env ? "has-env" : ""}`} variant="outlined" label="Environment" .value=${env?.id ?? "none"}
      @change=${(e) => {
        if (e.target.value !== ADD_ENVIRONMENT) return s.setActiveEnvironment(e.target.value === "none" ? null : e.target.value);
        e.target.value = "none";
        this.go("environments");
      }}>
      <lit-material-select-option value="none">${envs.length ? "No environment" : "No environments"}</lit-material-select-option>
      ${envs.map((x) => html`<lit-material-select-option value=${x.id}>${s.environmentLabel(x)}</lit-material-select-option>`)}
      ${envs.length ? nothing : html`<lit-material-select-option value=${ADD_ENVIRONMENT}>Add environment…</lit-material-select-option>`}
    </lit-material-select>`;
  }

  // The organization switcher (signed in to HITOP Cloud): the top bar, and the drawer on phones.
  orgSelect() {
    const s = this.store;
    if (!s.orgs.length) return nothing;
    return html`<lit-material-select class="org-select pill-select" variant="outlined" label="Organization" .value=${s.currentOrg ?? ""}
      @change=${(e) => {
        if (e.target.value !== NEW_ORG) return s.setCurrentOrg(e.target.value);
        e.target.value = s.currentOrg;
        this.openDialog("new-org");
      }}>
      ${s.orgs.map((o) => html`<lit-material-select-option value=${o.id}>${orgLabel(o)}</lit-material-select-option>`)}
      <lit-material-select-option value=${NEW_ORG}>New organization…</lit-material-select-option>
    </lit-material-select>`;
  }

  // U49: the current org's fullest limit, before a create is refused (from the server's usage;
  // nothing when it couldn't be read or the server has no limits). Opens Plan & billing.
  usageChip() {
    const s = this.store;
    const all = usageWarnings(s.orgUsage?.get(s.currentOrg));
    const warn = all[0];
    if (!warn) return nothing;
    const label = all.map((m) => `${m.label}: ${m.text}${m.level === "full" ? ", full" : ", nearly full"}`).join("; ");
    return html`<button type="button" class=${`usage-chip ${warn.level}`} aria-label=${`Plan usage: ${label}`} title=${`${label}. Open Plan & billing`} @click=${() => this.openOrg("billing")}>
      ${icon("warning-circle")}<span class="mono">${warn.text}</span><span class="usage-chip-what">${warn.label.toLowerCase()}</span>${all.length > 1 ? html`<span class="mono usage-chip-more">+${all.length - 1}</span>` : nothing}
    </button>`;
  }

  /** U49: reads an org's usage again (after a create, a refusal, a change on the Organization page). */
  async refreshUsage(org = this.store.currentOrg) {
    const s = this.store;
    const client = s.library?.client ?? syncClient(s);
    const { loadUsage } = await import("../orgs.js");
    await loadUsage(s, client, org);
  }

  // Ask AI (the agent side sheet): an explanation after a failure, tests for a response, a change
  // to a request with a URL, else a request from text. Tests and explanations start at once.
  openAi(action, data = {}) {
    const t = this.store.tab;
    const failed = t.responseType === "error" || (t.tests || []).some((x) => !x.pass);
    const pick = action ?? (failed ? "explain" : t.statusCode != null ? "tests" : t.url ? "edit" : "generate");
    this.openAgent({ action: pick, ...data });
    if ((pick === "tests" || pick === "explain") && aiSettings().key) runAi(this, pick);
  }

  setAgent(changes) {
    this.agent = { ...(this.agent ?? {}), ...changes };
  }

  async openAgent(data) {
    this.navOpen = false;
    this.agent = { text: "", result: null, patch: null, dryRun: null, error: null, busy: false, ...data };
    await this.updateComplete;
    const sheet = this.querySelector("#agent");
    sheet?.show();
    await sheet?.updateComplete;
    requestAnimationFrame(() => this.querySelector("#ai-prompt")?.focus());
  }

  closeAgent() {
    this.aiAbort?.abort();
    this.querySelector("#agent")?.close();
  }

  renderAgent() {
    return html`<lit-material-side-sheet id="agent" class="agent-sheet" variant="modal" position="end" aria-label="Agent" @close=${() => this.aiAbort?.abort()}>
      <div slot="header" class="agent-head">${icon("sparkle")}<span>Agent</span>
        <lit-material-icon-button aria-label="Close the agent" @click=${() => this.closeAgent()}>${icon("x")}</lit-material-icon-button>
      </div>
      ${this.agent ? agentPanel(this) : nothing}
    </lit-material-side-sheet>`;
  }

  // New organization (then it's the current one) or an invite to the current one.
  async submitOrg(d) {
    const s = this.store;
    const client = s.library?.client ?? syncClient(s);
    try {
      if (d.kind === "new-org") {
        const trial = this.querySelector('#dialog input[name="org-start"]:checked')?.value === "trial";
        const org = await client.api("POST", "/v1/orgs", { name: this.field("name") });
        let note = "";
        if (trial) {
          try {
            await client.api("POST", `/v1/orgs/${encodeURIComponent(org.id)}/billing/trial`, {});
            note = " The Team trial has started.";
          } catch (e) {
            note = ` No trial: ${e.message}`;
          }
        }
        await this.reloadOrgs(client);
        s.setCurrentOrg(org.id);
        this.closeDialog();
        this.toast(`${org.name} created.${note}`);
        await this.openOrg("members");
        const usage = await client.api("GET", `/v1/orgs/${encodeURIComponent(org.id)}/usage`).catch(() => null);
        this.openDialog("org-invite", { seats: usage });
      } else {
        const emails = inviteEmails(this.field("email"));
        if (!emails.length) throw new Error("Enter at least one email address");
        const role = this.querySelector("#dialog #role")?.value || "member";
        const added = [...(d.added ?? [])];
        for (const email of emails) {
          await client.api("POST", `/v1/orgs/${encodeURIComponent(s.currentOrg)}/members`, { email, role });
          added.push(email);
        }
        this.dialog = { ...d, submitted: false, error: null, added, draft: "" };
        this.querySelector("hitop-org")?.load();
      }
    } catch (err) {
      const limit = limitInfo(err);
      if (limit) return this.openLimit(limit, s.currentOrgInfo);
      this.dialog = { ...d, submitted: false, error: err?.message || String(err) };
    }
  }

  // UX2: Esc with nothing of its own open (a dialog, the palette, a menu, a popover, a select's
  // list) stops what the request tab has in flight (7), or leaves a rail page for the workspace (3).
  onEscape(e) {
    if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (this.dialog || this.querySelector("lit-material-command-palette")?.open || document.querySelector("lit-material-confirm-dialog[open]")) return;
    // An open menu or popover takes its own Esc (a toast or tooltip is not one).
    if ([...document.querySelectorAll(":popover-open")].some((el) => !el.matches("lit-material-snackbar, lit-material-tooltip"))) return;
    const path = e.composedPath();
    if (path.some((el) => el instanceof Element && el.matches("lit-material-select, lit-material-menu, lit-material-popover, lit-material-sheet, dialog, .var-peek"))) return;
    if (document.querySelector("lit-material-menu[open], lit-material-popover[open], lit-material-select[open], lit-material-sheet[open]")) return;
    const s = this.store;
    if (s.view.kind === "request") {
      if (this.querySelector("hitop-request")?.cancelInFlight()) e.preventDefault();
      return;
    }
    this.leavePage?.(e);
  }

  async reloadOrgs(client) {
    const s = this.store;
    const { loadOrgs } = await import("../orgs.js");
    await loadOrgs(s, client ?? s.library?.client ?? syncClient(s));
  }

  /** The Organization area on one of its sections (org-view.js). */
  async openOrg(section = "general") {
    this.go("org");
    await this.updateComplete;
    this.querySelector("hitop-org")?.pick(section);
  }

  /** A plan limit stopped something (402): what still works, and who can change the plan. */
  openLimit(limit, org) {
    // The org the server refused (a share files under the collection's org, not the top bar's).
    org = this.store.orgs.find((o) => o.id === limit.org) ?? org;
    const admin = org ? org.personal || org.role === "owner" || org.role === "admin" : false;
    if (limit.org) this.refreshUsage(limit.org);
    this.openDialog("limit", { limit, admin });
  }

  /** The current org's plan page on the portal. */
  planUrl() {
    const org = this.store.currentOrg;
    return org ? `${PORTAL_URL}orgs/${encodeURIComponent(org)}/plan` : PORTAL_URL;
  }

  // Organization picker, environments and shortcuts: the top bar's
  // right end, or the top of the drawer on phones (one shows at a time).
  // The title bar's pills (HITOP Next): organization, environment. Phones: the top of the
  // drawer, with the buttons the status bar has on wide screens.
  tools(where) {
    const drawer = where === "in-drawer";
    return html`<div class=${`app-tools ${where}`}>
      ${this.orgSelect()}
      ${this.usageChip()}
      ${this.envSelect()}
      ${drawer
        ? html`<lit-material-icon-button aria-label="Manage environments" title="Manage environments" @click=${() => this.go("environments")}>${icon("sliders-horizontal")}</lit-material-icon-button>
            <lit-material-icon-button aria-label="Keyboard shortcuts" title="Keyboard shortcuts (?)" @click=${() => this.openDialog("shortcuts")}>${icon("keyboard")}</lit-material-icon-button>`
        : nothing}
    </div>`;
  }

  toggleCollection(id) {
    const key = String(id);
    this.collapsed = this.collapsed.includes(key) ? this.collapsed.filter((k) => k !== key) : [...this.collapsed, key];
    saveCollapsed(this.collapsed);
  }

  /** Unfolds a collection and the folders down to `folder`, so what was just added is on screen. */
  expand(collectionId, folder = "") {
    const keys = [String(collectionId)];
    const parts = folder ? folder.split("/") : [];
    for (let i = 1; i <= parts.length; i++) keys.push(`${collectionId}/${parts.slice(0, i).join("/")}`);
    const next = this.collapsed.filter((k) => !keys.includes(k));
    if (next.length !== this.collapsed.length) {
      this.collapsed = next;
      saveCollapsed(this.collapsed);
    }
  }

  // Folders made in the sidebar before any request is in them: on screen only
  // (a folder is the path in its requests' `file`), until a request is saved there.
  addFolder(collectionId, parent, name) {
    const base = name.trim().replace(/[\\/]+/g, "-");
    if (!base) return;
    const path = parent ? `${parent}/${base}` : base;
    const key = String(collectionId);
    const mine = this.emptyFolders[key] ?? [];
    if (!mine.includes(path)) this.emptyFolders = { ...this.emptyFolders, [key]: [...mine, path] };
    this.expand(collectionId, path);
  }

  folderRenamed(collectionId, path, newPath) {
    const key = String(collectionId);
    const mine = this.emptyFolders[key];
    if (mine) this.emptyFolders = { ...this.emptyFolders, [key]: mine.map((p) => (p === path || p.startsWith(`${path}/`) ? `${newPath}${p.slice(path.length)}` : p)) };
    this.collapsed = this.collapsed.map((k) => (k === `${collectionId}/${path}` || k.startsWith(`${collectionId}/${path}/`) ? `${collectionId}/${newPath}${k.slice(`${collectionId}/${path}`.length)}` : k));
    saveCollapsed(this.collapsed);
  }

  /** A new request in a collection (and folder) from the sidebar or a collection page. */
  newRequestIn(collectionId, folder = "") {
    this.newRequest(collectionId, folder);
  }

  /**
   * FV1: the one way a new request is made (the tab bar's +, Mod+N / Alt+Shift+N, the palette, the
   * sidebar): an "Untitled" request added to the collection on screen (or `collectionId`, in
   * `folder`), its row shown and selected, the URL field focused; the drawer closes.
   */
  newRequest(collectionId = null, folder = "") {
    this.navOpen = false;
    const s = this.store;
    const r = collectionId != null ? s.newRequestIn(collectionId, folder) : s.newRequestInFirstCollection();
    this.expand(r.collectionId, r.folder);
    this.focusUrl();
  }

  // A request dropped on another list (`sort` with toList !== fromList, lit-material
  // sortable `group`): its new collection and folder are the list's data attributes.
  dropRequest(e, c, entries) {
    const { from, to, toList, fromList } = e.detail;
    const request = entries[from]?.request;
    if (!request || !toList || toList === fromList) return;
    const toCollection = this.store.collections.find((x) => String(x.id) === toList.dataset.collection);
    if (!toCollection) return;
    const folder = toList.dataset.folder ?? "";
    const target = [...toList.querySelectorAll("[data-sort-id]")].filter((el) => el.dataset.sortId !== String(request.id));
    const beforeId = target[to] ? Number(target[to].dataset.sortId) : null;
    if (this.store.moveRequestTo(c.id, request.id, toCollection.id, folder, beforeId)) this.expand(toCollection.id, folder);
  }

  // Right click (or the context menu key) on a collection or a folder in the sidebar, or its "⋯".
  openCollectionMenu(e, collection, anchor = null) {
    e.preventDefault();
    e.stopPropagation();
    this.menuCollection = collection;
    const menu = this.querySelector("#collection-menu");
    if (anchor) menu.show(anchor);
    else showMenuFor(e, menu);
  }

  openFolderMenu(e, collection, folder, anchor = null) {
    e.preventDefault();
    e.stopPropagation();
    this.menuFolder = { collection, folder };
    const menu = this.querySelector("#folder-menu");
    if (anchor) menu.show(anchor);
    else showMenuFor(e, menu);
  }

  renderCollectionMenu() {
    const s = this.store;
    const c = () => this.menuCollection;
    const folder = this.menuCollection ? s.folderOf(this.menuCollection.id) : null;
    return html`<lit-material-menu id="collection-menu" aria-label="Collection">
      <lit-material-list-item interactive @click=${() => this.newRequestIn(c().id)}>${icon("plus")} New request</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("new-folder", { collectionId: c().id, parent: "" })}>${icon("folder-simple-plus")} New folder…</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.go("collection", c().id)}>${icon("play")} Open and run</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("rename-collection", { id: c().id, name: c().name })}>${icon("pencil-simple")} Rename…</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => s.duplicateCollection(c().id)}>${icon("copy")} Duplicate</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.deleteCollection(c())}>${icon(folder ? "x" : "trash")} ${folder ? "Close folder" : "Delete…"}</lit-material-list-item>
    </lit-material-menu>`;
  }

  /** U39: runs one folder's requests, its report on the collection page. */
  runFolder(collectionId, path) {
    const s = this.store;
    const c = s.collection(collectionId);
    if (!(c?.requests || []).some((r) => String(r.file ?? "").startsWith(`${path}/`))) return this.toast(`Nothing to run in ${path} yet: save a request into it first.`);
    if (s.run.running) return this.toast("A run is going on: wait for it to finish.");
    this.go("collection", collectionId);
    s.runCollection(collectionId, { folder: path });
  }

  renderFolderMenu() {
    const m = () => this.menuFolder;
    return html`<lit-material-menu id="folder-menu" aria-label="Folder">
      <lit-material-list-item interactive @click=${() => this.newRequestIn(m().collection.id, m().folder.path)}>${icon("plus")} New request here</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.runFolder(m().collection.id, m().folder.path)}>${icon("play")} Run this folder</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("new-folder", { collectionId: m().collection.id, parent: m().folder.path })}>${icon("folder-simple-plus")} New folder inside…</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("rename-folder", { collectionId: m().collection.id, path: m().folder.path, name: m().folder.name })}>${icon("pencil-simple")} Rename…</lit-material-list-item>
    </lit-material-menu>`;
  }

  // One level of a collection's requests, as a reorderable list (`sortable`):
  // `entries` are { request, index } with the index in the collection.
  // Every list is in the group "requests": a request dragged onto another
  // collection's or folder's list moves there (`dropRequest`).
  requestList(c, entries, label, activeRequest, sortable = true, folder = "") {
    const s = this.store;
    return html`<lit-material-sortable role="list" label=${label} group=${sortable ? "requests" : ""} data-collection=${String(c.id)} data-folder=${folder}
      @sort=${(e) => sortable && (e.detail.toList && e.detail.toList !== e.detail.fromList ? this.dropRequest(e, c, entries) : s.moveRequest(c.id, entries[e.detail.from].index, entries[e.detail.to].index))}>
      ${repeat(
        entries,
        (x) => x.request.id,
        ({ request: r }) => html`<div role="listitem" class="request-item" data-sort-id=${r.id} data-sort-label=${r.name}>
          <button type="button" class="tree-row leaf" aria-current=${activeRequest === r.id ? "page" : "false"}
            @click=${() => ((this.navOpen = false), s.openRequest(r, c.id))} @contextmenu=${(e) => this.openRequestMenu(e, r, c.id)}>
            ${this.requestRowTag(r, c.id)}<span>${this.requestRowName(r, c.id)}${s.isDirtyRequest(r.id, c.id) ? html`<span class="dirty-dot" aria-hidden="true">●</span><span class="visually-hidden">, unsaved changes</span>` : nothing}</span>${this.gitLetter(c, r)
              ? html`<span class="git-letter mono" title=${`Changed in git: ${this.gitLetter(c, r)}`}>${this.gitLetter(c, r)}<span class="visually-hidden"> (changed in git)</span></span>`
              : nothing}
          </button>
          ${sortable ? html`<button type="button" class="sort-handle" data-sort-handle aria-label=${`Reorder ${r.name}`}>${icon("dots-six-vertical")}</button>` : nothing}
        </div>`,
      )}
    </lit-material-sortable>`;
  }

  // A sidebar row's method tag and name: a new request's follow its open tab while its name
  // follows the URL; a name that starts with the method shows without it ("GET /users" is the GET
  // tag and "/users").
  requestRowTag(r, collectionId) {
    const live = this.store.autoNameTab(r, collectionId);
    const tag = live ? tagOf(live) : methodLabel(r);
    return html`<span class=${`method-tag m-${tag}`}>${tag}</span>`;
  }

  requestRowName(r, collectionId) {
    const live = this.store.autoNameTab(r, collectionId);
    const tag = live ? tagOf(live) : methodLabel(r);
    const name = live ? live.title : r.name;
    return name.startsWith(`${tag} `) && name.length > tag.length + 1 ? name.slice(tag.length + 1) : name;
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
    // No request here: an empty list still takes a dropped request, and says what to do.
    if (!node.requests.length) runs.push([]);
    // The hint only where there is nothing at all (a collection whose requests are all in folders needs none).
    const bare = !node.items.length;
    const label = node.path ? `Requests in ${c.name}, ${node.path}` : `Requests in ${c.name}`;
    return html`${runs.map((run) =>
      Array.isArray(run)
        ? html`<li>
            ${this.requestList(c, run, label, activeRequest, true, node.path)}
            ${run.length || !bare
              ? nothing
              : html`<div class="tree-hint">
                  ${isMyRequests(c) && !node.path ? nothing : html`<span>No requests yet</span>`}
                  <button type="button" class="tree-hint-action" @click=${() => this.newRequestIn(c.id, node.path)}>${icon("plus")} Add request</button>
                  ${isMyRequests(c) && !node.path
                    ? html`<span class="tree-hint-sep" aria-hidden="true">·</span><button type="button" class="tree-hint-action" aria-label="Import Postman, OpenAPI or HITOP" title="Import Postman, OpenAPI or HITOP collections" @click=${() => this.openDialog("import")}>${icon("file-arrow-up")} Import</button>`
                    : nothing}
                </div>`}
          </li>`
        : this.folderRow(c, run.folder, activeRequest),
    )}`;
  }

  folderRow(c, f, activeRequest) {
    const key = `${c.id}/${f.path}`;
    const open = !this.collapsed.includes(key);
    return html`<li class="tree-folder">
      <div class="collection-line folder-line" @contextmenu=${(e) => this.openFolderMenu(e, c, f)}>
        <button type="button" class="tree-row folder" aria-expanded=${String(open)} title=${f.path} @click=${() => this.toggleCollection(key)}>
          ${icon(open ? "caret-down" : "caret-right")}${icon("folder-simple")}<span>${f.name}</span>
        </button>
        <button type="button" class="row-action" aria-label=${`Add a request to ${f.name}`} title=${`Add a request to the folder ${f.name}`} @click=${() => this.newRequestIn(c.id, f.path)}>${icon("plus")}</button>
        <button type="button" class="row-action" aria-label=${`More for folder ${f.name}`} title="More" aria-haspopup="menu" @click=${(e) => this.openFolderMenu(e, c, f, e.currentTarget)}>${icon("dots-three")}</button>
      </div>
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
      <lit-material-list-item interactive @click=${() => this.openDialog("move-request", { ...m() })}>Move to…</lit-material-list-item>
      ${this.menuRequest && ["cloud", "github"].includes(storageOf(s, s.collection(m().collectionId) ?? { id: m().collectionId }).kind)
        ? html`<lit-material-list-item interactive @click=${() => {
            const d = { ...m(), history: null };
            this.openDialog("request-history", d);
            loadRequestHistory(this, d);
          }}>Version history…</lit-material-list-item>`
        : nothing}
      <lit-material-list-item interactive @click=${() => this.deleteRequest(m().request, m().collectionId)}>Delete</lit-material-list-item>
    </lit-material-menu>`;
  }

  // --- account and sharing --------------------------------------------------------------

  account() {
    return syncClient(this.store).account();
  }

  // What needs the person's attention: a clash with GitHub, a shared collection's conflicts, a
  // folder that can't be saved, HITOP Cloud refusing a save. Each with the place to fix it.
  needs() {
    const s = this.store;
    const out = [];
    const lib = s.library;
    if (lib?.status.on && lib.status.error) out.push({ text: lib.status.error, run: () => this.go("sync") });
    const sync = syncClient(s);
    for (const c of s.collections) {
      const w = storageOf(s, c);
      const link = sync.link(c.id);
      const conflicts = link ? Object.keys(sync.state(link.workspace)?.conflicts ?? {}).length : 0;
      if (conflicts) out.push({ text: `${c.name}: ${conflicts === 1 ? "a change" : `${conflicts} changes`} made by someone else too`, run: () => this.openShare(c, "people") });
      else if (w.attention) out.push({ text: `${c.name}: ${w.kind === "github" ? w.state : w.detail}`, run: () => this.openShare(c, this.tabFor(w)) });
    }
    return out;
  }

  tabFor(w) {
    return w.kind === "github" ? "github" : w.kind === "folder" ? "file" : "people";
  }

  /** The Share dialog of `c` on `tab` (people, github, file). */
  openShare(c, tab) {
    const web = platform().name === "web";
    const t = tab ?? this.tabFor(storageOf(this.store, c));
    this.openDialog("share", { collectionId: c.id, tab: t === "github" && !web ? "people" : t });
  }

  accountButton() {
    const a = this.account();
    if (!a) {
      return html`<lit-material-button variant="filled" class="account-signin" @click=${() => this.openSignIn()}>Sign in</lit-material-button>`;
    }
    const n = this.needs().length;
    const label = `Account, ${a.email}${n ? `, ${n} ${n === 1 ? "thing needs" : "things need"} you` : ""}`;
    return html`<button type="button" class="account-button" aria-label=${label} title=${label} aria-haspopup="menu"
        @click=${(e) => showMenuFor(e, this.querySelector("#account-menu"))}>
        <span class="account-avatar" aria-hidden="true">${(a.email || "?").charAt(0).toUpperCase()}</span>
        ${n ? html`<span class="account-badge" aria-hidden="true">${n}</span>` : nothing}
      </button>`;
  }

  renderAccountMenu() {
    const a = this.account();
    if (!a) return nothing;
    const lib = this.store.library;
    const st = lib?.status;
    const where = !st?.on ? "Collections stay on this device" : st.error ? "Couldn't save to HITOP Cloud" : st.saving ? "Saving to HITOP Cloud…" : "Collections saved to HITOP Cloud";
    const needs = this.needs();
    const org = this.store.currentOrgInfo;
    return html`<lit-material-menu id="account-menu" aria-label="Account">
      <div class="account-head" role="presentation"><b>${a.email}</b><span class="hint">${st?.on && !st.error ? icon("check-circle") : nothing} ${where}</span></div>
      ${needs.map((x) => html`<lit-material-list-item interactive class="account-need" @click=${() => x.run()}>${icon("warning-circle")} ${x.text}</lit-material-list-item>`)}
      <lit-material-list-item interactive @click=${() => this.go("sync")}>${icon("user-circle")} Account & devices</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("join")}>${icon("users-three")} Join with invite code…</lit-material-list-item>
      ${org && !org.personal && ["owner", "admin"].includes(org.role)
        ? html`<lit-material-list-item interactive @click=${() => this.openDialog("org-invite")}>${icon("plus")} Invite to ${org.name}…</lit-material-list-item>`
        : nothing}
      ${this.store.orgs?.length ? html`<lit-material-list-item interactive @click=${() => this.openOrg("billing")}>${icon("users-three")} Plan & billing</lit-material-list-item>` : nothing}
      <lit-material-list-item interactive @click=${() => this.signOut()}>${icon("sign-out")} Sign out</lit-material-list-item>
    </lit-material-menu>`;
  }

  async signOut() {
    await syncClient(this.store).signOut();
    await this.store.library?.checkAccount().catch(() => {});
    this.requestUpdate();
    this.store.show(this.store.view.kind === "sync" ? "collections" : this.store.view.kind, this.store.view.collectionId);
  }

  // The Share dialog's tabs: People (end to end encrypted, sync-view.js), GitHub (web, through
  // HITOP Cloud, github-view.js), File (a folder or a .zip).
  shareBody(c, tab) {
    const s = this.store;
    const web = platform().name === "web";
    // A mock needs the collection in the HITOP Cloud library (signed in, not a folder or shared one).
    const mockable = storageOf(s, c).kind === "cloud" || storageOf(s, c).kind === "github";
    const tabs = [["people", "People", "users-three"], ...(web && s.git?.enabled ? [["github", "GitHub", "github-logo"]] : []), ...(mockable ? [["mock", "Mock", "play"], ["monitor", "Monitor", "clock-counter-clockwise"]] : []), ["file", "File", "file-zip"]];
    const current = tabs.some(([k]) => k === tab) ? tab : "people";
    const pick = (k) => (this.dialog = { ...this.dialog, tab: k });
    const folder = s.folderOf(c.id);
    let panel;
    if (current === "people") panel = html`<hitop-sync .store=${s} .version=${s.version} .app=${this} .collection=${c}></hitop-sync>`;
    else if (current === "github") panel = html`<hitop-git-panel .store=${s} .version=${s.version} .collection=${c}></hitop-git-panel>`;
    else if (current === "mock") panel = html`<hitop-mock .store=${s} .collection=${c}></hitop-mock>`;
    else if (current === "monitor") panel = html`<hitop-monitor .store=${s} .collection=${c}></hitop-monitor>`;
    else {
      panel = html`<div class="share-file">
        <p>A collection as files: one YAML file per request, for your own git and for <code>hitop run</code> in CI.</p>
        ${folder
          ? html`<p>${icon("folder-open")} ${folder.connected ? `Saved to the folder ${folder.label} as you edit.` : `Opened from the folder ${folder.label}.`}</p>`
          : platform().folders?.pick
            ? html`<lit-material-button variant="outlined" @click=${() => (this.closeDialog(), s.saveToFolder(c.id))}>${icon("folder-open")} Save to a folder…</lit-material-button>`
            : nothing}
        <lit-material-button variant="outlined" @click=${() => s.exportZip(c.id)}>${icon("file-zip")} Download as .zip</lit-material-button>
        <p>A documentation page: one HTML file with every request, its headers, body and saved examples (credentials that aren't <code>\${variables}</code> are hidden). Publish it anywhere.</p>
        <lit-material-button variant="outlined" @click=${() => s.exportDocs(c.id)}>${icon("file-arrow-down")} Download docs page</lit-material-button>
      </div>`;
    }
    return html`<div class="share-tabs" role="tablist" aria-label="Share with">
        ${tabs.map(([k, label, ic]) => html`<button type="button" role="tab" id=${`share-tab-${k}`} aria-selected=${String(k === current)} aria-controls="share-panel" class="share-tab" @click=${() => pick(k)}>${icon(ic)} ${label}</button>`)}
      </div>
      <div id="share-panel" role="tabpanel" aria-labelledby=${`share-tab-${current}`}>${panel}</div>`;
  }

  // Where the collection is kept (git.js storageOf): its icon in the sidebar, a dot when it needs you.
  storageIcon(c) {
    const w = storageOf(this.store, c);
    return html`<span class="storage-icon" title=${w.detail} aria-hidden="true">${icon(w.icon)}${w.attention ? html`<span class="storage-dot" aria-hidden="true"></span>` : nothing}</span>`;
  }

  // The rail's sections (HITOP Next): icon only, the label is the tooltip and the accessible name.
  // Desktop-only pages come from the platform adapter.
  railItems() {
    const pages = (platform().pages ?? []).filter((page) => page.sidebar !== false);
    return [
      { kind: "collections", label: "Collections", icon: "folders", also: ["collection", "variables", "request"] },
      { kind: "history", label: "History", icon: "clock-counter-clockwise" },
      { kind: "environments", label: "Environments", icon: "globe" },
      { kind: "cookies", label: "Cookies", icon: "cookie" },
      ...pages.map((page) => ({ kind: page.kind, label: page.label, icon: page.icon })),
      ...(this.store.orgs?.length ? [{ kind: "org", label: "Organization", icon: "users-three" }] : []),
      { kind: "settings", label: "Settings", icon: "gear-six" },
    ];
  }

  renderRail() {
    const kind = this.store.view.kind;
    const items = this.railItems();
    const selected = items.findIndex((it) => it.kind === kind || it.also?.includes(kind));
    return html`<nav class="rail sidebar-footer" aria-label="Sections">
      <lit-material-navigation-rail compact .selected=${selected}>
        ${items.map((it) => html`<lit-material-navigation-rail-item @click=${() => this.go(it.kind)}>${icon(it.icon, { slot: "icon" })}${it.label}</lit-material-navigation-rail-item>`)}
      </lit-material-navigation-rail>
    </nav>`;
  }

  renderSidebar() {
    const s = this.store;
    const { kind, collectionId } = s.view;
    const activeRequest = kind === "request" ? s.tab?.loadedRequestId : null;
    const filter = this.filter.trim().toLowerCase();
    const matches = (r) => !filter || `${methodLabel(r)} ${r.name} ${r.url ?? ""}`.toLowerCase().includes(filter);
    return html`
      <aside class=${`sidebar ${this.navOpen ? "open" : ""}`} aria-label="Collections">
        ${this.renderRail()}
        <div class="side-pane">
        ${this.tools("in-drawer")}
        <div class="side-pane-top">
          <span class="side-title" id="side-title">Collections</span>
          <button type="button" class="mini new-collection" aria-label="New collection" title="New collection" @click=${() => this.openDialog("add-collection")}>${icon("folder-simple-plus")}</button>
          <button type="button" class="mini new-request" aria-label="Add a request" title="New request in the collection on screen" @click=${() => this.newRequest()}>${icon("plus")}</button>
        </div>
        <label class="tree-filter-wrap">
          ${icon("magnifying-glass")}
          <input type="search" class="tree-filter" placeholder="Filter" aria-label="Filter requests" .value=${this.filter}
            @input=${(e) => (this.filter = e.target.value)} />
        </label>
        <nav class="tree" aria-label="Collections">
          <ul>
            ${!s.visibleCollections.length
              ? html`<li class="tree-empty">
                  <p class="hint">${s.currentOrgInfo && !s.currentOrgInfo.personal ? `No collections in ${s.currentOrgInfo.name} yet.` : "No collections yet. A collection groups requests to run and share together."}</p>
                  <button type="button" class="tree-row" @click=${() => this.openDialog("add-collection")}>${icon("folder-simple-plus")}<span>New collection</span></button>
                  <button type="button" class="tree-row" @click=${() => this.openDialog("import")}>${icon("file-arrow-up")}<span>Import Postman, OpenAPI or HITOP</span></button>
                </li>`
              : nothing}
            ${s.visibleCollections.map((c) => {
              const requests = (c.requests || []).filter(matches);
              if (filter && !requests.length) return nothing;
              const open = Boolean(filter) || !this.collapsed.includes(String(c.id));
              return html`
                <li>
                  <div class="collection-line" @contextmenu=${(e) => this.openCollectionMenu(e, c)}>
                    <button type="button" class="tree-toggle" aria-expanded=${String(open)} aria-label="Requests" title=${`Show or hide the requests in ${c.name}`}
                      ?disabled=${Boolean(filter)} @click=${() => this.toggleCollection(c.id)}>${icon(open ? "caret-down" : "caret-right")}</button>
                    <button type="button" class="tree-row collection" aria-current=${kind === "collection" && collectionId === c.id ? "page" : "false"} aria-describedby=${`storage-of-${c.id}`} @click=${() => this.go("collection", c.id)}>
                      ${this.storageIcon(c)}<span>${c.name}</span>
                    </button>
                    <span class="visually-hidden" id=${`storage-of-${c.id}`}>${storageOf(s, c).detail}</span>
                    <button type="button" class="row-action" aria-label=${`Add a request to ${c.name}`} title=${`Add a request to ${c.name}`} @click=${() => this.newRequestIn(c.id)}>${icon("plus")}</button>
                    <button type="button" class="row-action" aria-label=${`More for ${c.name}`} title="More" aria-haspopup="menu" @click=${(e) => this.openCollectionMenu(e, c, e.currentTarget)}>${icon("dots-three")}</button>
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
                          : this.requestTreeRows(c, withFolders(requestTree(c.requests || []), this.emptyFolders[String(c.id)]), activeRequest)}
                      </ul>`
                    : nothing}
                </li>`;
            })}
          </ul>
        </nav>
        ${this.sideFooter()}
        </div>
      </aside>`;
  }

  // U6: the newcomer's three steps, checked off from what the app sees (getting-started.js).
  gettingStarted() {
    const s = this.store;
    let storage = null;
    let runs = null;
    try {
      storage = globalThis.localStorage ?? null;
      runs = JSON.parse(storage?.getItem("hitop_run_history") || "null");
    } catch {
      // storage blocked: decided from this page's state only
    }
    const steps = gettingStartedSteps({ history: s.history, collections: s.collections, runs, ranNow: Boolean(s.run.report), savedOnce: savedOnce(storage), keys: { send: SEND_KEYS, save: SAVE_KEYS } });
    if (!showGettingStarted(storage, { fresh: s.isFresh, steps })) return nothing;
    const done = steps.filter((x) => x.done).length;
    const next = steps.find((x) => !x.done);
    return html`<section class="getting-started" aria-labelledby="getting-started-title">
      <div class="gs-head">
        <h2 id="getting-started-title">Get started</h2>
        <span class="mono gs-count" aria-label=${`${done} of ${steps.length} done`}>${done}/${steps.length}</span>
        <button type="button" class="mini" aria-label="Hide getting started" title="Hide" @click=${() => (hideGettingStarted(storage), this.requestUpdate())}>${icon("x")}</button>
      </div>
      <ol class="gs-steps">
        ${steps.map(
          (x) => html`<li class=${x.done ? "done" : x === next ? "next" : ""} aria-current=${x === next ? "step" : "false"}>
            ${icon(x.done ? "check-circle" : "caret-right")}<span>${x.label}${x.done ? html`<span class="visually-hidden"> (done)</span>` : nothing}</span>
          </li>`,
        )}
      </ol>
      ${next ? html`<p class="hint gs-hint">${next.hint}</p>` : nothing}
      ${next?.id === "send" && !s.collections.some((c) => c.name === SAMPLE_NAME)
        ? html`<button type="button" class="link-button" @click=${() => (this.navOpen = false, s.addSampleCollection())}>${icon("play")}<span>Open the sample</span></button>`
        : nothing}
    </section>`;
  }

  // The pane's foot (HITOP Next): where collections are kept, the counts, Import and Export.
  sideFooter() {
    const s = this.store;
    const cols = s.visibleCollections;
    const requests = cols.reduce((n, c) => n + (c.requests?.length ?? 0), 0);
    const current = this.currentCollection() ?? cols[0];
    const w = current ? storageOf(s, current) : null;
    const where = !w
      ? IS_TAURI ? "On this computer" : "In this browser"
      : w.kind === "folder" ? "Files in your repo · no cloud"
      : w.kind === "shared" ? "Shared workspace · end-to-end encrypted"
      : w.kind === "github" ? `Saved to GitHub · ${w.label}`
      : w.kind === "cloud" ? "Saved in HITOP Cloud"
      : IS_TAURI ? "On this computer · no cloud" : "In this browser · no cloud";
    const place = w?.kind === "folder" ? w.label : "";
    const changed = current ? (this.gitStatus.get(current.id)?.files.length ?? 0) : 0;
    return html`<div class="side-pane-footer">
      ${this.gettingStarted()}
      <div class="side-card">
        <div class="side-card-line">${icon(w?.kind === "folder" ? "lock" : (w?.icon ?? (IS_TAURI ? "desktop" : "browser")))}<span>${where}</span></div>
        <div class="side-card-meta mono">${place ? html`${place} · ` : nothing}${cols.length} ${cols.length === 1 ? "collection" : "collections"} · ${requests} ${requests === 1 ? "request" : "requests"}${changed ? ` · ${changed} changed` : ""}</div>
        <div class="side-card-actions">
          <button type="button" class="link-button" @click=${() => this.openDialog("import")}>${icon("file-arrow-up")}<span>Import</span></button>
          <button type="button" class="link-button" @click=${() => this.openDialog("export")}>${icon("file-arrow-down")}<span>Export</span></button>
        </div>
      </div>
    </div>`;
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
          scrollable
          reorderable
          .selected=${s.currentTab}
          @change=${(e) => s.selectTab(e.target.selected)}
          @reorder=${(e) => s.moveTab(e.detail.from, e.detail.to)}
        >
          ${s.tabs.map(
            (t, i) => html`<lit-material-tab id=${`tab-${i}`} title=${t.title} @contextmenu=${(e) => this.openTabMenu(e, i)}
              @auxclick=${(e) => e.button === 1 && (e.preventDefault(), this.closeTab(i))}>${t.title.startsWith(`${tagOf(t)} `) ? nothing : html`<span class=${`method-tag tab-method m-${tagOf(t)}`} aria-hidden="true">${tagOf(t)}</span>`}${t.title}${s.isDirty(t)
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
        <lit-material-icon-button aria-label="New request" title="New request in this collection" @click=${() => this.newRequest()}>${icon("plus")}</lit-material-icon-button>
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

  // --- git status in the shell (desktop) --------------------------------------------------------

  /** Folder collections git can look at: connected, on disk (not hitop sync's app storage). */
  gitFolders() {
    const s = this.store;
    if (!platform().git) return [];
    return s.collections.map((c) => [c, s.folderOf(c.id)]).filter(([, f]) => f?.connected && !String(f.ref ?? "").startsWith("hitop-sync:"));
  }

  // A moment after the last change (edits save to the folder in batches), every folder's status.
  scheduleGitStatus() {
    const folders = this.gitFolders();
    if (!folders.length) {
      if (this.gitStatus.size) {
        this.gitStatus = new Map();
        this.requestUpdate();
      }
      return;
    }
    const key = `${this.store.version}|${folders.map(([c]) => c.id).join()}`;
    if (key === this.gitSeen) return;
    this.gitSeen = key;
    clearTimeout(this.gitTimer);
    this.gitTimer = setTimeout(() => this.refreshGitStatus(), 700);
  }

  async refreshGitStatus() {
    const api = platform().git;
    if (!api) return;
    const next = new Map();
    await Promise.all(
      this.gitFolders().map(async ([c, f]) => {
        try {
          next.set(c.id, await api.call("status", { path: f.ref }));
        } catch {
          // Not a repository, or git can't read it: no pill, no letters.
        }
      }),
    );
    this.gitStatus = next;
    this.requestUpdate();
  }

  /** The git letter of a saved request in a folder collection: M, A, D, R, ? (untracked). */
  gitLetter(c, r) {
    const st = this.gitStatus.get(c.id);
    if (!st) return "";
    // A request's file: its `file` (in a folder) or its name as hitop-core slugs it.
    const file = `${st.prefix ? `${st.prefix.replace(/\/$/, "")}/` : ""}requests/${r.file || slugName(r.name)}.yaml`;
    const f = st.files.find((x) => x.path === file);
    if (!f) return "";
    if (f.conflicted) return "!";
    const change = f.worktree ?? f.index;
    return { added: "A", modified: "M", deleted: "D", renamed: "R", typechange: "T", untracked: "?" }[change] ?? "M";
  }

  // The title bar's branch pill: the collection on screen, when git knows it; opens the Git page.
  branchPill() {
    const c = this.currentCollection();
    const st = c ? this.gitStatus.get(c.id) : null;
    if (!st) return nothing;
    const name = st.branch ?? (st.head ? `detached ${st.head}` : "no commits");
    const label = `Git: ${name}${st.upstream ? `, ${st.ahead} ahead, ${st.behind} behind` : ""}. Open the Git page`;
    return html`<button type="button" class="chip branch-pill" aria-label=${label} title=${label} @click=${() => this.go("git", c.id)}>
      ${icon("git-branch")}<span class="mono branch-name">${name}</span>
      ${st.upstream && st.ahead ? html`<span class="mono branch-ahead">↑${st.ahead}</span>` : nothing}
      ${st.upstream && st.behind ? html`<span class="mono branch-behind">↓${st.behind}</span>` : nothing}
    </button>`;
  }

  // --- title bar and status bar (HITOP Next) ---------------------------------------------------

  /** The collection on screen: the page's, or the open request's. */
  currentCollection() {
    const s = this.store;
    const { kind, collectionId } = s.view;
    if (kind === "collection" || kind === "variables") return s.collection(collectionId);
    if (kind === "git") return s.collection(collectionId) ?? this.gitFolders()[0]?.[0] ?? null;
    if (kind === "request") return s.collection(s.tab?.loadedCollectionId);
    return null;
  }

  // "HITOP / <collection> ▾": the collection on screen; the menu switches to another or
  // opens the collections page.
  crumb() {
    const c = this.currentCollection();
    const s = this.store;
    const cols = s.visibleCollections;
    if (!c && !cols.length) return nothing;
    const name = c?.name ?? "Collections";
    return html`<span class="crumb" aria-hidden="true">/</span>
      <button type="button" class="crumb crumb-name" title=${c ? `Collection: ${name}` : "Collections"} aria-haspopup="menu" aria-label=${c ? `Collection ${name}: switch` : "Collections"}
        @click=${(e) => showMenuFor(e, this.querySelector("#crumb-menu"))}>
        <span>${name}</span>${icon("caret-down")}
      </button>
      <lit-material-menu id="crumb-menu" aria-label="Collections">
        ${cols.map((x) => html`<lit-material-list-item interactive ?selected=${x.id === c?.id} @click=${() => this.go("collection", x.id)}>${x.name}</lit-material-list-item>`)}
        <lit-material-list-item interactive @click=${() => this.go("collections")}>All collections</lit-material-list-item>
        <lit-material-list-item interactive @click=${() => this.openDialog("add-collection")}>New collection…</lit-material-list-item>
      </lit-material-menu>`;
  }

  // The title bar's search field: one place for requests and commands.
  commandField() {
    return html`<button type="button" class="command-field" aria-label="Search requests and commands" @click=${() => this.openCommand()}>
      ${icon("magnifying-glass")}<span class="command-text">Search requests, run a command…</span><lit-material-kbd class="command-kbd" keys="Mod+K"></lit-material-kbd>
    </button>`;
  }

  /** The command palette (Mod+K): requests, actions, environments and runs. */
  openCommand() {
    this.navOpen = false;
    this.paletteItems = this.buildPaletteItems();
    this.requestUpdate();
    this.updateComplete.then(() => this.querySelector("lit-material-command-palette")?.show());
  }

  /** The palette's preview pane for a saved request: method and URL, headers, body, where it lives. */
  palettePreview(item) {
    const full = this.paletteItems?.find((x) => x.id === item.id);
    const r = full?.request;
    if (!r) return nothing;
    const headers = (r.headers || []).filter((h) => h.name && h.enabled !== false);
    const body = (r.body || "").split("\n").slice(0, 14).join("\n");
    const place = r.file || `${full.collection?.name ?? ""}/`;
    return html`<style>${PREVIEW_CSS}</style><div class="palette-preview">
      <div class="pp-line"><lit-material-method-tag method=${methodLabel(r)} size="medium"></lit-material-method-tag><span class="pp-url mono">${r.url || "(no URL)"}</span></div>
      <div class="pp-place mono">${place}</div>
      ${headers.length
        ? html`<div class="pp-label">Headers</div><dl class="pp-headers mono">${headers.slice(0, 8).map((h) => html`<dt>${h.name}</dt><dd>${h.value}</dd>`)}</dl>`
        : nothing}
      ${body ? html`<div class="pp-label">Body</div><pre class="pp-body mono">${body}</pre>` : nothing}
    </div>`;
  }

  buildPaletteItems() {
    return [...storeItems(this, methodLabel), ...paletteActions(this, ACTION_KEYS)];
  }

  // UX2 (2): a curl command pasted into the URL field fills the open request instead (any other
  // text pastes as usual).
  pasteIntoUrl(e) {
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (!isCurl(text)) return;
    e.preventDefault();
    let parsed;
    try {
      parsed = parseCurl(text);
    } catch (err) {
      this.toast(`That curl command can't be read: ${err.message}`);
      return;
    }
    this.applyCurl(parsed, false);
  }

  // The parsed command into the open request, with a toast that can undo it (`added`: the request
  // was made for it by Import, so Undo takes it out again).
  applyCurl(parsed, added) {
    const s = this.store;
    const undo = s.applyCurl(parsed);
    const id = s.tab?.id;
    const note = parsed.warnings.length ? ` ${parsed.warnings.join(" ")}` : "";
    this.toast(`Imported from curl: ${parsed.method} ${parsed.url}.${note}`, {
      label: "Undo",
      run: () => {
        undo?.();
        const at = s.tabs.findIndex((t) => t.id === id);
        if (added && at !== -1 && s.tabs.length > 1) this.closeTab(at);
      },
    });
  }

  focusUrl() {
    const s = this.store;
    if (s.view.kind !== "request") s.selectTab(s.currentTab);
    // The request view (and its field) may render a moment after the app does.
    return this.updateComplete.then(async () => {
      const view = this.querySelector("hitop-request");
      await view?.updateComplete;
      const field = view?.querySelector(".request-line .url");
      await field?.updateComplete;
      field?.focus();
    });
  }

  // Tab in the palette: the query goes to Ask AI as a "Request from text" prompt; with nothing
  // typed, Ask AI opens as from its button (tests, explain or generate).
  paletteToAi(e) {
    e.preventDefault();
    const query = e.detail?.query?.trim() ?? "";
    this.querySelector("lit-material-command-palette")?.hide();
    if (this.store.view.kind !== "request") this.store.selectTab(this.store.currentTab);
    if (query) this.openAgent({ action: "generate", prompt: query });
    else this.openAi();
  }

  renderPalette() {
    return html`<lit-material-command-palette label="Command palette" placeholder="Search requests, run a command, or press Tab to ask AI…"
      .items=${(this.paletteItems ?? []).map(({ run, request, collection, ...it }) => it)} .prefixes=${PREFIXES}
      .preview=${(item) => this.palettePreview(item)} .hints=${PALETTE_HINTS}
      @select=${(e) => this.paletteItems?.find((x) => x.id === e.detail.item.id)?.run()}
      @tab=${(e) => this.paletteToAi(e)}></lit-material-command-palette>`;
  }

  /** Where the open request lives (its file in a folder collection), for the status bar. */
  requestPlace() {
    const s = this.store;
    const t = s.tab;
    if (s.view.kind !== "request" || !t) return "";
    const c = s.collection(t.loadedCollectionId);
    const r = c?.requests?.find((x) => x.id === t.loadedRequestId);
    if (!c || !r) return "";
    // FV1: the title bar names the collection and the tab the request, so only a folder
    // collection's file (where the request is written) adds something here.
    const folder = s.folderOf(c.id);
    return folder && r.file ? `${folder.label}/requests/${r.file}.yaml` : "";
  }

  renderStatusBar() {
    const s = this.store;
    const t = s.view.kind === "request" ? s.tab : null;
    const dirty = t && s.isDirty(t);
    const place = t && s.loadedRequest(t) ? this.requestPlace() : "";
    // A new request nobody has filled in yet (closing it takes it out again).
    const fresh = t?.autoName && isBlankRequest(s.loadedRequest(t)) && s.loadedRequest(t)?.name === UNTITLED;
    const proxy = IS_TAURI ? "Proxy off" : "Proxy for public hosts";
    const engine = IS_TAURI ? "Local engine · Rust" : "Browser engine";
    // The last response's scheme and HTTP version ("HTTPS · h2"), when the engine said.
    const wire = t?.httpVersion ? protocolLabel(t.httpVersion, `${t.responseScheme || ""}:`) : "";
    // @lit-material/status-bar (HITOP Next): a labelled region, tabular numbers, wraps on phones.
    return html`<lit-material-status-bar class="statusbar" label="Status">
      ${place ? html`<lit-material-status-bar-item class="mono">${place}</lit-material-status-bar-item>` : nothing}
      ${dirty ? html`<lit-material-status-bar-item class="status-unsaved" tone="warning">● Unsaved · <lit-material-kbd keys="Mod+S"></lit-material-kbd> ${place ? "writes the file" : "saves it"}</lit-material-status-bar-item>` : nothing}
      ${t && s.loadedRequest(t) && !dirty
        ? fresh
          ? html`<lit-material-status-bar-item class="status-saved">New request, nothing to save yet</lit-material-status-bar-item>`
          : html`<lit-material-status-bar-item class="status-saved">${icon("check-circle")} Saved</lit-material-status-bar-item>`
        : nothing}
      ${t && !s.loadedRequest(t)
        ? html`<lit-material-status-bar-item class="status-unsaved" button description="Save to a collection" @click=${() => this.saveCurrent()}>● Draft, not in a collection yet · <lit-material-kbd keys="Mod+S"></lit-material-kbd> saves it to one</lit-material-status-bar-item>`
        : nothing}
      ${wire ? html`<lit-material-status-bar-item slot="end" class="mono status-wire">${wire}</lit-material-status-bar-item>` : nothing}
      <lit-material-status-bar-item slot="end" class="status-hint">${proxy}</lit-material-status-bar-item>
      <lit-material-status-bar-item slot="end">${engine}</lit-material-status-bar-item>
      <lit-material-status-bar-item slot="end" button description="Keyboard shortcuts" @click=${() => this.openDialog("shortcuts")}><lit-material-kbd keys="?"></lit-material-kbd> Shortcuts</lit-material-status-bar-item>
    </lit-material-status-bar>`;
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
    if (kind === "org") {
      return html`<hitop-org .store=${s} .version=${s.version} .app=${this}></hitop-org>`;
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
    return html`
      <a class="skip-link" href="#main">Skip to content</a>
      <header class="topbar">
        <lit-material-icon-button class="nav-toggle" aria-label=${this.navOpen ? "Close collections" : "Open collections"} aria-expanded=${String(this.navOpen)} @click=${() => (this.navOpen = !this.navOpen)}>
          ${icon(this.navOpen ? "x" : "list")}
        </lit-material-icon-button>
        ${IS_TAURI
          ? html`<span class="brand">${unsafeSVG(mark())}<span>HITOP</span></span>`
          : html`<a class="brand" href="/" aria-label="HITOP website">${unsafeSVG(mark())}<span>HITOP</span></a>`}
        ${this.crumb()}
        <span class="topbar-spacer"></span>
        ${this.commandField()}
        <span class="topbar-spacer"></span>
        ${this.tools("in-topbar")}
        ${this.branchPill()}
        ${this.accountButton()}
      </header>
      <div class="layout">
        ${this.renderSidebar()}
        <div class="scrim" ?hidden=${!this.navOpen} @click=${() => (this.navOpen = false)}></div>
        <main id="main" class="main">${this.renderMain()}</main>
      </div>
      ${this.renderStatusBar()}
      ${this.renderDialog()} ${this.renderRequestMenu()} ${this.renderCollectionMenu()} ${this.renderFolderMenu()} ${this.renderAccountMenu()} ${this.renderPalette()} ${this.renderAgent()}
      <lit-material-snackbar id="toast" closable>${this.toastAction
        ? html`<lit-material-button slot="action" variant="text" @click=${() => this.toastAction?.run()}>${this.toastAction.label}</lit-material-button>`
        : nothing}</lit-material-snackbar>`;
  }
}

customElements.define("hitop-app", HitopApp);
