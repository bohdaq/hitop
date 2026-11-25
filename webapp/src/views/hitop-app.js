// The app shell: sidebar with collections, the request tabs, the main view,
// and every dialog. Light DOM, so the global theme and app.css style it.
import { LitElement } from "lit";
import { Store } from "../store.js";
import { TabDrag } from "./tab-drag.js";
import { TreeDrag } from "./tree-drag.js";

import { loadCollapsed } from "./app/shared.js";
import { parseOrgLink, sectionsFor } from "../org.js";
import { platform } from "../platform/index.js";
import { shortcutFor } from "../shortcuts.js";
import { storeBuild } from "../store-mode.js";
import "@lit-material/navigation";
import "@lit-material/command-palette";
import "./request-view.js";
import "./collection-views.js";
import "./settings-view.js";
import "./history-view.js";
import "./github-view.js";
import "./limit-note.js";
import "./org-view.js";
import "./mock-view.js";
import "./monitor-view.js";
import { QuickOpenPart } from "./app/quick-open.js";
import { SignInPart } from "./app/sign-in.js";
import { DialogsPart } from "./app/dialogs.js";
import { SidebarPart } from "./app/sidebar.js";
import { SidebarTreePart } from "./app/sidebar-tree.js";
import { AccountPart } from "./app/account.js";
import { SidePanePart } from "./app/side-pane.js";
import { TabsPart } from "./app/tabs.js";
import { GitStatusPart } from "./app/git-status.js";
import { BarsPart } from "./app/bars.js";
import { MainPart } from "./app/main.js";

// The shell by subject (app/); this file keeps its properties, start, keys and render.
const Parts = [QuickOpenPart, SignInPart, DialogsPart, SidebarPart, SidebarTreePart, AccountPart, SidePanePart, TabsPart, GitStatusPart, BarsPart, MainPart].reduce((Base, part) => part(Base), LitElement);

export class HitopApp extends Parts {
  static properties = {
    navOpen: { state: true },
    dialog: { state: true },
    filter: { state: true },
    collapsed: { state: true },
    menuTab: { state: true },
    menuCollection: { state: true },
    menuFolder: { state: true },
    exportFormat: { state: true },
    agent: { state: true },
  };

  constructor() {
    super();
    // UX2 (4): the widths the side pane and the drawer follow (made here, so the first render knows).
    this.narrow = globalThis.matchMedia?.("(max-width: 899px)") ?? null;
    this.compact = globalThis.matchMedia?.("(max-width: 1200px)") ?? null;
    this.store = new Store();
    this.navOpen = false;
    this.treeDrag = new TreeDrag(this);
    this.tabDrag = new TabDrag(this);
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
    // UX2 (4): the side pane folds by itself at 1200 px or less (until chosen); phones use the drawer.
    for (const m of [this.narrow, this.compact]) m?.addEventListener?.("change", () => this.requestUpdate());
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
        const label = sectionsFor(null, { store: storeBuild() }).find(([k]) => k === link.section)?.[1] ?? "the organization";
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
    this.rovingTree();
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
      case "sidePane":
        this.toggleSidePane();
        break;
      case "environment": {
        // UX2: the environment picker next to Send (or the one in the drawer on phones).
        if (!onRequest) s.selectTab(s.currentTab);
        this.updateComplete.then(() => {
          const pick = [...this.querySelectorAll(".env-select")].find((el) => el.getClientRects().length);
          const trigger = pick?.shadowRoot?.querySelector('[part="trigger"]');
          (trigger ?? pick)?.focus();
          trigger?.click();
        });
        break;
      }
      case "filter":
        if (this.sideCollapsed()) this.toggleSidePane();
        if (this.narrow?.matches) this.navOpen = true;
        this.updateComplete.then(() => this.querySelector(".tree-filter")?.focus());
        break;
      case "responseTabs":
        this.querySelector('hitop-request lit-material-tabs[aria-label="Response details"] lit-material-tab[selected]')?.focus();
        break;
      case "help":
        this.openDialog("shortcuts");
        break;
      case "quickOpen":
        this.openDialog("quick-open", { query: "", active: 0 });
        break;
    }
  }
}

customElements.define("hitop-app", HitopApp);
