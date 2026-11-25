// A part of the app shell (HitopApp, ../hitop-app.js).
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { planLabel } from "../../store-mode.js";
import { platform } from "../../platform/index.js";
import { showMenuFor } from "./shared.js";
import { storageOf } from "../../git.js";
import { syncClient } from "../sync-view.js";

export const AccountPart = (Base) =>
  class extends Base {
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
      ${this.store.orgs?.length ? html`<lit-material-list-item interactive @click=${() => this.openOrg("billing")}>${icon("users-three")} ${planLabel()}</lit-material-list-item>` : nothing}
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
      { kind: "collections", label: "Collections", icon: "folders", also: ["collection", "variables", "request", "collections"] },
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
      <lit-material-navigation-rail ?compact=${!this.narrow?.matches} .selected=${selected}>
        ${items.map((it) => html`<lit-material-navigation-rail-item @click=${() => (it.kind === "collections" ? this.railCollections() : this.go(it.kind))}>${icon(it.icon, { slot: "icon" })}${it.label}</lit-material-navigation-rail-item>`)}
      </lit-material-navigation-rail>
      ${this.sidePaneToggle()}
    </nav>`;
  }

  // UX2 (3): the rail's Collections is the workspace (tabs and the open request); the overview
  // cards stay in the crumb menu ("All collections"). On the workspace already with the side pane
  // folded away, it brings the pane back.
  railCollections() {
    const s = this.store;
    if (s.view.kind === "request" && this.sideCollapsed()) return this.toggleSidePane();
    this.showWorkspace();
  }

  /** The workspace: the tab strip and the open request, as they were left. */
  showWorkspace() {
    this.navOpen = false;
    const s = this.store;
    if (s.view.kind !== "request") s.selectTab(Math.min(s.currentTab, s.tabs.length - 1));
  }

  // UX2 (3): Esc on a page (History, Environments, Cookies, Settings, a collection…) goes back to
  // the workspace, unless it is typing into a field there.
  // In a field, the first Esc leaves the field (focus goes to the page) and the next one goes back.
  leavePage(e) {
    if (this.navOpen) return;
    const typing = e.composedPath().some((n) => n.localName === "input" || n.localName === "textarea" || n.localName === "select" || n.isContentEditable || n.localName === "hitop-code-editor");
    e.preventDefault();
    if (typing) {
      let el = document.activeElement;
      while (el?.shadowRoot?.activeElement) el = el.shadowRoot.activeElement;
      el?.blur?.();
      document.activeElement?.blur?.();
      this.querySelector("#main")?.focus({ preventScroll: true });
      return;
    }
    this.showWorkspace();
  }

  // UX: a page (History, Environments, a collection…) keeps the way back to the open request.
  backToRequest() {
    const t = this.store.tabs[this.store.currentTab];
    if (!t) return nothing;
    return html`<div class="page-back-bar">
      <button type="button" class="back-to-request" @click=${() => this.showWorkspace()}>${icon("arrow-left")} Back to <span class="back-title">${this.tabLabel(t)}</span></button>
      <span class="hint key-hint"><lit-material-kbd keys="Escape"></lit-material-kbd></span>
    </div>`;
  }
  };
