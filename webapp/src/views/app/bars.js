// A part of the app shell (HitopApp, ../hitop-app.js).
import { ACTION_KEYS } from "../../shortcuts.js";
import { IS_TAURI } from "../../platform/index.js";
import { PALETTE_HINTS, PREFIXES, actions as paletteActions, rankSections, storeItems } from "../../commands.js";
import { PREVIEW_CSS, showMenuFor } from "./shared.js";
import { UNTITLED, isBlankRequest } from "../../new-request.js";
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { isCurl, parseCurl } from "../../curl.js";
import { methodLabel } from "../../store.js";
import { protocolLabel } from "../../request-facts.js";

export const BarsPart = (Base) =>
  class extends Base {
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
    // UX2 (3): on a page (History, Settings…) the crumb names that page.
    const kind = s.view.kind;
    const page = c ? null : kind === "collections" ? "All collections" : kind === "sync" ? "Account" : this.railItems().find((it) => it.kind === kind && it.kind !== "collections")?.label ?? null;
    if (!c && !page && !cols.length) return nothing;
    const name = c?.name ?? page ?? "Collections";
    return html`<span class="crumb" aria-hidden="true">/</span>
      <button type="button" class="crumb crumb-name" title=${c ? `Collection: ${name}` : name} aria-haspopup="menu" aria-label=${c ? `Collection ${name}: switch` : `${name}: collections`}
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
    this.updateComplete.then(() => {
      const palette = this.querySelector("lit-material-command-palette");
      // UX2: a section whose item starts with what was typed comes first ("import" → the Import
      // action before a request that only fuzzy-matches it).
      if (palette && !palette.rankedSections) {
        const groups = palette.groups.bind(palette);
        palette.groups = () => rankSections(groups(), palette.parse().text);
        palette.rankedSections = true;
      }
      palette?.show();
    });
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
    if (added) this.saveCurrent();
    const note = parsed.warnings.length ? ` ${parsed.warnings.join(" ")}` : "";
    this.toast(`Imported from curl: ${parsed.method} ${parsed.url}.${note}`, {
      label: "Undo",
      run: () => {
        const at = s.tabs.findIndex((t) => t.id === id);
        if (added && at !== -1) s.deleteTabRequest(at);
        else undo?.();
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
      ${place ? html`<lit-material-status-bar-item class="mono status-place">${place}</lit-material-status-bar-item>` : nothing}
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
      <lit-material-status-bar-item slot="end" class="status-engine">${engine}</lit-material-status-bar-item>
      <lit-material-status-bar-item slot="end" class="status-shortcuts" button description="Keyboard shortcuts" @click=${() => this.openDialog("shortcuts")}><lit-material-kbd keys="?"></lit-material-kbd> Shortcuts</lit-material-status-bar-item>
    </lit-material-status-bar>`;
  }
  };
