// A part of the app shell (HitopApp, ../hitop-app.js).
import { ADD_ENVIRONMENT, NEW_ORG, PORTAL_URL, inviteEmails, saveCollapsed, showMenuFor, tagOf } from "./shared.js";
import { methodLabel } from "../../store.js";
import { agentPanel, runAi } from "../ai-view.js";
import { aiSettings } from "../../ai.js";
import { confirm } from "@lit-material/confirm";
import { folderOf, folderTitle } from "../../collection-tree.js";
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { isMyRequests } from "../../new-request.js";
import { limitInfo, usageWarnings } from "../../org.js";
import { orgLabel } from "../../orgs.js";
import { planLabel, storeBuild } from "../../store-mode.js";
import { repeat } from "lit/directives/repeat.js";
import { syncClient } from "../sync-view.js";

export const SidebarPart = (Base) =>
  class extends Base {
  // --- sidebar ------------------------------------------------------------------

  // The environment picker, next to Send (request-view.js): the current org's environments.
  // Phones: the active environment stays in sight in the top bar; a tap opens the drawer's picker.
  phoneEnvChip() {
    const env = this.store.activeEnvironment;
    return html`<button type="button" class=${`env-chip-phone ${env ? "has-env" : ""}`} aria-label=${`Environment: ${env ? this.store.environmentLabel(env) : "none"}. Change it`}
      @click=${() => (this.navOpen = true)}><span class="env-dot" aria-hidden="true"></span><span class="env-chip-name">${env ? this.store.environmentLabel(env) : "No env"}</span></button>`;
  }

  envSelect() {
    const s = this.store;
    const env = s.activeEnvironment;
    const envs = s.visibleEnvironments;
    // A pill with a coloured dot (HITOP Next): amber with an environment, grey without. FV1: with
    // none at all it says so and offers to add one (the Environments page).
    return html`<lit-material-select class=${`env-select pill-select ${env ? "has-env" : ""}`} variant="outlined" label="Environment" .value=${env?.id ?? "none"}
      @change=${(e) => {
        if (e.target.value !== ADD_ENVIRONMENT) return s.setActiveEnvironment(e.target.value === "none" ? null : e.target.value);
        e.target.value = env?.id ?? "none";
        this.go("environments");
      }}>
      <lit-material-select-option value="none">${envs.length ? "No environment" : "No environments"}</lit-material-select-option>
      ${envs.map((x) => html`<lit-material-select-option value=${x.id}>${s.environmentLabel(x)}</lit-material-select-option>`)}
      <lit-material-select-option value=${ADD_ENVIRONMENT}>${envs.length ? "Manage environments…" : "Add environment…"}</lit-material-select-option>
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

  /** DD3: Account → Devices, opened and in view. */
  async openDevices() {
    this.focusDevices = true;
    this.go("sync");
    await this.updateComplete;
    setTimeout(() => this.querySelector("#account-devices")?.scrollIntoView({ block: "start" }), 50);
  }

  // U49: the current org's fullest limit, before a create is refused (from the server's usage;
  // nothing when it couldn't be read or the server has no limits). Opens Plan & billing.
  usageChip() {
    const s = this.store;
    const all = usageWarnings(s.orgUsage?.get(s.currentOrg));
    const warn = all[0];
    if (!warn) return nothing;
    const label = all.map((m) => `${m.label}: ${m.text}${m.level === "full" ? ", full" : ", nearly full"}`).join("; ");
    // DD3: a device limit is about your own devices: the chip opens Account → Devices.
    const devices = warn.id === "devices";
    return html`<button type="button" class=${`usage-chip ${warn.level}`} aria-label=${`Plan usage: ${label}`} title=${`${label}. ${devices ? "Open your devices" : `Open ${planLabel()}`}`} @click=${() => (devices ? this.openDevices() : this.openOrg("billing"))}>
      ${icon("warning-circle")}<span class="mono">${warn.text}</span><span class="usage-chip-what">${warn.label.toLowerCase()}</span>${all.length > 1 ? html`<span class="mono usage-chip-more">+${all.length - 1}</span>` : nothing}
    </button>`;
  }

  /** U49: reads an org's usage again (after a create, a refusal, a change on the Organization page). */
  async refreshUsage(org = this.store.currentOrg) {
    const s = this.store;
    const client = s.library?.client ?? syncClient(s);
    const { loadUsage } = await import("../../orgs.js");
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
    // UX2: focus goes back where it was when the sheet closes (not to <body>).
    this.agentOpener = document.activeElement?.closest?.("hitop-app *") ? document.activeElement : null;
    this.agent = { text: "", result: null, patch: null, dryRun: null, error: null, busy: false, ...data };
    await this.updateComplete;
    const sheet = this.querySelector("#agent");
    sheet?.show();
    await sheet?.updateComplete;
    requestAnimationFrame(() => this.querySelector("#ai-prompt")?.focus());
  }

  agentReturnFocus() {
    const to = this.agentOpener;
    this.agentOpener = null;
    // The opener may be gone or hidden (a menu item): then the URL field.
    requestAnimationFrame(() => (to?.isConnected && to.getClientRects().length && !to.closest("lit-material-menu") ? to.focus() : this.focusUrl()));
  }

  closeAgent() {
    this.aiAbort?.abort();
    this.querySelector("#agent")?.close();
  }

  renderAgent() {
    return html`<lit-material-side-sheet id="agent" class="agent-sheet" variant="modal" position="end" aria-label="Agent" @close=${() => (this.aiAbort?.abort(), this.agentReturnFocus())}>
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
        // ST1: a store build offers no trial (the choice isn't shown there).
        const trial = !storeBuild() && this.querySelector('#dialog input[name="org-start"]:checked')?.value === "trial";
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
        if (this.stillOpen(d)) this.dialog = { ...d, submitted: false, error: null, added, draft: "" };
        this.querySelector("hitop-org")?.load();
      }
    } catch (err) {
      const limit = limitInfo(err);
      if (limit) return this.openLimit(limit, s.currentOrgInfo);
      if (this.stillOpen(d)) this.dialog = { ...d, submitted: false, error: err?.message || String(err) };
      else this.toast(err?.message || String(err));
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
    if (path.some((el) => el instanceof Element && el.matches("lit-material-select, lit-material-menu, lit-material-popover, lit-material-sheet, lit-material-side-sheet, dialog, .var-peek"))) return;
    if (document.querySelector("lit-material-menu[open], lit-material-popover[open], lit-material-select[open], lit-material-sheet[open], lit-material-side-sheet[open]")) return;
    const s = this.store;
    if (s.view.kind === "request") {
      if (this.querySelector("hitop-request")?.cancelInFlight()) e.preventDefault();
      return;
    }
    this.leavePage?.(e);
  }

  async reloadOrgs(client) {
    const s = this.store;
    const { loadOrgs } = await import("../../orgs.js");
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

  /** ST1: a desktop store build (Mac App Store, Microsoft Store): no purchase call to action anywhere. */
  storeBuild() {
    return storeBuild();
  }

  /** The current org's plan page on the portal (web only; null in a store build). */
  planUrl() {
    if (storeBuild()) return null;
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

  // DD1: a folder made in the sidebar is kept in its collection (`folders`), empty or not.
  addFolder(collectionId, parent, name) {
    const path = this.store.addFolder(collectionId, parent, name);
    if (path) this.expand(collectionId, path);
  }

  folderRenamed(collectionId, path, newPath) {
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

  /**
   * DD1: a dragged row dropped (tree-drag.js): a request opened on the tab strip, collections
   * reordered, or a request/folder moved or copied (store.moveNode), with Undo in the toast.
   */
  dropTreeNode(node, target, { copy = false } = {}) {
    const s = this.store;
    if (target.tabs) {
      const c = s.collection(node.collectionId);
      const r = c?.requests.find((x) => x.id === node.id);
      if (r) s.openRequest(r, c.id);
      return;
    }
    if (node.kind === "collection") {
      const ids = s.visibleCollections.map((c) => c.id);
      const from = ids.indexOf(node.collectionId);
      let to = ids.indexOf(target.collectionId);
      if (from === -1 || to === -1) return;
      if (from < to) to -= 1;
      if (target.line === "after") to += 1;
      if (to === from) return;
      const before = s.collections;
      s.reorderCollections(from, to);
      const after = s.collections;
      const undo = () => (s.collections === after ? ((s.collections = before), s.emit()) : this.toast("Can't undo that: the collections changed since."));
      this.toast(`Moved ${node.label}.`, { label: "Undo", run: undo });
      return;
    }
    this.moveTreeNode(node, { collectionId: target.collectionId, folder: target.folder ?? "", before: target.before ?? null, after: target.after ?? null }, { copy });
  }

  /** DD2: a tab dropped on the sidebar tree: its request moves there (an unsaved tab is saved there). */
  dropTabOnTree(node, target) {
    const s = this.store;
    const place = { collectionId: target.collectionId, folder: target.folder ?? "", before: target.before ?? null, after: target.after ?? null };
    if (!node.unsaved) return this.moveTreeNode(node, place);
    s.selectTab(node.tab);
    s.saveRequest(s.tabs[node.tab]?.title || "Untitled", place.collectionId, place.folder);
    this.expand(place.collectionId, place.folder);
    this.toast(`Saved ${node.label} to ${s.collection(place.collectionId)?.name ?? "the collection"}.`);
    return null;
  }

  /** DD2: Close other tabs / Close tabs to the right; unsaved and pinned tabs stay. */
  closeTabsFrom(index, which) {
    const s = this.store;
    const indices = s.tabs.map((_, i) => i).filter((i) => (which === "right" ? i > index : i !== index));
    const { kept } = s.closeTabsAt(indices);
    if (kept) this.toast(`Kept ${kept} tab${kept === 1 ? "" : "s"} with unsaved changes.`);
  }

  /** A request or folder moved (or copied) to `place`; the toast says where, with Undo. */
  moveTreeNode(node, place, { copy = false, quiet = false } = {}) {
    const s = this.store;
    const r = s.moveNode({ kind: node.kind, from: node.collectionId, id: node.id, path: node.path, to: place.collectionId, folder: place.folder, before: place.before, after: place.after, copy });
    if (!r.ok) {
      if (r.error) this.toast(r.error);
      return null;
    }
    if (node.kind === "folder" && !copy && node.collectionId === place.collectionId && r.path !== node.path) this.folderRenamed(place.collectionId, node.path, r.path);
    this.expand(place.collectionId, place.folder);
    const where = s.collection(place.collectionId)?.name ?? "";
    const into = place.folder ? `${where} › ${place.folder.replace(/\//g, " › ")}` : where;
    const undo = () => r.undo() || this.toast("Can't undo that: the collections changed since. Move it back by hand.");
    if (!quiet) this.toast(`${copy ? "Copied" : "Moved"} ${node.label} to ${into}.`, { label: "Undo", run: undo });
    return r;
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
      <lit-material-list-item interactive @click=${() => this.go("collection", c().id)}>${icon("folder-open")} Open</lit-material-list-item>
      <lit-material-list-item interactive ?disabled=${!c()?.requests?.length} @click=${() => (this.go("collection", c().id), s.runCollection(c().id))}>${icon("play")} Run</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("export", { only: String(c().id) })}>${icon("file-arrow-down")} Export…</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.go("variables", c().id)}>${icon("sliders-horizontal")} Variables…</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("rename-collection", { id: c().id, name: c().name })}>${icon("pencil-simple")} Rename… <lit-material-kbd slot="trailing" keys="F2"></lit-material-kbd></lit-material-list-item>
      <lit-material-list-item interactive @click=${() => s.duplicateCollection(c().id)}>${icon("copy")} Duplicate</lit-material-list-item>
      <hr class="menu-divider" role="separator" />
      <lit-material-list-item interactive class="menu-danger" @click=${() => this.deleteCollection(c())}>${icon(folder ? "x" : "trash")} ${folder ? "Close folder" : "Delete…"} <lit-material-kbd slot="trailing" keys="Delete"></lit-material-kbd></lit-material-list-item>
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
      <lit-material-list-item interactive @click=${() => this.openDialog("rename-folder", { collectionId: m().collection.id, path: m().folder.path, name: m().folder.name })}>${icon("pencil-simple")} Rename… <lit-material-kbd slot="trailing" keys="F2"></lit-material-kbd></lit-material-list-item>
      <hr class="menu-divider" role="separator" />
      <lit-material-list-item interactive class="menu-danger" @click=${() => this.deleteFolder(m().collection, m().folder.path)}>${icon("trash")} Delete folder…</lit-material-list-item>
    </lit-material-menu>`;
  }

  /** DD1: a folder deleted with what is in it, after a confirm that counts it. */
  async deleteFolder(c, path) {
    const n = (c.requests || []).filter((r) => folderOf(r) === path || folderOf(r).startsWith(`${path}/`)).length;
    const ok = await confirm({
      headline: `Delete the folder ${path.split("/").pop()}?`,
      message: n ? `Its ${n} request${n === 1 ? "" : "s"} and subfolders are deleted too.` : "It holds no requests.",
      confirmLabel: "Delete",
      destructive: true,
    });
    if (ok) this.store.deleteFolder(c.id, path);
  }

  // One level of a collection's requests: `entries` are { request, index } with the index in the
  // collection. Rows drag (tree-drag.js, DD1) unless the filter is on.
  requestList(c, entries, label, activeRequest, sortable = true, folder = "") {
    const s = this.store;
    const level = folder ? folder.split("/").length + 2 : 2;
    return html`<div role="none" class="request-list" aria-label=${label} data-collection=${String(c.id)} data-folder=${folder}>
      ${repeat(
        entries,
        (x) => x.request.id,
        ({ request: r }) => html`<div role="none" class="request-item">
          <button type="button" class="tree-row leaf" role="treeitem" tabindex="-1" aria-level=${level} data-tree="request" data-key=${`r:${c.id}:${r.id}`} data-collection=${String(c.id)} data-request=${String(r.id)}
            data-drag=${sortable ? "request" : nothing} data-in-folder=${folder} aria-description=${sortable ? "Drag to move, or Alt+Up and Alt+Down" : nothing}
            aria-current=${activeRequest === r.id ? "page" : "false"}
            @click=${() => ((this.navOpen = false), s.openRequest(r, c.id))} @contextmenu=${(e) => this.openRequestMenu(e, r, c.id)}>
            ${this.requestRowTag(r, c.id)}<span>${this.requestRowName(r, c.id)}${s.isDirtyRequest(r.id, c.id) ? html`<span class="dirty-dot" aria-hidden="true">●</span><span class="visually-hidden">, unsaved changes</span>` : nothing}</span>${this.gitLetter(c, r)
              ? html`<span class="git-letter mono" title=${`Changed in git: ${this.gitLetter(c, r)}`}>${this.gitLetter(c, r)}<span class="visually-hidden"> (changed in git)</span></span>`
              : nothing}
          </button>
          ${sortable ? html`<span class="drag-grip" aria-hidden="true">${icon("dots-six-vertical")}</span>` : nothing}
        </div>`,
      )}
    </div>`;
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
        ? html`<li role="none">
            ${this.requestList(c, run, label, activeRequest, true, node.path)}
            ${run.length || !bare
              ? nothing
              : html`<div class="tree-hint">
                  ${isMyRequests(c) && !node.path ? nothing : html`<span>No requests yet</span>`}
                  <button type="button" class="tree-hint-action" @click=${() => this.newRequestIn(c.id, node.path)}>${icon("plus")} Add request</button>

                </div>`}
          </li>`
        : this.folderRow(c, run.folder, activeRequest),
    )}`;
  }

  folderRow(c, f, activeRequest) {
    const key = `${c.id}/${f.path}`;
    const open = !this.collapsed.includes(key);
    const groupId = `group-${c.id}-${f.path.replace(/[^\w-]+/g, "_")}`;
    return html`<li class="tree-folder" role="none">
      <div class="collection-line folder-line" role="none" @contextmenu=${(e) => this.openFolderMenu(e, c, f)}>
        <button type="button" class="tree-row folder" role="treeitem" tabindex="-1" aria-level=${f.path.split("/").length + 1} aria-expanded=${String(open)} aria-owns=${open ? groupId : nothing}
          data-tree="folder" data-drag="folder" aria-description="Drag to move, or Alt+Up and Alt+Down" data-key=${`f:${c.id}:${f.path}`} data-collection=${String(c.id)} data-folder=${f.path} title=${f.path} @click=${() => this.toggleCollection(key)}>
          ${icon(open ? "caret-down" : "caret-right")}${icon("folder-simple")}<span>${this.store.folderOf(c.id) ? f.name : folderTitle(f.name)}</span>
        </button>
        <button type="button" class="row-action" tabindex="-1" aria-label=${`Add a request to ${f.name}`} title=${`Add a request to the folder ${f.name}`} @click=${() => this.newRequestIn(c.id, f.path)}>${icon("plus")}</button>
        <button type="button" class="row-action" tabindex="-1" aria-label=${`More for folder ${f.name}`} title="More (Shift+F10)" aria-haspopup="menu" @click=${(e) => this.openFolderMenu(e, c, f, e.currentTarget)}>${icon("dots-three")}</button>
      </div>
      ${open ? html`<ul role="group" id=${groupId}>${this.requestTreeRows(c, f, activeRequest)}</ul>` : nothing}
    </li>`;
  }
  };
