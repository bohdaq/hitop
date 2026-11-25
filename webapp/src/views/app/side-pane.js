// A part of the app shell (HitopApp, ../hitop-app.js).
import { ACTION_KEYS, SAVE_KEYS, SEND_KEYS, keyLabel } from "../../shortcuts.js";
import { IS_TAURI } from "../../platform/index.js";
import { SAMPLE_NAME, gettingStartedSteps, hideGettingStarted, importedIds, savedOnce, showGettingStarted } from "../../getting-started.js";
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { loadSidePane, saveLayout } from "../../split.js";
import { methodLabel } from "../../store.js";
import { requestTree } from "../../collection-tree.js";
import { storageOf } from "../../git.js";
import { treeOf } from "../../tree-moves.js";

export const SidePanePart = (Base) =>
  class extends Base {
  // --- the side pane (UX2 (4)): Mod+B, remembered in hitop_layout.sidePane (true = shown,
  // false = folded); not chosen yet, it folds by itself on a request at 1200 px or less ---------
  sideCollapsed() {
    if (this.narrow?.matches) return false; // phones: the drawer instead
    if (this.sidePref === undefined) this.sidePref = loadSidePane();
    if (typeof this.sidePref === "boolean") return !this.sidePref;
    return Boolean(this.compact?.matches) && this.store.view.kind === "request";
  }

  toggleSidePane() {
    if (this.narrow?.matches) {
      this.navOpen = !this.navOpen;
      return;
    }
    const shown = this.sideCollapsed();
    this.sidePref = shown;
    saveLayout({ sidePane: shown });
    this.requestUpdate();
  }

  sidePaneToggle() {
    const collapsed = this.sideCollapsed();
    const label = collapsed ? "Show the side pane" : "Hide the side pane";
    return html`<button type="button" class="mini rail-toggle" aria-label=${label} title=${`${label} (${keyLabel(ACTION_KEYS.sidePane)})`} aria-expanded=${String(!collapsed)} aria-controls="side-pane"
      @click=${() => this.toggleSidePane()}>${icon("sidebar-simple")}</button>`;
  }

  renderSidebar() {
    const s = this.store;
    const { kind, collectionId } = s.view;
    const activeRequest = kind === "request" ? s.tab?.loadedRequestId : null;
    const filter = this.filter.trim().toLowerCase();
    const matches = (r) => !filter || `${methodLabel(r)} ${r.name} ${r.url ?? ""}`.toLowerCase().includes(filter);
    return html`
      <aside class=${`sidebar ${this.navOpen ? "open" : ""} ${this.sideCollapsed() ? "side-collapsed" : ""}`} aria-label="Collections">
        ${this.renderRail()}
        <div class="side-pane" id="side-pane" ?hidden=${this.sideCollapsed()}>
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
        <nav class="tree" aria-label="Collections" @keydown=${(e) => this.treeKey(e)} @focusin=${(e) => this.treeFocused(e)} @pointerdown=${(e) => this.treeDrag.down(e)}>
          <ul role="tree" aria-label="Collections">
            ${!s.visibleCollections.length
              ? html`<li class="tree-empty" role="none">
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
                <li role="none">
                  <div class="collection-line" role="none" @contextmenu=${(e) => this.openCollectionMenu(e, c)}>
                    <button type="button" class="tree-toggle" tabindex="-1" aria-hidden="true" title=${`Show or hide the requests in ${c.name}`}
                      ?disabled=${Boolean(filter)} @click=${() => this.toggleCollection(c.id)}>${icon(open ? "caret-down" : "caret-right")}</button>
                    <button type="button" class="tree-row collection" role="treeitem" tabindex="-1" aria-level="1" aria-expanded=${String(open)} aria-owns=${open ? `group-${c.id}` : nothing} data-tree="collection" data-drag="collection" data-key=${`c:${c.id}`} data-collection=${String(c.id)}
                      aria-current=${kind === "collection" && collectionId === c.id ? "page" : "false"} aria-describedby=${`storage-of-${c.id}`} @click=${() => this.go("collection", c.id)}>
                      ${this.storageIcon(c)}<span>${c.name}</span>
                    </button>
                    <span class="visually-hidden" id=${`storage-of-${c.id}`}>${storageOf(s, c).detail}</span>
                    <button type="button" class="row-action" tabindex="-1" aria-label=${`Add a request to ${c.name}`} title=${`Add a request to ${c.name}`} @click=${() => this.newRequestIn(c.id)}>${icon("plus")}</button>
                    <button type="button" class="row-action" tabindex="-1" aria-label=${`More for ${c.name}`} title="More (Shift+F10)" aria-haspopup="menu" @click=${(e) => this.openCollectionMenu(e, c, e.currentTarget)}>${icon("dots-three")}</button>
                  </div>
                  ${open
                    ? html`<ul role="group" id=${`group-${c.id}`}>
                        ${filter || !Object.keys(c.variables || {}).length
                          ? nothing
                          : html`<li role="none">
                              <button type="button" class="tree-row leaf" role="treeitem" tabindex="-1" aria-level="2" data-tree="variables" data-key=${`v:${c.id}`} data-collection=${String(c.id)}
                                aria-current=${kind === "variables" && collectionId === c.id ? "page" : "false"} @click=${() => this.go("variables", c.id)}>
                                ${icon("sliders-horizontal")}<span>Variables</span>
                              </button>
                            </li>`}
                        ${filter
                          ? requests.length
                            ? html`<li role="none">${this.requestList(c, requests.map((request) => ({ request, index: -1 })), `Requests in ${c.name}`, activeRequest, false)}</li>`
                            : nothing
                          : this.requestTreeRows(c, treeOf(c, requestTree), activeRequest)}
                      </ul>`
                    : nothing}
                </li>`;
            })}
          </ul>
        </nav>
        <p class="visually-hidden" aria-live="polite" data-tree-live>${this.treeLive ?? ""}</p>
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
    const steps = gettingStartedSteps({ history: s.history, collections: s.collections, runs, ranNow: Boolean(s.run.report), savedOnce: savedOnce(storage), imported: importedIds(storage), keys: { send: SEND_KEYS, save: SAVE_KEYS } });
    if (!showGettingStarted(storage, { fresh: s.isFresh, steps })) return nothing;
    const done = steps.filter((x) => x.done).length;
    const next = steps.find((x) => !x.done);
    // After the first step the card folds to one line (the next step), so it doesn't push the
    // collections down; a click opens it again.
    const folded = done > 0 && !this.gsOpen;
    const head = html`<div class="gs-head">
        <h2 id="getting-started-title">Get started</h2>
        <span class="mono gs-count" aria-label=${`${done} of ${steps.length} done`}>${done}/${steps.length}</span>
        ${done > 0 ? html`<button type="button" class="mini" aria-expanded=${String(!folded)} aria-label=${folded ? "Show the steps" : "Fold the steps"} title=${folded ? "Show the steps" : "Fold"} @click=${() => ((this.gsOpen = folded), this.requestUpdate())}>${icon(folded ? "caret-down" : "caret-up")}</button>` : nothing}
        <button type="button" class="mini" aria-label="Hide getting started" title="Hide" @click=${() => (hideGettingStarted(storage), this.requestUpdate())}>${icon("x")}</button>
      </div>`;
    if (folded) {
      return html`<section class="getting-started folded" aria-labelledby="getting-started-title">
        ${head}
        <p class="gs-next">${icon("caret-right")}<span>Next: ${next.label}</span></p>
      </section>`;
    }
    return html`<section class="getting-started" aria-labelledby="getting-started-title">
      ${head}
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
    // UX2: while Get started shows (after its first step) the storage card is one line.
    const guide = this.gettingStarted();
    const compact = guide !== nothing && s.history.length > 0;
    return html`<div class="side-pane-footer">
      ${guide}
      <div class=${`side-card ${compact ? "compact" : ""}`}>
        <div class="side-card-line">${icon(w?.kind === "folder" ? "lock" : (w?.icon ?? (IS_TAURI ? "desktop" : "browser")))}<span>${where}</span></div>
        <div class="side-card-meta mono">${place ? html`${place} · ` : nothing}${cols.length} ${cols.length === 1 ? "collection" : "collections"} · ${requests} ${requests === 1 ? "request" : "requests"}${changed ? ` · ${changed} changed` : ""}</div>
        <div class="side-card-actions">
          <button type="button" class="link-button" @click=${() => this.openDialog("import")}>${icon("file-arrow-up")}<span>Import</span></button>
          <button type="button" class="link-button" @click=${() => this.openDialog("export")}>${icon("file-arrow-down")}<span>Export</span></button>
        </div>
      </div>
    </div>`;
  }
  };
