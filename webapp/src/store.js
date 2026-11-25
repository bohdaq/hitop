// App state and every action, without UI. Started as a port of the old React
// app's logic onto its services (now shared/services/), so collections, tabs,
// history and contexts keep the localStorage keys and shapes data stored by the
// React extension and desktop UI still has. Views subscribe with `store.addEventListener("change")`
// and report problems through the `notify` event (never window.alert).
import tabService from "../../shared/services/tabService.js";
import { COOKIES_KEY, COOKIE_JAR_KEY, NO_RESPONSE, initialState, loadCookies, storage, storedTab, storedTabInDb } from "./store/shared.js";
import { emptyGlobals, normalizeGlobals } from "./features.js";
import { generateUniqueId } from "../../shared/services/idGenerator.js";
import { loadFormat } from "./wasm/format.js";
import { loadSettings } from "./settings.js";
import { platform } from "./platform/index.js";
import { CookiesPart } from "./store/cookies.js";
import { FormFieldsPart } from "./store/form-fields.js";
import { TabsPart } from "./store/tabs.js";
import { RequestsPart } from "./store/requests.js";
import { OrganizationsPart } from "./store/organizations.js";
import { CollectionsPart } from "./store/collections.js";
import { ImportExportPart } from "./store/import-export.js";
import { OpenapiPart } from "./store/openapi.js";
import { ComparePart } from "./store/compare.js";
import { EnvironmentsPart } from "./store/environments.js";
import { ScriptsPart } from "./store/scripts.js";
import { SendingPart } from "./store/sending.js";
import { WebsocketPart } from "./store/websocket.js";
import { GrpcPart } from "./store/grpc.js";
import { FoldersPart } from "./store/folders.js";

export { applyPathVariables, inheritDefaults, inheritedAuth, pathVariableNames, requestTimeout } from "./defaults.js";
export { isGraphql, isWebsocket, isGrpc, methodLabel, wsUrl, METHODS, BODY_METHODS, KEEP_BODY, KEEP_HEAD, storedTab, INLINE_BODY, STORED_HEAD, storedTabInDb } from "./store/shared.js";

// The parts by subject (src/store/); this file keeps the start, saving and views.
const Parts = [CookiesPart, FormFieldsPart, TabsPart, RequestsPart, OrganizationsPart, CollectionsPart, ImportExportPart, OpenapiPart, ComparePart, EnvironmentsPart, ScriptsPart, SendingPart, WebsocketPart, GrpcPart, FoldersPart].reduce((Base, part) => part(Base), EventTarget);

