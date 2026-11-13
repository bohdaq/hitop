// DD2: while a tab is dragged (lit-material-tabs `reorderable` moves it and marks it `dragging`),
// a line shows where it will land, the strip scrolls at its edges, and over the sidebar tree the
// tab's request can be dropped into a collection or folder (tree-drag.js finds the place; the
// tab strip's own reorder is stopped then).

const EDGE = 40;

export class TabDrag {
  constructor(app) {
    this.app = app;
    this.line = null;
    this.target = null;
    window.addEventListener("pointermove", (e) => this.move(e), { capture: true, passive: true });
    window.addEventListener("pointerup", (e) => this.up(e), true);
  }

  dragged() {
    return this.app.querySelector(".tab-strip lit-material-tab[dragging]");
  }

  move(e) {
    const tab = this.dragged();
    if (!tab) return this.clear();
    if (!this.line) {
      this.line = document.createElement("div");
      this.line.className = "tree-drop-line";
      document.body.append(this.line);
    }
    const strip = this.app.querySelector(".tab-strip lit-material-tabs");
    const box = strip.getBoundingClientRect();
    // Over the sidebar: a place in the tree for the tab's request.
    if (e.clientY > box.bottom + 8) {
      const node = this.nodeOf(tab);
      this.target = node ? this.app.treeDrag.targetAt(e.clientX, e.clientY, node) : null;
      this.app.treeDrag.mark(this.target?.tabs ? null : this.target, e, this.line);
      tab.toggleAttribute("data-leaving", true);
      return;
    }
    tab.toggleAttribute("data-leaving", false);
    this.target = null;
    this.app.querySelectorAll(".drop-inside").forEach((el) => el.classList.remove("drop-inside"));
    // In the strip: the same rule lit-material uses (the first other tab whose middle is past the pointer).
    const others = [...strip.querySelectorAll("lit-material-tab")].filter((t) => t !== tab);
    const next = others.find((t) => {
      const r = t.getBoundingClientRect();
      return e.clientX < r.left + r.width / 2;
    });
    const r = (next ?? others.at(-1))?.getBoundingClientRect();
    if (!r) return;
    const x = next ? r.left : r.right;
    Object.assign(this.line.style, { left: `${x - 1}px`, top: `${box.top + 4}px`, width: "2px", height: `${box.height - 8}px` });
    this.line.hidden = false;
    const scroller = strip.shadowRoot?.querySelector(".scroller");
    if (scroller) {
      if (e.clientX < box.left + EDGE) scroller.scrollBy(-16, 0);
      else if (e.clientX > box.right - EDGE) scroller.scrollBy(16, 0);
    }
  }

  // The tab's request as a tree node ({ kind: "request", ... }), or a "save here" node for a tab
  // that isn't in a collection yet.
  nodeOf(tab) {
    const s = this.app.store;
    const index = [...tab.parentElement.querySelectorAll("lit-material-tab")].indexOf(tab);
    const t = s.tabs[index];
    if (!t) return null;
    const c = t.loadedCollectionId != null ? s.collection(t.loadedCollectionId) : null;
    const r = c?.requests.find((x) => x.id === t.loadedRequestId);
    return r ? { kind: "request", collectionId: c.id, id: r.id, label: r.name, tab: index } : { kind: "request", collectionId: null, label: t.title, tab: index, unsaved: true };
  }

  up(e) {
    const tab = this.dragged();
    if (!tab) return this.clear();
    const target = this.target;
    if (target && !target.tabs) {
      // Dropped on the tree: stop the strip's own reorder, then put the request there.
      e.stopPropagation();
      const strip = this.app.querySelector(".tab-strip lit-material-tabs");
      strip?.endDrag?.();
      const node = this.nodeOf(tab);
      this.clear();
      if (node) this.app.dropTabOnTree(node, target);
      return;
    }
    this.clear();
  }

  clear() {
    this.target = null;
    this.app.querySelectorAll(".tab-strip lit-material-tab[data-leaving]").forEach((t) => t.removeAttribute("data-leaving"));
    if (this.line) {
      this.line.remove();
      this.line = null;
      this.app.querySelectorAll(".drop-inside").forEach((el) => el.classList.remove("drop-inside"));
    }
  }
}
