// A part of the app shell (HitopApp, ../hitop-app.js).
import { folderOf, requestTree } from "../../collection-tree.js";
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { loadRequestHistory } from "../history-dialog.js";
import { neighbourPlace, treeOf } from "../../tree-moves.js";
import { showMenuFor } from "./shared.js";
import { storageOf } from "../../git.js";

export const SidebarTreePart = (Base) =>
  class extends Base {
  // --- the sidebar as a keyboard tree (UX2 (9)): one Tab stop, roving tabindex --------------
  // Up/Down move, Home/End, Right opens (or goes to the first child), Left folds (or goes to the
  // parent), Enter/Space open, F2 renames, Delete asks, Shift+F10 / the menu key opens the row's
  // menu, Alt+Up/Down moves a request within its list.

  treeItems() {
    return [...this.querySelectorAll('nav.tree [role="treeitem"]')].filter((el) => el.offsetParent !== null);
  }

  // The one row with tabindex 0: the last one focused, else the current request's (or page's), else the first.
  rovingTree() {
    const items = this.treeItems();
    if (!items.length) return;
    const pick = items.find((el) => el.dataset.key === this.treeKeyNow) ?? items.find((el) => el.getAttribute("aria-current") === "page") ?? items[0];
    for (const el of items) el.tabIndex = el === pick ? 0 : -1;
  }

  treeFocused(e) {
    const item = e.target.closest?.('[role="treeitem"]');
    if (!item) return;
    this.treeKeyNow = item.dataset.key;
    this.rovingTree();
  }

  treeRowData(el) {
    const s = this.store;
    const c = s.collection(Number(el.dataset.collection)) ?? s.collections.find((x) => String(x.id) === el.dataset.collection);
    const request = el.dataset.request ? c?.requests.find((r) => String(r.id) === el.dataset.request) : null;
    const folder = el.dataset.folder ? { path: el.dataset.folder, name: el.dataset.folder.split("/").pop() } : null;
    return { kind: el.dataset.tree, c, request, folder };
  }

  focusTreeItem(el) {
    if (!el) return;
    this.treeKeyNow = el.dataset.key;
    this.rovingTree();
    el.focus();
  }

  async focusTreeKey(key) {
    this.treeKeyNow = key;
    await this.updateComplete;
    this.focusTreeItem(this.treeItems().find((el) => el.dataset.key === key));
  }

  treeKey(e) {
    const item = e.target.closest?.('[role="treeitem"]');
    if (!item || e.ctrlKey || e.metaKey) return;
    const items = this.treeItems();
    const at = items.indexOf(item);
    const level = Number(item.getAttribute("aria-level")) || 1;
    const expanded = item.getAttribute("aria-expanded");
    const { kind, c, request, folder } = this.treeRowData(item);
    const done = () => (e.preventDefault(), e.stopPropagation());
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      if ((kind !== "request" && kind !== "folder") || !c || this.filter) return;
      done();
      this.moveTreeRow(item, c, request, folder, e.key === "ArrowUp" ? -1 : 1);
      return;
    }
    if (e.altKey) return;
    switch (e.key) {
      case "ArrowDown":
        done();
        this.focusTreeItem(items[Math.min(at + 1, items.length - 1)]);
        break;
      case "ArrowUp":
        done();
        this.focusTreeItem(items[Math.max(at - 1, 0)]);
        break;
      case "Home":
        done();
        this.focusTreeItem(items[0]);
        break;
      case "End":
        done();
        this.focusTreeItem(items.at(-1));
        break;
      case "ArrowRight":
        done();
        if (expanded === "false") this.foldTreeRow(kind, c, folder, item.dataset.key);
        else if (expanded === "true" && Number(items[at + 1]?.getAttribute("aria-level")) > level) this.focusTreeItem(items[at + 1]);
        break;
      case "ArrowLeft": {
        done();
        if (expanded === "true") {
          this.foldTreeRow(kind, c, folder, item.dataset.key);
          break;
        }
        for (let i = at - 1; i >= 0; i--) {
          if (Number(items[i].getAttribute("aria-level")) < level) {
            this.focusTreeItem(items[i]);
            break;
          }
        }
        break;
      }
      case "F2":
        done();
        if (kind === "request") this.openDialog("rename-request", { request, collectionId: c.id, name: request.name });
        else if (kind === "collection") this.openDialog("rename-collection", { id: c.id, name: c.name });
        else if (kind === "folder") this.openDialog("rename-folder", { collectionId: c.id, path: folder.path, name: folder.name });
        break;
      case "Delete":
        done();
        if (kind === "request") this.deleteRequest(request, c.id);
        else if (kind === "collection") this.deleteCollection(c);
        break;
      case "ContextMenu":
      case "F10":
        if (e.key === "F10" && !e.shiftKey) return;
        done();
        this.treeMenu(item, kind, c, request, folder);
        break;
      default:
    }
  }

  // Shift+F10 / the menu key: the row's own menu, at the row.
  treeMenu(item, kind, c, request, folder) {
    const fake = { preventDefault() {}, stopPropagation() {}, currentTarget: item, target: item, clientX: 0, clientY: 0 };
    if (kind === "request") {
      this.menuRequest = { request, collectionId: c.id };
      showMenuFor(fake, this.querySelector("#request-menu"));
    } else if (kind === "collection") this.openCollectionMenu(fake, c, item);
    else if (kind === "folder") this.openFolderMenu(fake, c, folder, item);
  }

  foldTreeRow(kind, c, folder, key) {
    if (kind === "collection") this.toggleCollection(c.id);
    else if (kind === "folder") this.toggleCollection(`${c.id}/${folder.path}`);
    this.focusTreeKey(key);
  }

  // Alt+Up/Down: a request or folder swaps places with its neighbour in the same folder (DD1).
  moveTreeRow(item, c, request, folder, by) {
    const parentOf = (p) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
    const inFolder = request ? folderOf(request) : parentOf(folder.path);
    const place = neighbourPlace(treeOf(c, requestTree), inFolder, request ? { request: request.id } : { folder: folder.path }, by);
    if (!place) return;
    const node = request ? { kind: "request", collectionId: c.id, id: request.id, label: request.name } : { kind: "folder", collectionId: c.id, path: folder.path, label: folder.name };
    if (!this.moveTreeNode(node, { collectionId: c.id, ...place }, { quiet: true })) return;
    this.treeLive = `${node.label} moved ${by < 0 ? "up" : "down"}.`;
    this.requestUpdate();
    this.focusTreeKey(item.dataset.key);
  }

  // The request menu's Copy as cURL: hitop-core's snippet, secrets left as {{NAME}}.
  async copyCurl(request, collectionId) {
    try {
      const { code } = await this.store.snippetFor(request, collectionId, "curl");
      await navigator.clipboard?.writeText(code);
      this.toast(`Copied ${request.name} as cURL.`);
    } catch (e) {
      this.toast(`Couldn't copy it as cURL: ${e?.message ?? e}`);
    }
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
    // UX2 (9): icons and keys, Run and Copy as cURL; Delete set apart at the end.
    const key = (k) => html`<lit-material-kbd slot="trailing" keys=${k}></lit-material-kbd>`;
    return html`<lit-material-menu id="request-menu" aria-label="Request">
      <lit-material-list-item interactive @click=${() => ((this.navOpen = false), s.openRequest(m().request, m().collectionId))}>${icon("arrow-square-out", { slot: "leading" })} Open ${key("Enter")}</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => ((this.navOpen = false), s.openRequest(m().request, m().collectionId), s.send())}>${icon("paper-plane-tilt", { slot: "leading" })} Run</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.copyCurl(m().request, m().collectionId)}>${icon("terminal-window", { slot: "leading" })} Copy as cURL</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("rename-request", { ...m(), name: m().request.name })}>${icon("pencil-simple", { slot: "leading" })} Rename ${key("F2")}</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => s.duplicateRequest(m().collectionId, m().request.id)}>${icon("copy", { slot: "leading" })} Duplicate</lit-material-list-item>
      <lit-material-list-item interactive @click=${() => this.openDialog("move-request", { ...m() })}>${icon("arrow-right", { slot: "leading" })} Move to…</lit-material-list-item>
      ${this.menuRequest && ["cloud", "github"].includes(storageOf(s, s.collection(m().collectionId) ?? { id: m().collectionId }).kind)
        ? html`<lit-material-list-item interactive @click=${() => {
            const d = { ...m(), history: null };
            this.openDialog("request-history", d);
            loadRequestHistory(this, d);
          }}>${icon("clock-counter-clockwise", { slot: "leading" })} Version history…</lit-material-list-item>`
        : nothing}
      <hr class="menu-divider" role="separator" />
      <lit-material-list-item interactive class="menu-danger" @click=${() => this.deleteRequest(m().request, m().collectionId)}>${icon("trash", { slot: "leading" })} Delete… ${key("Delete")}</lit-material-list-item>
    </lit-material-menu>`;
  }
  };