export class Store extends Parts {
  constructor() {
    super();
    const s = initialState();
    this.collections = s.collections;
    this.tabs = s.tabs;
    this.currentTab = s.currentTab;
    // FV1: nothing was stored (the sample and My requests were just made): the app focuses the URL.
    this.firstVisit = Boolean(s.firstVisit);
    this.history = storage().loadHistory() || [];
    this.contexts = storage().loadContexts() || {};
    const envs = storage().loadEnvironments();
    this.environments = envs.environments;
    this.activeEnvironmentId = this.environments.some((e) => e.id === envs.activeEnvironmentId) ? envs.activeEnvironmentId : null;
    // The globals: below collection variables and environments, for every collection.
    this.globals = normalizeGlobals(storage().loadGlobals?.() ?? emptyGlobals());
    // ${secret:vault|aws|azure|op:...} values (desktop only), by inner text:
    // memory only, for one send or run, masked like secrets, never saved.
    this.refValues = {};
    // Every value a reference resolved to since the page loaded (memory only): masking reads
    // this, not refValues, which the next send or run clears while others are still in flight.
    this.refSeen = new Set();
    // hitop-wasm, once loaded: the Headers note asks it for the Content-Type
    // (hitop-core's rules, as every surface sends it) while rendering.
    this.format = null;
    loadFormat().then((f) => { this.format = f; this.emit(); }, () => {});
    // Globals scripts set (pm.globals.set): app-wide (folder collections: their own), until reload.
    this.scriptGlobals = {};
    const jar = loadCookies();
    // The desktop keeps the jar in Rust (hitop-core), the web app in localStorage.
    this.cookies = platform().cookies ? platform().cookies.load() : jar.cookies;
    this.cookieJarEnabled = jar.enabled;
    // Files picked for multipart fields, by field id. Never stored: a page
    // reload asks for them again.
    this.files = new Map();
    // GraphQL schemas fetched by introspection, by tab id: { schema, types,
    // url, loading, error }. Memory only: fetched again after a reload.
    this.schemas = new Map();
    // WebSocket connections by tab id: { status: connecting | open | closed,
    // protocol, log: [{ time, kind: sent | received | system | error, text,
    // bytes? }], conn }. Memory only: the log holds filled-in values
    // (tokens), so it is never saved, and connections end with the page.
    this.sockets = new Map();
    // gRPC by tab id: { status: idle | running | done, log: [{ time, kind:
    // sent | headers | message | status | error | system, text, ok? }],
    // call, schema: { services, source } | null, schemaLoading,
    // schemaError }. Memory only, as the WebSocket log.
    this.grpcTabs = new Map();
    // What's wrong with a tab's URL, shown under the field until it changes (by tab id).
    this.urlErrors = new Map();
    // Response bodies as received, by tab id, where the tab shows them formatted (JSON).
    this.rawBodies = new Map();
    // U19: tab id → { text, missing } for a send stopped by a secret with no value here (memory only).
    this.notSent = new Map();
    // What the main area shows: the request tabs, all collections, one
    // collection, or one collection's variables.
    this.view = { kind: "request", collectionId: null };
    this.run = { collectionId: null, results: [], running: false, completed: 0, summary: null };
    // Collections that live in a folder: collection id → { ref, label,
    // connected, error, written }. Their environments are in
    // `environments` too, under ids from `folderEnvs` (env id → { collectionId, id }).
    this.folders = new Map();
    this.folderEnvs = new Map();
    // Variables scripts set (pm.environment.set, ...), by collection id: they
    // last until the page is reloaded, as in a CLI run. null unsets one.
    this.scriptVariables = {};
    platform().onError?.((message) => this.notify(message));
    addEventListener("pagehide", () => this.save());
    // Large response bodies across reloads (IndexedDB), null without it;
    // keptBodies: tab id → the body last written there.
    this.responseStore = platform().responses ?? null;
    // Settings (settings.js), and requests in flight by tab id (AbortController).
    this.settings = loadSettings();
    this.inflight = new Map();
    this.keptBodies = new Map();
    this.responsesRestored = this.restoreResponses();
    this.ready = this.restoreFolders();
    // HITOP Cloud organizations (signed in, main.js loadOrgs): the top bar's switcher. A collection
    // is in one org (`org`, else the personal org; browser-only and folder collections are always
    // personal, shared ones are in their workspace's org); the app shows the current org's only.
    // An environment is in one org (`org`) or every org (no `org`). Signed out: no orgs, no filter.
    this.orgs = [];
    this.personalOrg = null;
    this.currentOrg = null;
    this.workspaceOrgs = new Map();
    // U49: org id → `GET /v1/orgs/:org/usage` (orgs.js loadUsage); memory only.
    this.orgUsage = new Map();
  }

