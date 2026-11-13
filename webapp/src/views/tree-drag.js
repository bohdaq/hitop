// DD1: drag and drop in the sidebar tree. Rows (requests, folders, collections) drag after the
// pointer moves 5 px, so a click still opens them. Where it drops:
//   - a request row: before it (top half) or after it (bottom half);
//   - a folder row: before it (top quarter), inside it at the end (middle), after it (bottom quarter);
//     a collapsed folder held over for 600 ms opens;
//   - a collection row: inside it at the end (for a collection being dragged: before/after it);
//   - the tab strip (a request): opens it.
// Alt or Ctrl at the drop copies instead of moving. Esc cancels. Every move toasts with Undo.
// The tree's own pure maths is in tree-moves.js; this file only reads rows and the pointer.

const THRESHOLD = 5;
const HOLD_TO_OPEN = 600;
const EDGE = 32;

export class TreeDrag {
  constructor(app) {
    this.app = app;
    this.pending = null;
    this.drag = null;
    this.onMove = (e) => this.move(e);
    this.onUp = (e) => this.up(e);
    this.onKey = (e) => {
      if (e.key === "Escape" && this.drag) {
        e.preventDefault();
        e.stopPropagation();
        this.cancel();
      }
    };
  }

  /** pointerdown on the tree: maybe the start of a drag. */
  down(e) {
    if (e.button !== 0 || e.pointerType === "touch" || this.app.filter) return;
    if (e.target.closest(".row-action, .tree-toggle, input, a")) return;
    const row = e.target.closest("[data-drag]");
    if (!row) return;
    const node = this.nodeOf(row);
    if (!node) return;
    this.pending = { x: e.clientX, y: e.clientY, row, node, id: e.pointerId };
    window.addEventListener("pointermove", this.onMove, true);
    window.addEventListener("pointerup", this.onUp, true);
    window.addEventListener("pointercancel", this.onUp, true);
  }

  /** The thing a row stands for: { kind, collectionId, id?, path?, label }. */
  nodeOf(row) {
    const s = this.app.store;
    const c = s.collections.find((x) => String(x.id) === row.dataset.collection);
    if (!c) return null;
    const kind = row.dataset.drag;
    if (kind === "request") {
      const r = c.requests.find((x) => String(x.id) === row.dataset.request);
      return r ? { kind, collectionId: c.id, id: r.id, label: r.name } : null;
    }
    if (kind === "folder") return { kind, collectionId: c.id, path: row.dataset.folder, label: row.dataset.folder.split("/").pop() };
    if (kind === "collection") return { kind, collectionId: c.id, label: c.name };
    return null;
  }

  move(e) {
    if (this.pending && !this.drag) {
      if (Math.hypot(e.clientX - this.pending.x, e.clientY - this.pending.y) < THRESHOLD) return;
      this.start();
    }
    if (!this.drag) return;
    e.preventDefault();
    this.ghost.style.transform = `translate(${e.clientX + 12}px, ${e.clientY + 8}px)`;
    this.autoScroll(e);
    this.aim(e.clientX, e.clientY, e);
  }

  start() {
    const { node, row } = this.pending;
    this.drag = { node, target: null };
    row.classList.add("drag-source");
    this.app.classList.add("tree-dragging");
    this.ghost = document.createElement("div");
    this.ghost.className = "tree-drag-ghost";
    this.ghost.textContent = node.label;
    this.ghost.setAttribute("aria-hidden", "true");
    this.line = document.createElement("div");
    this.line.className = "tree-drop-line";
    this.line.hidden = true;
    document.body.append(this.ghost, this.line);
    window.addEventListener("keydown", this.onKey, true);
  }

  // The drop target under the pointer, and its marks (line or row highlight).
  aim(x, y, e) {
    const target = this.targetAt(x, y);
    this.drag.target = target;
    this.mark(target, e);
    clearTimeout(this.holdTimer);
    if (target?.inside && target.row?.getAttribute("aria-expanded") === "false" && target.row.dataset.drag === "folder") {
      const row = target.row;
      this.holdTimer = setTimeout(() => {
        if (this.drag?.target?.row === row) this.app.toggleCollection(`${row.dataset.collection}/${row.dataset.folder}`);
      }, HOLD_TO_OPEN);
    }
  }