  // --- plumbing -----------------------------------------------------------
  emit() {
    // Views get the store by reference; this changing number re-renders them.
    this.version = (this.version ?? 0) + 1;
    // Saving serializes everything, responses included (cut at 256 KB), so it
    // runs at most every 250 ms rather than on each keystroke.
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => this.save(), 250);
    this.dispatchEvent(new Event("change"));
  }

  save() {
    clearTimeout(this.saveTimer);
    const st = storage();
    // Adapters return false when the browser refused a write (storage full).
    const failed = [];
    if (st.saveCollections(this.collections) === false) failed.push("collections");
    if (st.saveTabs(this.storedTabs()) === false) failed.push("tabs");
    st.saveCurrentTab(this.currentTab);
    if (st.saveHistory(this.history) === false) failed.push("history");
    if (st.saveContexts(this.contexts) === false) failed.push("script values");
    this.reportStorage(failed);
    st.saveGlobals?.(this.globals);
    const globalEnvs = this.environments.filter((e) => !this.folderEnvs.has(e.id));
    st.saveEnvironments(globalEnvs, globalEnvs.some((e) => e.id === this.activeEnvironmentId) ? this.activeEnvironmentId : null);
    this.syncFolders();
    if (platform().cookies) platform().cookies.save(this.cookies);
    try {
      if (!platform().cookies) localStorage.setItem(COOKIES_KEY, JSON.stringify(this.cookies));
      localStorage.setItem(COOKIE_JAR_KEY, this.cookieJarEnabled ? "on" : "off");
    } catch {
      // Storage full or blocked: the jar lasts for this page only.
    }
  }

  // The tabs as saved. With IndexedDB, large bodies are written there (when
  // they changed) and bodies of closed tabs go; without it, storedTab.
  storedTabs() {
    const db = this.responseStore;
    if (!db) return this.tabs.map(storedTab);
    const wanted = new Set();
    const tabs = this.tabs.map((t) => {
      // A tab without an id can't be found again after a reload: the 1.1 rule.
      if (t.id == null) return storedTab(t);
      const saved = storedTabInDb(t);
      if (saved === t) return t;
      wanted.add(t.id);
      if (this.keptBodies.get(t.id) !== t.response) {
        this.keptBodies.set(t.id, t.response);
        const raw = this.rawBodies.get(t.id);
        db.put(t.id, { body: t.response, ...(raw !== undefined ? { raw } : {}) }).catch((e) => this.responseStoreFailed(e));
      }
      return saved;
    });
    for (const id of [...this.keptBodies.keys()]) {
      if (wanted.has(id)) continue;
      this.keptBodies.delete(id);
      db.delete(id).catch(() => {});
    }
    return tabs;
  }

  // IndexedDB refused a body (full, or gone): say so once and save the 1.1
  // way from now on.
  responseStoreFailed(e) {
    if (!this.responseStore) return;
    console.error("Couldn't keep a response body:", e);
    this.responseStore = null;
    this.keptBodies.clear();
    this.notify("Couldn't keep a large response for after a reload: this browser's storage for HITOP is full. Responses over 256 KB are kept as their first 64 KB from now on.");
    this.save();
  }

  // After a reload: the whole bodies of tabs saved with `responseStored`, as
  // long as the tab still shows what was saved (not a newer response).
  // Bodies no tab refers to are deleted. A body that isn't there (site data
  // cleared, 1.1's cut tabs) leaves the part that was saved, with its note.
  async restoreResponses() {
    const db = this.responseStore;
    if (!db) return;
    const heads = new Map(this.tabs.filter((t) => t.responseStored && t.id != null).map((t) => [t.id, t.response]));
    for (const key of db.keys) if (!heads.has(key)) db.delete(key).catch(() => {});
    let changed = false;
    for (const [id, head] of heads) {
      if (!db.keys.includes(id)) continue;
      let kept;
      try {
        kept = await db.get(id);
      } catch {
        continue;
      }
      const at = this.tabs.findIndex((t) => t.id === id);
      if (!kept || typeof kept.body !== "string" || at === -1 || this.tabs[at].response !== head || !this.tabs[at].responseCut) continue;
      this.keptBodies.set(id, kept.body);
      if (typeof kept.raw === "string") this.rawBodies.set(id, kept.raw);
      this.tabs = tabService.updateTab(this.tabs, at, { response: kept.body, responseCut: false });
      changed = true;
    }
    if (changed) this.emit();
  }

  // Settings: every response kept for the tabs goes (IndexedDB and the saved
  // tabs); history keeps its own.
  async clearStoredResponses() {
    try {
      await this.responseStore?.clear();
    } catch {
      // gone already
    }
    this.keptBodies.clear();
    this.rawBodies.clear();
    this.tabs = this.tabs.map((t) => (t.loading ? t : { ...t, ...NO_RESPONSE }));
    this.emit();
    this.notify("Cleared the responses kept for your tabs");
  }

  // Says once that saving fails (and again after a save that worked), so a
  // full storage isn't silent: edits made meanwhile would be gone on reload.
  reportStorage(failed) {
    if (!failed.length) {
      this.storageFull = false;
      return;
    }
    if (this.storageFull) return;
    this.storageFull = true;
    const what = failed.length === 1 ? failed[0] : `${failed.slice(0, -1).join(", ")} and ${failed.at(-1)}`;
    this.notify(`Couldn't save your ${what}: this browser's storage for HITOP is full, so changes are lost on reload. Close tabs with large responses or clear History.`);
  }

  // --- views --------------------------------------------------------------
  show(kind, collectionId = null, environmentId = null) {
    this.view = { kind, collectionId, environmentId };
    if (kind === "collections" && !this.run.running) this.run = { ...this.run, collectionId: null, results: [] };
    this.emit();
  }
}

export { generateUniqueId };