  /** The drop target at (x, y) for `node` (the row being dragged, or a tab's request). */
  targetAt(x, y, node = this.drag.node) {
    const app = this.app;
    const el = document.elementFromPoint(x, y);
    if (!el) return null;
    if (node.kind === "request" && el.closest(".tab-strip")) return { tabs: true };
    const row = el.closest("nav.tree [data-drag]");
    if (!row) {
      // The empty part under the last collection: the end of the last collection.
      return null;
    }
    const rect = row.getBoundingClientRect();
    const f = (y - rect.top) / Math.max(rect.height, 1);
    const kind = row.dataset.drag;
    const s = app.store;
    const collectionId = s.collections.find((c) => String(c.id) === row.dataset.collection)?.id;
    if (collectionId == null) return null;
    if (node.kind === "collection") {
      if (kind !== "collection" || collectionId === node.collectionId) return null;
      return { row, collectionId, line: f < 0.5 ? "before" : "after" };
    }
    if (kind === "collection") return { row, collectionId, folder: "", inside: true };
    if (kind === "folder") {
      const path = row.dataset.folder;
      const parent = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
      if (node.kind === "folder" && node.collectionId === collectionId && (path === node.path || path.startsWith(`${node.path}/`))) return null;
      if (f < 0.25) return { row, collectionId, folder: parent, before: { folder: path }, line: "before" };
      if (f > 0.75 && row.getAttribute("aria-expanded") !== "true") return { row, collectionId, folder: parent, after: { folder: path }, line: "after" };
      return { row, collectionId, folder: path, inside: true };
    }
    if (kind === "request") {
      const folder = row.dataset.inFolder ?? "";
      if (node.kind === "folder" && node.collectionId === collectionId && (folder === node.path || folder.startsWith(`${node.path}/`))) return null;
      const id = s.collection(collectionId)?.requests.find((r) => String(r.id) === row.dataset.request)?.id;
      if (id == null) return null;
      return f < 0.5 ? { row, collectionId, folder, before: { request: id }, line: "before" } : { row, collectionId, folder, after: { request: id }, line: "after" };
    }
    return null;
  }

  mark(target, e, line = this.line) {
    this.app.querySelectorAll(".drop-inside").forEach((el) => el.classList.remove("drop-inside"));
    this.app.querySelector(".tab-strip")?.classList.toggle("drop-open", Boolean(target?.tabs));
    line.hidden = true;
    if (!target || target.tabs) return;
    const copy = e && (e.altKey || e.ctrlKey);
    if (this.ghost) this.ghost.dataset.copy = copy ? "true" : "false";
    if (target.inside) {
      target.row.closest(".collection-line, .request-item, .folder-line")?.classList.add("drop-inside");
      return;
    }
    const r = target.row.getBoundingClientRect();
    const nav = this.app.querySelector("nav.tree").getBoundingClientRect();
    const level = Number(target.row.getAttribute("aria-level")) || 1;
    const left = Math.max(nav.left, r.left) + Math.min(level, 6) * 4;
    Object.assign(line.style, { top: `${(target.line === "before" ? r.top : r.bottom) - 1}px`, left: `${left}px`, width: `${Math.max(40, nav.right - left - 8)}px`, height: "" });
    line.hidden = false;
  }

  autoScroll(e) {
    const nav = this.app.querySelector("nav.tree");
    if (!nav) return;
    const r = nav.getBoundingClientRect();
    if (e.clientY < r.top + EDGE) nav.scrollBy(0, -12);
    else if (e.clientY > r.bottom - EDGE) nav.scrollBy(0, 12);
  }

  up(e) {
    const drag = this.drag;
    const pending = this.pending;
    this.stopListening();
    if (!drag) {
      this.pending = null;
      return;
    }
    // The click that ends a drag opens nothing.
    const swallow = (ev) => (ev.stopPropagation(), ev.preventDefault());
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
    const target = e.type === "pointercancel" ? null : drag.target;
    this.cleanup(pending);
    if (target) this.app.dropTreeNode(drag.node, target, { copy: e.altKey || e.ctrlKey });
  }

  cancel() {
    const pending = this.pending;
    this.stopListening();
    this.cleanup(pending);
  }

  stopListening() {
    window.removeEventListener("pointermove", this.onMove, true);
    window.removeEventListener("pointerup", this.onUp, true);
    window.removeEventListener("pointercancel", this.onUp, true);
    window.removeEventListener("keydown", this.onKey, true);
  }

  cleanup(pending) {
    clearTimeout(this.holdTimer);
    pending?.row?.classList.remove("drag-source");
    this.app.classList.remove("tree-dragging");
    this.app.querySelectorAll(".drop-inside").forEach((el) => el.classList.remove("drop-inside"));
    this.app.querySelector(".tab-strip")?.classList.remove("drop-open");
    this.ghost?.remove();
    this.line?.remove();
    this.ghost = this.line = null;
    this.drag = null;
    this.pending = null;
  }
}
