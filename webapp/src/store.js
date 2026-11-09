// App state and every action, without UI. A port of frontend/src/App.jsx's
// logic onto the same services, so collections, tabs, history and contexts
// keep their localStorage keys and shapes: the React app and this one read
// each other's data. Views subscribe with `store.addEventListener("change")`
// and report problems through the `notify` event (never window.alert).
import collectionService from "../../frontend/src/services/collectionService.jsx";
import contextService from "../../frontend/src/services/contextService.jsx";
import historyService from "../../frontend/src/services/historyService.jsx";
import tabService from "../../frontend/src/services/tabService.jsx";
import { interpolateVariables } from "../../frontend/src/services/variableInterpolation.jsx";
import { generateUniqueId } from "../../frontend/src/utils/idGenerator.jsx";
import {
  applyAuth,
  awsV4Headers,
  digestAnswer,
  parseDigestChallenge,
  asksForEvents,
  parseEvents,
  oauthAuthorizeUrl,
  oauthCodeRequest,
  oauthParseToken,
  oauthTokenRequest,
  pkcePair,
  cleanAuth,
  cookieHeader,
  emptyAuth,
  emptyGlobals,
  exportableEnvironments,
  exportableGlobals,
  isPostmanGlobals,
  mergeGlobals,
  normalizeGlobals,
  parseGlobals,
  makeAssert,
  maskSecrets,
  mergeCookies,
  multipartBody,
  parseEnvironments,
  secretNames,
  secretValues,
  secretRefs,
  withSecretRefs,
  urlEncodedBody,
} from "./features.js";
import { platform } from "./platform/index.js";
import { fillVariables, forWebApp, isPostmanCollection, loadShim, runScript, scriptScans, usesPostmanApi } from "./pm.js";
import pmShimText from "../../shared/pm-shim.js?raw";
import { loadFormat } from "./wasm/format.js";
import { exampleResponse, snapshotResponse } from "./diff.js";
import { folderToZip, zipToFolder } from "./zip.js";
import { loadSettings, saveSettings, seconds } from "./settings.js";
import { addCollections } from "./collection-tree.js";

// Postman's pm.* API for scripts (the shim the CLI and desktop run too).
const pmShim = loadShim(pmShimText);

const storage = () => platform().storage;
const account = (workspace, env, name) => `${workspace}/${env}/${name}`;
const workspaceOf = (collection) => collection.workspace || String(collection.id);
// Secret globals go by this environment name (env var HITOP_SECRET_GLOBAL_<NAME>,
// keychain account <workspace>/global/<name>), as in the CLI.
const GLOBALS_ENV = "global";
// Ids the Rust model reads (whole numbers); old web data has strings.
const numericId = (id) => (Number.isSafeInteger(id) && id >= 0 ? id : generateUniqueId());
const errorText = (e) => (e && typeof e === "object" && "message" in e ? e.message : String(e));
// An environment as written to a file or export: secret values left out.
const withoutSecretValues = (e) => ({ ...e, variables: Object.fromEntries(Object.entries(e.variables || {}).filter(([k]) => !secretNames(e).includes(k))) });

const COOKIES_KEY = "hitop_cookies";
const COOKIE_JAR_KEY = "hitop_cookie_jar";
// The org picked in the top bar (store.currentOrg), when signed in to HITOP Cloud.
const ORG_KEY = "hitop_org";

function loadCookies() {
  try {
    return { cookies: JSON.parse(localStorage.getItem(COOKIES_KEY) || "[]"), enabled: localStorage.getItem(COOKIE_JAR_KEY) !== "off" };
  } catch {
    return { cookies: [], enabled: true };
  }
}

// A saved request sent as GraphQL (hitop-core's rule): bodyType graphql on
// a raw body. Its query is `body`, the rest is in `graphql`.
export const isGraphql = (r) => r?.bodyType === "graphql" && (!r.bodyMode || r.bodyMode === "raw");

// A saved request that opens a WebSocket connection (hitop-core's `kind`).
export const isWebsocket = (r) => r?.kind === "websocket";
// A saved request that makes a gRPC call (hitop-core's `kind`).
export const isGrpc = (r) => r?.kind === "grpc";
// What request lists show in the method's place.
export const methodLabel = (r) => (isWebsocket(r) ? "WS" : isGrpc(r) ? "GRPC" : r.method);

// Request fields the web app adds, as a tab carries them: a GraphQL request
// shows the body mode "graphql", with its variables and operation name; a
// WebSocket request its subprotocols and saved messages; a gRPC request its
// service, method, message and schema source.
const EXTRA_FIELDS = (r) => ({
  kind: isWebsocket(r) ? "websocket" : isGrpc(r) ? "grpc" : undefined,
  subprotocols: r.websocket?.subprotocols ?? [],
  wsMessages: r.websocket?.messages ?? [],
  grpcService: r.grpc?.service ?? "",
  grpcMethod: r.grpc?.method ?? "",
  grpcMessage: r.grpc?.message ?? "",
  grpcProtos: r.grpc?.protos ?? [],
  grpcImportPaths: r.grpc?.importPaths ?? [],
  grpcPlaintext: !!r.grpc?.plaintext,
  bodyMode: isGraphql(r) ? "graphql" : r.bodyMode || "raw",
  formFields: r.formFields || [],
  disabledParams: r.disabledParams || [],
  graphqlVariables: r.graphql?.variables ?? "",
  graphqlOperationName: r.graphql?.operationName ?? "",
});

// The same fields of a tab, as saved with the request: the "graphql" mode is
// a raw body with bodyType graphql. Variables and operation name are kept
// after a switch to another body type (as the form rows are), not sent.
const SAVED_FIELDS = (t) => {
  const graphql = Object.fromEntries(Object.entries({ variables: t.graphqlVariables, operationName: t.graphqlOperationName }).filter(([, v]) => v));
  return {
    bodyMode: t.bodyMode === "graphql" ? "raw" : t.bodyMode || "raw",
    formFields: t.formFields || [],
    disabledParams: t.disabledParams || [],
    graphql: Object.keys(graphql).length ? graphql : undefined,
    ...(t.bodyMode === "graphql" ? { bodyType: "graphql" } : {}),
    kind: t.kind === "websocket" || t.kind === "grpc" ? t.kind : undefined,
    websocket: websocketFields(t),
    grpc: grpcFields(t),
  };
};

// A tab's gRPC fields as saved (`grpc`), or undefined when it has none.
// Kept when the request switches to HTTP, as the WebSocket ones are.
function grpcFields(t) {
  const g = {
    service: t.grpcService || undefined,
    method: t.grpcMethod || undefined,
    message: t.grpcMessage || undefined,
    protos: (t.grpcProtos || []).filter(Boolean),
    importPaths: (t.grpcImportPaths || []).filter(Boolean),
    plaintext: t.grpcPlaintext || undefined,
  };
  if (!g.protos.length) delete g.protos;
  if (!g.importPaths.length) delete g.importPaths;
  for (const k of Object.keys(g)) if (g[k] === undefined) delete g[k];
  return Object.keys(g).length ? g : undefined;
}

// The log of a gRPC tab keeps this many entries.
const GRPC_LOG_MAX = 500;

// A tab's subprotocols and saved messages as saved (`websocket`), or
// undefined when it has neither. Kept when the request switches to HTTP.
function websocketFields(t) {
  const subprotocols = (t.subprotocols || []).filter(Boolean);
  const messages = t.wsMessages || [];
  if (!subprotocols.length && !messages.length) return undefined;
  return { ...(subprotocols.length ? { subprotocols } : {}), ...(messages.length ? { messages } : {}) };
}

// ws:// and wss:// as given; http(s):// read as ws(s)://, as hitop-core does.
export function wsUrl(url) {
  const m = /^(wss?|https?):\/\//i.exec(url);
  if (!m) throw new Error(`'${url}' is not a WebSocket URL: use ws:// or wss://`);
  const scheme = { ws: "ws", wss: "wss", http: "ws", https: "wss" }[m[1].toLowerCase()];
  return `${scheme}://${url.slice(m[0].length)}`;
}

// The log of a WebSocket tab keeps this many entries.
const WS_LOG_MAX = 500;

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
export const BODY_METHODS = ["POST", "PUT", "PATCH"];

const emptyRequest = () =>
  collectionService.createRequest("New request", {
    url: "",
    method: "GET",
    headers: [{ name: "", value: "" }],
    body: "",
    preRequestScript: "",
    postRequestScript: "",
  });

function tabForRequest(request, collectionId) {
  return {
    ...tabService.createNewTab(),
    title: request.name,
    url: request.url,
    method: request.method,
    headers: request.headers?.length ? request.headers : [{ name: "", value: "" }],
    requestBody: request.body || "",
    preRequestScript: request.preRequestScript || "",
    postRequestScript: request.postRequestScript || "",
    auth: request.auth ? { ...emptyAuth(), ...request.auth } : emptyAuth(),
    ...EXTRA_FIELDS(request),
    loadedRequestId: request.id,
    loadedCollectionId: collectionId,
  };
}

function initialState() {
  const collections = storage().loadCollections();
  const tabs = storage().loadTabs();
  const current = storage().loadCurrentTab();
  if (collections?.length && tabs?.length) {
    return {
      collections,
      // A request in flight when the page closed never finishes.
      tabs: tabs.map((t) => ({ ...t, loading: false })),
      currentTab: current !== null && current >= 0 && current < tabs.length ? current : 0,
    };
  }
  const request = emptyRequest();
  const fallback = collections?.length ? collections : [{ id: 1, name: "Default", requests: [request] }];
  const first = fallback[0];
  const target = first.requests?.[0] ?? request;
  return { collections: fallback, tabs: [tabForRequest(target, first.id)], currentTab: 0 };
}

// Turn a fetch-like response into what the response viewer shows.
async function readResponse(res) {
  const contentType = res.headers.get("content-type") || "";
  const text = await res.text();
  if (contentType.includes("json")) {
    try {
      return { data: JSON.stringify(JSON.parse(text), null, 2), type: "json", raw: text };
    } catch (e) {
      // Shown as text, with what the parser said (not silently).
      return { data: text, type: "text", jsonError: text.trim() ? e.message : null };
    }
  }
  if (contentType.includes("html")) return { data: text, type: "html" };
  if (contentType.includes("xml")) return { data: text, type: "xml" };
  return { data: text, type: "text" };
}

// A tab as saved without IndexedDB (the 1.1 behaviour): a response body over
// KEEP_BODY is cut to its first KEEP_HEAD characters (`responseCut`), so one
// large response can't fill the browser's storage (about 5 MB for
// everything) and stop every later save. The open tab keeps the whole body
// until the page is reloaded.
export const KEEP_BODY = 256 * 1024;
export const KEEP_HEAD = 64 * 1024;
export function storedTab(t) {
  if (typeof t.response !== "string" || t.response.length <= KEEP_BODY) return t;
  return { ...t, response: t.response.slice(0, KEEP_HEAD), responseCut: true, responseStored: undefined };
}

// With IndexedDB (platform().responses): a body over INLINE_BODY goes there
// whole, and the saved tab keeps its first STORED_HEAD characters with
// `responseCut` and `responseStored` (so the React app and 1.1 still read a
// body, with the note). After a reload the whole body comes back.
export const INLINE_BODY = 64 * 1024;
export const STORED_HEAD = 16 * 1024;
export function storedTabInDb(t) {
  if (typeof t.response !== "string" || t.response.length <= INLINE_BODY || t.id == null) return t;
  return { ...t, response: t.response.slice(0, STORED_HEAD), responseCut: true, responseStored: true };
}

// A tab's response fields when it has none (Settings: clear stored responses).
const NO_RESPONSE = { response: null, responseHeaders: null, statusCode: null, responseType: "", tests: null, durationMs: null, timings: null, truncated: false, responseCut: false, responseStored: undefined, jsonError: null };

// Drops keys set to undefined (a cleared bodyType), so they aren't saved.
const clean = (r) => Object.fromEntries(Object.entries(r).filter(([, v]) => v !== undefined));

const headersObject = (headers) => {
  const out = {};
  for (const h of headers || []) if (h.name && h.value) out[h.name] = h.value;
  return out;
};

// A draft's title follows its URL as typed: "GET api.example.com/users", so
// drafts can be told apart in the tab strip.
function autoTitle(tab) {
  if (tab.loadedRequestId) return tab.title;
  if (tab.kind === "grpc") return `GRPC ${tab.grpcMethod || tab.url || ""}`.trim();
  const verb = tab.kind === "websocket" ? "WS" : tab.method;
  const url = (tab.url || "").trim();
  if (!url) return "New request";
  let where = url;
  try {
    const u = new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(url) ? url : `https://${url}`);
    where = `${u.host}${u.pathname === "/" ? "" : u.pathname}`;
  } catch {
    // ${baseUrl}/users and the like: as typed
  }
  return `${verb} ${where.length > 48 ? `${where.slice(0, 47)}…` : where}`;
}

// What decides whether a tab differs from its saved request: the fields
// saveRequest writes, normalized (empty header rows and unused auth fields
// don't count).
function savedShape(tab) {
  return JSON.stringify(
    clean({
      url: tab.url || "",
      method: tab.method || "GET",
      headers: (tab.headers || []).filter((h) => h.name || h.value).map((h) => [h.name || "", h.value || "", h.enabled !== false]),
      body: tab.requestBody || "",
      preRequestScript: tab.preRequestScript || "",
      postRequestScript: tab.postRequestScript || "",
      auth: cleanAuth(tab.auth),
      ...SAVED_FIELDS(tab),
    }),
  );
}

export class Store extends EventTarget {
  constructor() {
    super();
    const s = initialState();
    this.collections = s.collections;
    this.tabs = s.tabs;
    this.currentTab = s.currentTab;
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

  // --- cookies -------------------------------------------------------------------

  setCookieJarEnabled(on) {
    this.cookieJarEnabled = on;
    this.emit();
  }

  setCookies(cookies) {
    this.cookies = cookies;
    this.emit();
  }

  // --- form fields --------------------------------------------------------------

  setFile(fieldId, file) {
    if (file) this.files.set(fieldId, file);
    else this.files.delete(fieldId);
    this.emit();
  }

  notify(message) {
    this.dispatchEvent(new CustomEvent("notify", { detail: message }));
  }

  get tab() {
    return this.tabs[this.currentTab];
  }

  collection(id) {
    return this.collections.find((c) => c.id === id);
  }

  defaultCollection() {
    let col = this.visibleCollections.find((c) => c.name === "Default");
    if (!col) {
      col = this.inOrg(collectionService.createCollection("Default"));
      this.collections = [...this.collections, col];
    }
    return col;
  }

  // --- views --------------------------------------------------------------

  show(kind, collectionId = null, environmentId = null) {
    this.view = { kind, collectionId, environmentId };
    if (kind === "collections" && !this.run.running) this.run = { ...this.run, collectionId: null, results: [] };
    this.emit();
  }

  // --- tabs ---------------------------------------------------------------

  updateTab(updates, index = this.currentTab) {
    if ("url" in updates && this.tabs[index]) this.urlErrors.delete(this.tabs[index].id);
    let tabs = tabService.updateTab(this.tabs, index, updates);
    const t = tabs[index];
    const title = autoTitle(t);
    if (title !== t.title) tabs = tabService.updateTab(tabs, index, { title });
    this.tabs = tabs;
    this.emit();
  }

  // Changes the tab with this id (wherever it is now) without a title change
  // or a save: for answers that arrive after the tabs may have moved.
  patchTab(id, updates) {
    const at = this.tabs.findIndex((t) => t.id === id);
    if (at !== -1) this.tabs = tabService.updateTab(this.tabs, at, updates);
  }

  // The tab with this id, which may no longer be the current one.
  updateTabById(id, updates) {
    const at = this.tabs.findIndex((t) => t.id === id);
    if (at !== -1) this.updateTab(updates, at);
  }

  selectTab(index) {
    this.currentTab = index;
    this.view = { kind: "request", collectionId: null };
    this.emit();
  }

  // A new, unsaved request: it uses the collection's variables but joins the
  // collection only when saved, so collections don't fill with blank rows.
  newTab(collection = this.defaultCollection()) {
    const tab = { ...tabService.createNewTab(), title: "New request", auth: emptyAuth(), ...EXTRA_FIELDS({}), loadedRequestId: null, loadedCollectionId: collection.id };
    this.tabs = [...this.tabs, tab];
    this.currentTab = this.tabs.length - 1;
    this.view = { kind: "request", collectionId: null };
    this.emit();
  }

  /** Moves a tab (the tab strip's drag or Alt+Shift+Arrow); the open tab stays open. */
  moveTab(from, to) {
    const n = this.tabs.length;
    if (from === to || from < 0 || to < 0 || from >= n || to >= n) return;
    const openId = this.tabs[this.currentTab]?.id;
    const tabs = [...this.tabs];
    const [moved] = tabs.splice(from, 1);
    tabs.splice(to, 0, moved);
    this.tabs = tabs;
    this.currentTab = Math.max(0, tabs.findIndex((t) => t.id === openId));
    this.emit();
  }

  closeTab(index) {
    if (this.tabs.length <= 1) return;
    this.cancel(this.tabs[index].id);
    this.wsDisconnect(this.tabs[index].id);
    this.grpcCancel(this.tabs[index].id);
    const { tabs, newCurrentTab } = tabService.closeTab(this.tabs, index, this.currentTab);
    this.tabs = tabs;
    this.currentTab = newCurrentTab;
    this.emit();
  }

  renameTab(index, name) {
    const title = name.trim();
    if (!title) return;
    const tab = this.tabs[index];
    if (tab.loadedRequestId && tab.loadedCollectionId) {
      const request = this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId);
      if (request) {
        this.collections = collectionService.updateRequestInCollection(this.collections, tab.loadedCollectionId, tab.loadedRequestId, { ...request, name: title });
      }
    }
    this.tabs = tabService.updateTab(this.tabs, index, { title });
    this.emit();
  }

  duplicateTab(index) {
    const source = this.tabs[index];
    const collectionId = source.loadedCollectionId ?? this.defaultCollection().id;
    const name = `${source.title} (Copy)`;
    // The saved request's fields the editor doesn't show (examples, unknown
    // keys) come along; its folder path doesn't.
    const saved = source.loadedRequestId && this.collection(source.loadedCollectionId)?.requests.find((r) => r.id === source.loadedRequestId);
    const { id: _id, name: _name, file: _file, ...kept } = saved || {};
    const request = collectionService.createRequest(name, {
      ...kept,
      url: source.url,
      method: source.method,
      headers: source.headers,
      body: source.requestBody,
      preRequestScript: source.preRequestScript,
      postRequestScript: source.postRequestScript,
      auth: cleanAuth(source.auth),
      ...SAVED_FIELDS(source),
    });
    if (!request.auth) delete request.auth;
    if (!request.graphql) delete request.graphql;
    if (!request.kind) delete request.kind;
    if (!request.websocket) delete request.websocket;
    if (!request.grpc) delete request.grpc;
    if (request.bodyType === "graphql" && source.bodyMode !== "graphql") delete request.bodyType;
    this.collections = collectionService.addRequestToCollection(this.collections, collectionId, request);
    this.tabs = [...this.tabs, tabForRequest(request, collectionId)];
    this.currentTab = this.tabs.length - 1;
    this.emit();
  }

  deleteTabRequest(index) {
    const tab = this.tabs[index];
    this.cancel(tab.id);
    this.wsDisconnect(tab.id);
    this.grpcCancel(tab.id);
    if (tab.loadedRequestId && tab.loadedCollectionId) {
      this.collections = collectionService.deleteRequestFromCollection(this.collections, tab.loadedCollectionId, tab.loadedRequestId);
    }
    if (this.tabs.length > 1) {
      const { tabs, newCurrentTab } = tabService.closeTab(this.tabs, index, this.currentTab);
      this.tabs = tabs;
      this.currentTab = newCurrentTab;
    } else {
      this.tabs = [{ ...tabService.createNewTab(), title: "New request", auth: emptyAuth(), ...EXTRA_FIELDS({}) }];
      this.currentTab = 0;
    }
    this.emit();
  }

  // --- requests in collections --------------------------------------------

  openRequest(request, collectionId) {
    this.view = { kind: "request", collectionId: null };
    const existing = this.tabs.findIndex((t) => t.loadedRequestId === request.id && t.loadedCollectionId === collectionId);
    if (existing !== -1) {
      this.currentTab = existing;
    } else {
      this.tabs = [...this.tabs, tabForRequest(request, collectionId)];
      this.currentTab = this.tabs.length - 1;
    }
    this.emit();
  }

  newRequestInFirstCollection() {
    this.newTab(this.collections[0] ?? this.defaultCollection());
  }

  // The save dialog's starting values: the loaded request, or a new one.
  saveTarget() {
    const tab = this.tab;
    const request = tab.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId);
    return request
      ? { name: request.name, collectionId: tab.loadedCollectionId, overwrite: true }
      : { name: "", collectionId: (this.collection(tab.loadedCollectionId) ?? this.collections.find((c) => c.name === "Default") ?? this.collections[0])?.id ?? null, overwrite: false };
  }

  saveRequest(name, collectionId) {
    const title = name.trim();
    const tab = this.tab;
    if (!title || !collectionId) return;
    const data = {
      name: title,
      url: tab.url,
      method: tab.method,
      headers: tab.headers,
      body: tab.requestBody,
      preRequestScript: tab.preRequestScript,
      postRequestScript: tab.postRequestScript,
      auth: cleanAuth(tab.auth),
      ...SAVED_FIELDS(tab),
    };
    const existing = tab.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId);
    // A body type label from a folder is kept only while the body is the
    // same; graphql is the body mode the editor shows, so it goes with it.
    if (tab.bodyMode !== "graphql" && existing?.bodyType && (existing.body !== data.body || existing.bodyType === "graphql")) data.bodyType = undefined;
    if (existing) {
      if (collectionId !== tab.loadedCollectionId) {
        // Moving keeps every field the editor doesn't know (examples, file,
        // keys from other tools); the folder path is the new collection's to give.
        const { file: _file, ...rest } = existing;
        let cols = collectionService.deleteRequestFromCollection(this.collections, tab.loadedCollectionId, tab.loadedRequestId);
        cols = collectionService.addRequestToCollection(cols, collectionId, clean({ ...rest, ...data, id: tab.loadedRequestId }));
        this.collections = cols;
      } else {
        this.collections = collectionService.updateRequestInCollection(this.collections, collectionId, tab.loadedRequestId, data).map((c) =>
          c.id === collectionId ? { ...c, requests: c.requests.map((r) => (r.id === tab.loadedRequestId ? clean(r) : r)) } : c,
        );
      }
      this.tabs = tabService.updateTab(this.tabs, this.currentTab, { loadedCollectionId: collectionId, title });
    } else {
      const request = clean(collectionService.createRequest(title, data));
      this.collections = collectionService.addRequestToCollection(this.collections, collectionId, request);
      this.tabs = tabService.updateTab(this.tabs, this.currentTab, { loadedRequestId: request.id, loadedCollectionId: collectionId, title });
    }
    this.emit();
    this.notify(`Saved "${title}" to ${this.collection(collectionId)?.name ?? "the collection"}`);
  }

  // The sidebar's request menu: rename (open tabs take the new title) and
  // duplicate (right after it, in the same folder under a file name of its own).
  renameRequest(collectionId, requestId, name) {
    const title = name.trim();
    const request = this.collection(collectionId)?.requests.find((r) => r.id === requestId);
    if (!title || !request) return;
    this.collections = collectionService.updateRequestInCollection(this.collections, collectionId, requestId, { ...request, name: title });
    this.tabs = this.tabs.map((t) => (t.loadedRequestId === requestId && t.loadedCollectionId === collectionId ? { ...t, title } : t));
    this.emit();
  }

  duplicateRequest(collectionId, requestId) {
    const col = this.collection(collectionId);
    const at = col?.requests.findIndex((r) => r.id === requestId) ?? -1;
    if (at === -1) return;
    const { id: _id, file, ...kept } = col.requests[at];
    const copy = { ...kept, id: generateUniqueId(), name: `${kept.name} (Copy)` };
    if (typeof file === "string" && file) {
      const taken = new Set(col.requests.map((r) => r.file));
      let n = 1;
      let name = `${file}-copy`;
      while (taken.has(name)) name = `${file}-copy-${++n}`;
      copy.file = name;
    }
    this.collections = this.collections.map((c) => (c.id === collectionId ? { ...c, requests: [...c.requests.slice(0, at + 1), copy, ...c.requests.slice(at + 1)] } : c));
    this.openRequest(copy, collectionId);
  }

  deleteRequest(request, collectionId) {
    this.collections = collectionService.deleteRequestFromCollection(this.collections, collectionId, request.id);
    this.tabs = this.tabs.map((t) => (t.loadedRequestId === request.id ? { ...t, loadedRequestId: null, loadedCollectionId: null } : t));
    this.emit();
  }

  moveRequest(collectionId, from, to) {
    if (from === to) return;
    this.collections = this.collections.map((c) => {
      if (c.id !== collectionId) return c;
      const requests = [...c.requests];
      requests.splice(to, 0, ...requests.splice(from, 1));
      return { ...c, requests };
    });
    this.emit();
  }

  // --- organizations ----------------------------------------------------------

  /** Signed in: the account's orgs (`GET /v1/orgs`) and its shared workspaces' orgs. */
  setOrgs(orgs, workspaceOrgs = new Map()) {
    this.orgs = orgs ?? [];
    this.personalOrg = this.orgs.find((o) => o.personal)?.id ?? this.orgs[0]?.id ?? null;
    this.workspaceOrgs = workspaceOrgs;
    let saved = null;
    try {
      saved = localStorage.getItem(ORG_KEY);
    } catch {
      // storage blocked: the personal org
    }
    this.currentOrg = this.orgs.some((o) => o.id === saved) ? saved : this.personalOrg;
    this.leaveHidden();
    this.emit();
  }

  clearOrgs() {
    if (!this.orgs.length && !this.currentOrg) return;
    this.orgs = [];
    this.personalOrg = null;
    this.currentOrg = null;
    this.workspaceOrgs = new Map();
    this.emit();
  }

  setCurrentOrg(id) {
    if (!this.orgs.some((o) => o.id === id)) return;
    this.currentOrg = id;
    try {
      localStorage.setItem(ORG_KEY, id);
    } catch {
      // storage blocked: for this page only
    }
    this.leaveHidden();
    this.emit();
  }

  get currentOrgInfo() {
    return this.orgs.find((o) => o.id === this.currentOrg) ?? null;
  }

  // A collection page of another org: back to the collections list.
  leaveHidden() {
    const id = this.view.collectionId;
    if (id != null && this.currentOrg) {
      const c = this.collection(id);
      if (c && !this.inCurrentOrg(c)) this.view = { kind: "collections", collectionId: null };
    }
  }

  /** The org `c` is in (null signed out). */
  orgOf(c) {
    if (!this.personalOrg) return null;
    // An org that isn't one of mine (a collection someone shared from their own org): personal.
    const mine = (org) => (org && this.orgs.some((o) => o.id === org) ? org : this.personalOrg);
    const folder = this.folders.get(c.id);
    if (folder) {
      const ref = String(folder.ref ?? "");
      return ref.startsWith("hitop-sync:") ? mine(this.workspaceOrgs.get(this.syncClient?.link?.(c.id)?.workspace)) : this.personalOrg;
    }
    const lib = this.library;
    if (!lib?.status?.on || (lib.state?.localOnly ?? []).includes(String(c.id))) return this.personalOrg;
    return mine(c.org);
  }

  inCurrentOrg(c) {
    return !this.currentOrg || this.orgOf(c) === this.currentOrg;
  }

  /** The collections the app shows: the current org's (all of them signed out). */
  get visibleCollections() {
    return this.currentOrg ? this.collections.filter((c) => this.inCurrentOrg(c)) : this.collections;
  }

  envInCurrentOrg(e) {
    if (!this.currentOrg) return true;
    const f = this.folderEnvs.get(e.id);
    if (f) {
      const c = this.collection(f.collectionId);
      return !c || this.inCurrentOrg(c);
    }
    return !e.org || e.org === this.currentOrg;
  }

  get visibleEnvironments() {
    return this.environments.filter((e) => this.envInCurrentOrg(e));
  }

  // A new collection goes into the current org (the personal org needs no tag).
  inOrg(col) {
    return this.currentOrg && this.currentOrg !== this.personalOrg ? { ...col, org: this.currentOrg } : col;
  }

  /** An environment for every org (true) or the current org only. */
  setEnvironmentAllOrgs(id, all) {
    this.environments = this.environments.map((e) => {
      if (e.id !== id) return e;
      const { org: _org, ...rest } = e;
      return all || !this.currentOrg ? rest : { ...rest, org: this.currentOrg };
    });
    this.emit();
  }

  // --- collections ----------------------------------------------------------

  addCollection(name) {
    if (!name.trim()) return;
    this.collections = [...this.collections, this.inOrg(collectionService.createCollection(name.trim()))];
    this.emit();
  }

  // A first collection to try: JSONPlaceholder (public, through the proxy on the web), with a
  // variable, a script that passes a value on, and tests.
  addSampleCollection() {
    const req = (name, method, url, extra = {}) => collectionService.createRequest(name, { url, method, headers: [{ name: "", value: "" }], body: "", preRequestScript: "", postRequestScript: "", ...extra });
    const col = this.inOrg({
      ...collectionService.createCollection("Sample: JSONPlaceholder"),
      variables: { baseUrl: "https://jsonplaceholder.typicode.com" },
      requests: [
        req("List posts", "GET", "${baseUrl}/posts?_limit=3", {
          postRequestScript: 'pm.test("status is 200", () => pm.response.to.have.status(200));\npm.test("three posts", () => pm.expect(pm.response.json().length).to.eql(3));\n// The next request reads it as ${postId}.\npm.collectionVariables.set("postId", pm.response.json()[0].id);',
        }),
        req("Get the first post", "GET", "${baseUrl}/posts/${postId}", {
          postRequestScript: 'pm.test("same post", () => pm.expect(pm.response.json().id).to.eql(Number(pm.collectionVariables.get("postId"))));',
        }),
        req("Create a post", "POST", "${baseUrl}/posts", {
          headers: [{ name: "Content-Type", value: "application/json" }],
          body: '{\n  "title": "Hello from HITOP",\n  "body": "A sample post",\n  "userId": 1\n}',
          postRequestScript: 'pm.test("created", () => pm.response.to.have.status(201));',
        }),
      ],
    });
    this.collections = [...this.collections, col];
    this.view = { kind: "collection", collectionId: col.id };
    this.emit();
    return col;
  }

  // Nothing of one's own yet: no request with a URL in any shown collection, no history.
  get isFresh() {
    return !this.history.length && this.visibleCollections.every((c) => (c.requests || []).every((r) => !r.url));
  }

  renameCollection(id, name) {
    if (!name.trim()) return;
    this.collections = collectionService.renameCollection(this.collections, id, name.trim());
    this.emit();
  }

  duplicateCollection(id) {
    this.collections = collectionService.duplicateCollection(this.collections, id);
    this.emit();
  }

  deleteCollection(id) {
    if (this.folders.has(id)) this.closeFolder(id);
    this.collections = collectionService.deleteCollection(this.collections, id);
    this.tabs = this.tabs.map((t) => (t.loadedCollectionId === id ? { ...t, loadedRequestId: null, loadedCollectionId: null } : t));
    if (this.view.collectionId === id) this.view = { kind: "collections", collectionId: null };
    this.emit();
  }

  // `from` and `to` are places in the shown list (visibleCollections).
  reorderCollections(from, to) {
    if (from === to) return;
    const shown = this.visibleCollections;
    const at = (i) => this.collections.indexOf(shown[i]);
    if (at(from) < 0 || at(to) < 0) return;
    this.collections = collectionService.reorderCollections(this.collections, at(from), at(to));
    this.emit();
  }

  setVariables(id, variables) {
    this.collections = collectionService.updateCollectionVariables(this.collections, id, variables);
    this.emit();
  }

  // --- import, export, history --------------------------------------------

  exportJson(format) {
    return collectionService.exportCollections(this.collections, format);
  }

  // Returns the import report for a Postman collection (what hitop couldn't
  // carry over, unsupported pm.* calls by script line), else null. Imported
  // collections are added to the ones there are (a name in use gets " (2)"),
  // or with `replace` take their place.
  async importJson(json, { replace = false } = {}) {
    let imported;
    let report = null;
    const parsed = JSON.parse(json.replace(/^\uFEFF/, "")); // throws on bad JSON
    if (isPostmanCollection(parsed)) {
      // hitop-core's importer (hitop-wasm), as `hitop import` runs it.
      const out = (await loadFormat()).importPostman(json, scriptScans(pmShim, parsed));
      imported = [forWebApp(out.collection)];
      report = { name: out.collection.name, issues: out.report.issues || [] };
    } else {
      imported = collectionService.importCollections(json);
    }
    // The desktop keeps ids as numbers (Rust's model); give string ids new ones.
    if (platform().name === "tauri") imported = imported.map((c) => ({ ...c, id: numericId(c.id), requests: (c.requests || []).map((r) => ({ ...r, id: numericId(r.id) })) }));
    if (!replace) {
      const { added, renamed } = addCollections(this.collections, imported, generateUniqueId);
      this.collections = [...this.collections, ...added.map((c) => this.inOrg(c))];
      const first = added.find((c) => c.requests?.length);
      if (first) this.openRequest(first.requests[0], first.id);
      else this.show("collections");
      const names = added.map((c) => c.name).join(", ");
      this.notify(`Added ${added.length} collection${added.length === 1 ? "" : "s"}: ${names}${renamed.length ? ` (renamed: you already have ${renamed.map(([from]) => from).join(", ")})` : ""}`);
      return report;
    }
    this.closeAllFolders();
    this.collections = imported;
    const first = imported.find((c) => c.requests?.length);
    this.tabs = first ? [tabForRequest(first.requests[0], first.id)] : [tabService.createNewTab()];
    this.currentTab = 0;
    this.view = { kind: "request", collectionId: null };
    this.emit();
    this.notify(`Imported ${imported.length} collection${imported.length === 1 ? "" : "s"}`);
    return report;
  }

  // --- OpenAPI -------------------------------------------------------------

  // A spec from a URL, through the same transport as Send (the proxy for
  // public hosts on the web, Rust on the desktop).
  async fetchSpec(url) {
    const res = await platform().send(url, { method: "GET", headers: { Accept: "application/yaml, application/json;q=0.9, */*;q=0.8" } });
    const text = await res.text();
    if (!res.ok) throw new Error(`the server answered ${res.status}${text ? `: ${text.slice(0, 160)}` : ""}`);
    return text;
  }

  // True when the text is an OpenAPI (or Swagger) document.
  async isOpenapi(text) {
    return (await loadFormat()).isOpenapi(text);
  }

  // Adds an OpenAPI spec (text, or fetched from `url`) as a new collection,
  // through hitop-core's importer (hitop-wasm), as `hitop import openapi`
  // does. Returns the report: { name, mapped, skipped, warnings }.
  async importOpenapi({ text = "", url = "" } = {}) {
    if (url) text = await this.fetchSpec(url);
    const out = (await loadFormat()).importOpenapi(text, { source: url || null, inlineAuth: true });
    const col = this.inOrg(out.collection);
    if (this.collection(col.id)) col.id = generateUniqueId();
    this.collections = [...this.collections, col];
    this.view = { kind: "collection", collectionId: col.id };
    this.emit();
    this.notify(`Imported ${col.name} (${col.requests.length} request${col.requests.length === 1 ? "" : "s"})`);
    return { name: col.name, ...out.report };
  }

  // What a sync with a newer spec would do: { collection, diff, report, text }.
  // Nothing changes until applyOpenapiSync. Request ids go to Rust as their
  // positions (web ids may be strings) and come back as they were.
  async previewOpenapiSync(collectionId, { text = "", url = "" } = {}, prune = false) {
    const col = this.collection(collectionId);
    if (!col) throw new Error("the collection is gone");
    if (url) text = await this.fetchSpec(url);
    const ids = col.requests.map((r) => r.id);
    const sent = { ...col, id: 0, requests: col.requests.map((r, i) => ({ ...r, id: i })) };
    const out = (await loadFormat()).syncOpenapi(sent, text, url ? { source: url, inlineAuth: true } : { inlineAuth: true }, prune);
    // Added requests have ids derived from their files (53-bit hashes).
    const requests = out.collection.requests.map((r) => (r.id < ids.length ? { ...r, id: ids[r.id] } : r));
    return { ...out, collection: { ...out.collection, id: col.id, requests }, text };
  }

  applyOpenapiSync(collectionId, preview) {
    const keep = new Set(preview.collection.requests.map((r) => r.id));
    this.collections = this.collections.map((c) => (c.id === collectionId ? preview.collection : c));
    this.tabs = this.tabs.map((t) => (t.loadedCollectionId === collectionId && !keep.has(t.loadedRequestId) ? { ...t, loadedRequestId: null, loadedCollectionId: null } : t));
    this.emit();
    const d = preview.diff;
    this.notify(`Synced ${preview.collection.name}: ${d.added.length} added, ${d.changed.length} changed, ${d.removed.length} removed${d.pruned && d.removed.length ? " (deleted)" : ""}`);
  }

  openHistoryItem(item) {
    this.view = { kind: "request", collectionId: null };
    this.tabs = tabService.updateTab(this.tabs, this.currentTab, {
      url: item.url,
      method: item.method,
      headers: item.headers?.length ? item.headers : [{ name: "", value: "" }],
      requestBody: item.body || "",
      response: null,
      responseHeaders: null,
      statusCode: null,
      responseType: "",
      loadedRequestId: null,
      loadedCollectionId: null,
      preRequestScript: item.preRequestScript || "",
      postRequestScript: item.postRequestScript || "",
      auth: item.auth ? { ...emptyAuth(), ...item.auth } : emptyAuth(),
      title: item.isCollectionRun ? `${item.collectionName} → ${item.requestName}` : `${item.method} ${item.url}`,
    });
    this.emit();
  }

  clearHistory() {
    this.history = [];
    this.emit();
  }

  deleteHistoryItem(item) {
    this.history = this.history.filter((h) => h !== item);
    this.emit();
  }

  // --- response diff, examples and code snippets ------------------------------

  // Secret names and values a request in this collection sees: the active
  // environment's and those of the globals it uses.
  secretNamesFor(collectionId) {
    const own = this.folders.get(collectionId)?.globals ?? this.globals;
    return [...new Set([...secretNames(this.activeEnvironment), ...secretNames(own)])];
  }

  secretValuesFor(collectionId) {
    const own = this.folders.get(collectionId)?.globals ?? this.globals;
    return [...secretValues(this.activeEnvironment), ...secretValues(own), ...Object.values(this.sharedFor(collectionId)), ...[...this.refSeen]];
  }

  // HITOP Cloud C4: secret values shared end to end in a synced collection's workspace (`hitop
  // sync secrets share`), by environment name. Memory only, never saved; they fill a secret of the
  // active environment only where it has no value of its own here.
  setSharedValues(collectionId, byEnvironment) {
    this.sharedValues = { ...(this.sharedValues ?? {}), [collectionId]: byEnvironment ?? {} };
    this.emit();
  }

  sharedFor(collectionId) {
    const env = this.activeEnvironment;
    if (!env) return {};
    const shared = this.sharedValues?.[collectionId]?.[env.name] ?? {};
    return Object.fromEntries(secretNames(env).filter((n) => typeof shared[n] === "string" && !(typeof env.variables?.[n] === "string" && env.variables[n] !== "")).map((n) => [n, shared[n]]));
  }

  // Resolves the ${secret:...} references a request (not its scripts) and the
  // variables it sees use, through the desktop (hitop-core's secret managers).
  // The browser can't reach them: a request that uses one fails with a note.
  async resolveRefs(source, collectionId) {
    const wanted = [...new Set([...secretRefs(source), ...secretRefs(Object.values(this.variables(collectionId)))])].filter((r) => !(r in this.refValues));
    if (!wanted.length) return;
    const p = platform();
    const list = wanted.map((r) => `\${${r}}`).join(", ");
    if (!p.resolveSecretRefs) throw new Error(`Secret manager references resolve in the hitop desktop app and the CLI, not in the browser: ${list}`);
    const values = await p.resolveSecretRefs(wanted);
    this.refValues = { ...this.refValues, ...values };
    for (const v of Object.values(values ?? {})) if (typeof v === "string" && v) this.refSeen.add(v);
  }

  // True when the tab has edits its saved request doesn't (computed, never
  // stored). Cached per tab and request object: both are replaced on change.
  isDirty(tab = this.tab) {
    const request = this.loadedRequest(tab);
    if (!request) return false;
    this.dirtyCache ??= new WeakMap();
    const hit = this.dirtyCache.get(tab);
    if (hit?.request === request) return hit.dirty;
    const dirty = savedShape(tab) !== savedShape(tabForRequest(request, tab.loadedCollectionId));
    this.dirtyCache.set(tab, { request, dirty });
    return dirty;
  }

  isDirtyRequest(requestId, collectionId) {
    const tab = this.tabs.find((t) => t.loadedRequestId === requestId && t.loadedCollectionId === collectionId);
    return tab ? this.isDirty(tab) : false;
  }

  // An unsaved draft with something in it (closing it loses that).
  hasDraftContent(tab) {
    return !tab.loadedRequestId && Boolean(tab.url?.trim() || tab.requestBody?.trim() || tab.preRequestScript?.trim() || tab.postRequestScript?.trim());
  }

  // What importing in place of the collections would delete: [{ name, requests }],
  // empty when there is nothing to lose (the first-run Default with its blank request).
  collectionsAtStake() {
    const filled = (r) => Boolean(r.url?.trim() || r.body?.trim() || r.preRequestScript?.trim() || r.postRequestScript?.trim() || r.kind);
    const stake = this.collections.map((c) => ({ name: c.name, requests: (c.requests || []).length, filled: (c.requests || []).some(filled) || Object.keys(c.variables || {}).length > 0 }));
    return this.collections.length > 1 || stake.some((c) => c.filled) ? stake : [];
  }

  loadedRequest(tab = this.tab) {
    return (tab?.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId)) || null;
  }

  // Saves the current tab's response as an example of its saved request
  // (`examples` in the folder format), secret values masked.
  saveExample(name) {
    const tab = this.tab;
    const request = this.loadedRequest(tab);
    if (!request || tab.statusCode == null || tab.responseType === "error") return false;
    const example = maskSecrets(
      { name: name.trim() || `${tab.statusCode} response`, status: tab.statusCode, headers: { ...(tab.responseHeaders || {}) }, body: tab.response ?? "" },
      this.secretValuesFor(tab.loadedCollectionId),
    );
    this.collections = this.collections.map((c) =>
      c.id === tab.loadedCollectionId ? { ...c, requests: c.requests.map((r) => (r.id === request.id ? { ...r, examples: [...(r.examples || []), example] } : r)) } : c,
    );
    this.emit();
    this.notify(`Saved example "${example.name}"`);
    return true;
  }

  // What the compare dialog can pick from: the current response, the loaded
  // request's examples, then history items that kept their response (items
  // stored before responses were kept have none and are left out).
  compareSources() {
    const out = [];
    const tab = this.tab;
    if (tab?.statusCode != null && tab.responseType !== "error") {
      // Masked like the stored ones, so a secret never shows up as a difference.
      const response = maskSecrets(snapshotResponse({ status: tab.statusCode, headers: tab.responseHeaders, body: tab.response, durationMs: tab.durationMs }), this.secretValuesFor(tab.loadedCollectionId));
      out.push({ id: "current", label: "Current response", response, request: `${tab.method} ${tab.url}` });
    }
    (this.loadedRequest(tab)?.examples || []).forEach((e, i) => out.push({ id: `example:${i}`, label: `Example: ${e?.name ?? i + 1}`, response: exampleResponse(e) }));
    for (const h of [...this.history].reverse()) {
      if (!h?.response) continue;
      const when = new Date(h.timestamp).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
      const what = h.isCollectionRun ? `${h.collectionName} → ${h.requestName}` : `${h.method} ${h.url}`;
      out.push({ id: `history:${h.id}`, label: `${h.response.status ?? "?"} ${what} · ${when}`, response: h.response, request: h.isCollectionRun ? null : `${h.method} ${h.url}` });
    }
    return out;
  }

  // The current tab as hitop-wasm's snippet input. Secrets stay {{NAME}}
  // placeholders unless `showSecrets`.
  // The Content-Type dispatch adds to this tab's raw body (none set, not
  // GraphQL or a form), for the Headers note and the Code dialog.
  autoContentType(tab = this.tab) {
    if (!tab || tab.kind || !BODY_METHODS.includes(tab.method) || (tab.bodyMode && tab.bodyMode !== "raw")) return null;
    if ((tab.headers || []).some((h) => h.name?.toLowerCase() === "content-type" && h.value)) return null;
    return this.format?.rawContentType(tab.requestBody) ?? null;
  }

  snippetInput(showSecrets = false) {
    const tab = this.tab;
    const request = {
      id: 0,
      name: tab.title || "",
      url: tab.url || "",
      method: tab.method || "GET",
      // As sent: headers with no value are left out.
      headers: (tab.headers || []).filter((h) => h.name && h.value).map((h) => ({ name: h.name, value: h.value, ...(h.enabled === false ? { enabled: false } : {}) })),
      body: BODY_METHODS.includes(tab.method) ? tab.requestBody || "" : "",
      ...SAVED_FIELDS(tab),
    };
    // hitop-core's generator adds the Content-Type and scheme hitop sends with.
    const auth = cleanAuth(tab.auth);
    if (auth) request.auth = auth;
    // The web app sends a body only with these methods.
    if (!BODY_METHODS.includes(tab.method)) Object.assign(request, { bodyMode: "raw", bodyType: undefined });
    // ${secret:...} references stay placeholders (as `hitop snippet` keeps them
    // without --show-secrets): the page never puts their values in code.
    const variables = this.variables(tab.loadedCollectionId, false);
    const refs = [...secretRefs([request, Object.values(variables)])];
    for (const [k, v] of Object.entries(variables)) if (typeof v === "string" && v.includes("${secret:")) variables[k] = v.replace(/\$\{(secret:[^}]+)\}/g, "{{$1}}");
    return { request, variables, secrets: [...(showSecrets ? [] : this.secretNamesFor(tab.loadedCollectionId)), ...refs] };
  }

  // { code, placeholders } from hitop-core's generator.
  async snippet(lang, showSecrets = false) {
    return (await loadFormat()).snippet(this.snippetInput(showSecrets), lang);
  }

  // --- environments -----------------------------------------------------------

  get activeEnvironment() {
    const env = this.environments.find((e) => e.id === this.activeEnvironmentId) ?? null;
    // Another org's environment isn't used here.
    return env && this.envInCurrentOrg(env) ? env : null;
  }

  // The globals a collection sees: a folder's own globals.yaml when it has
  // one, else the app's; with what scripts set on top.
  globalsKey(collectionId) {
    return this.folders.get(collectionId)?.globals ? `folder:${collectionId}` : "app";
  }

  globalsFor(collectionId) {
    const own = this.folders.get(collectionId)?.globals;
    const out = { ...(own ?? this.globals).variables };
    for (const [k, v] of Object.entries(this.scriptGlobals[this.globalsKey(collectionId)] ?? {})) {
      if (v === null) delete out[k];
      else out[k] = v;
    }
    return out;
  }

  setScriptGlobals(collectionId, updates) {
    const key = this.globalsKey(collectionId);
    if (updates && Object.keys(updates).length) this.scriptGlobals[key] = { ...this.scriptGlobals[key], ...updates };
  }

  setGlobals(variables, secrets) {
    const names = (secrets ?? this.globals.secrets).filter((n) => n in variables);
    this.globals = { variables, secrets: names };
    this.emit();
  }

  // Postman globals or HITOP's globals export: added to the globals.
  importGlobals(json) {
    const incoming = parseGlobals(json); // throws on bad input
    this.globals = mergeGlobals(this.globals, incoming);
    this.emit();
    const n = Object.keys(incoming.variables).length;
    this.notify(`Imported ${n} global${n === 1 ? "" : "s"}`);
  }

  exportGlobals() {
    return JSON.stringify(exportableGlobals(this.globals), null, 2);
  }

  // The variables a request sees, later ones winning: the globals, its
  // collection's, the active environment's, then what scripts set.
  variables(collectionId, withRefs = true) {
    const out = { ...this.globalsFor(collectionId), ...collectionService.getCollectionVariables(this.collections, collectionId), ...(this.activeEnvironment?.variables ?? {}), ...this.sharedFor(collectionId) };
    for (const [k, v] of Object.entries(this.scriptVariables[collectionId] ?? {})) {
      if (v === null) delete out[k];
      else out[k] = v;
    }
    return withRefs ? withSecretRefs(out, this.refValues) : out;
  }

  /**
   * Where `${name}` gets its value in a request of `collectionId`, in the order `variables()` applies
   * them: { value, scope }, secret values masked; null when nothing defines it.
   */
  variableSource(collectionId, name) {
    if (name.startsWith("secret:")) return { value: "••••", scope: "secret manager reference" };
    const script = this.scriptVariables[collectionId] ?? {};
    if (name in script && script[name] !== null) return { value: String(script[name]), scope: "set by a script" };
    const env = this.activeEnvironment;
    const secrets = new Set([...(env?.secrets ?? []), ...(this.globals?.secrets ?? [])]);
    const scopes = [
      [this.sharedFor(collectionId), "team environment"],
      [env?.variables ?? {}, env ? `environment ${this.environmentLabel(env)}` : "environment"],
      [collectionService.getCollectionVariables(this.collections, collectionId) ?? {}, "collection"],
      [this.globalsFor(collectionId), "globals"],
    ];
    for (const [vars, scope] of scopes) {
      if (vars && Object.hasOwn(vars, name)) return { value: secrets.has(name) ? "••••" : String(vars[name] ?? ""), scope };
    }
    return null;
  }

  setScriptVariables(collectionId, updates) {
    if (updates && Object.keys(updates).length) this.scriptVariables[collectionId] = { ...this.scriptVariables[collectionId], ...updates };
  }

  setActiveEnvironment(id) {
    this.activeEnvironmentId = this.environments.some((e) => e.id === id) ? id : null;
    this.emit();
  }

  addEnvironment(name) {
    if (!name.trim()) return;
    // In the current org only, until it's made available in every org.
    const env = { id: crypto.randomUUID(), name: name.trim(), variables: {}, ...(this.currentOrg ? { org: this.currentOrg } : {}) };
    this.environments = [...this.environments, env];
    if (!this.activeEnvironment) this.activeEnvironmentId = env.id;
    this.view = { kind: "environments", collectionId: null, environmentId: env.id };
    this.emit();
  }

  renameEnvironment(id, name) {
    if (!name.trim()) return;
    this.environments = this.environments.map((e) => (e.id === id ? { ...e, name: name.trim() } : e));
    this.emit();
  }

  // `secrets`, when given, is the new list of secret names; they must be
  // keys of `variables`.
  setEnvironmentVariables(id, variables, secrets) {
    this.environments = this.environments.map((e) => {
      if (e.id !== id) return e;
      const next = { ...e, variables };
      const names = (secrets ?? secretNames(e)).filter((n) => n in variables);
      if (names.length) next.secrets = names;
      else delete next.secrets;
      return next;
    });
    this.emit();
  }

  deleteEnvironment(id) {
    this.environments = this.environments.filter((e) => e.id !== id);
    if (this.activeEnvironmentId === id) this.activeEnvironmentId = null;
    this.emit();
  }

  importEnvironments(json) {
    // Postman globals go to the globals, not to a new environment.
    let parsed = null;
    try {
      parsed = JSON.parse(json);
    } catch {
      // parseEnvironments says what's wrong
    }
    if (isPostmanGlobals(parsed)) return this.importGlobals(json);
    const imported = parseEnvironments(json); // throws on bad input
    this.environments = [...this.environments, ...imported];
    this.emit();
    this.notify(`Imported ${imported.length} environment${imported.length === 1 ? "" : "s"}`);
  }

  exportEnvironments() {
    return JSON.stringify(exportableEnvironments(this.environments), null, 2);
  }

  // --- scripts --------------------------------------------------------------

  setContext(collectionId, key, value) {
    this.contexts = contextService.updateCollectionContext(this.contexts, collectionId, key, value);
  }

  // Scripts run in this page, as in the React web build (`with(ctx)`); the
  // extension build keeps its sandboxed iframe.
  async runPreScript(script, collectionId, method, url, headers, body) {
    if (!script?.trim()) return { url, headers, body };
    const context = contextService.getCollectionContext(this.contexts, collectionId);
    const variables = this.variables(collectionId);
    const native = platform().scripts;
    if (native) {
      // The desktop runs scripts in Rust (boa), as the CLI does.
      const r = await native.pre({ script, url, method, headers, body, variables, context, globals: this.globalsFor(collectionId) });
      for (const [k, v] of Object.entries(r.contextUpdates)) this.setContext(collectionId, k, v);
      this.setScriptVariables(collectionId, r.variables);
      this.setScriptGlobals(collectionId, r.globals);
      return { url: r.url, headers: r.headers, body: r.body };
    }
    const updates = {};
    const ctx = {
      url,
      method,
      headers: headers.map((h) => ({ ...h })),
      body,
      variables: { ...variables },
      context: { ...context },
      setContext: (k, v) => (updates[k] = v),
      getContext: (k) => (updates[k] !== undefined ? updates[k] : context[k]),
      getVariable: (k) => variables[k],
      setHeader: (name, value) => {
        const h = ctx.headers.find((x) => x.name === name);
        if (h) h.value = value;
        else ctx.headers.push({ name, value });
      },
      setUrl: (u) => (ctx.url = u),
      setBody: (b) => (ctx.body = b),
    };
    const vars = {};
    const globals = {};
    runScript(pmShim, script, ctx, {
      vars: { ...variables },
      setVar: (k, v) => (vars[k] = v),
      globals: this.globalsFor(collectionId),
      setGlobal: (k, v) => (globals[k] = v),
      assert: { ok() {} },
      request: { get url() { return ctx.url; }, method, get headers() { return ctx.headers.map((h) => [h.name, h.value]); }, get body() { return ctx.body; } },
      setUrl: ctx.setUrl,
      setHeader: ctx.setHeader,
      removeHeader: (n) => (ctx.headers = ctx.headers.filter((h) => String(h.name).toLowerCase() !== String(n).toLowerCase())),
      setBody: ctx.setBody,
      response: null,
    });
    for (const [k, v] of Object.entries(updates)) this.setContext(collectionId, k, v);
    this.setScriptVariables(collectionId, vars);
    this.setScriptGlobals(collectionId, globals);
    return { url: ctx.url, headers: ctx.headers, body: ctx.body };
  }

  // Returns the assert.* results (an empty list without a script).
  // `sent` is the request as sent ({url, method, headers, body}), pm.request.
  async runPostScript(script, collectionId, text, headers, status, durationMs, sent) {
    if (!script?.trim()) return [];
    const native = platform().scripts;
    if (native) {
      const r = await native.post({ script, text, headers, status, durationMs, sent, variables: this.variables(collectionId), globals: this.globalsFor(collectionId), context: contextService.getCollectionContext(this.contexts, collectionId) });
      for (const [k, v] of Object.entries(r.contextUpdates)) this.setContext(collectionId, k, v);
      this.setScriptVariables(collectionId, r.variables);
      this.setScriptGlobals(collectionId, r.globals);
      return r.tests;
    }
    let parsed = text;
    try {
      parsed = JSON.parse(text);
    } catch {
      // not JSON
    }
    const context = contextService.getCollectionContext(this.contexts, collectionId);
    const variables = this.variables(collectionId);
    const updates = {};
    const { assert, results } = makeAssert({ statusCode: status, text, headers, durationMs });
    const ctx = {
      assert,
      response: parsed,
      responseText: text,
      responseHeaders: headers,
      statusCode: status,
      variables: { ...variables },
      context: { ...context },
      setContext: (k, v) => (updates[k] = v),
      getContext: (k) => (updates[k] !== undefined ? updates[k] : context[k]),
      getVariable: (k) => variables[k],
      getResponseValue: (path) => path.split(".").reduce((v, k) => (v && typeof v === "object" ? v[k] : undefined), parsed),
      getResponseHeader: (name) => headers[name.toLowerCase()],
    };
    const vars = {};
    const globals = {};
    try {
      runScript(pmShim, script, ctx, {
        vars: { ...variables },
        setVar: (k, v) => (vars[k] = v),
        globals: this.globalsFor(collectionId),
        setGlobal: (k, v) => (globals[k] = v),
        assert,
        request: { url: sent?.url ?? "", method: sent?.method ?? "", headers: (sent?.headers ?? []).filter((h) => h.name).map((h) => [h.name, h.value]), body: typeof sent?.body === "string" ? sent.body : "" },
        response: { code: status, text, headers, time: durationMs },
      });
    } finally {
      for (const [k, v] of Object.entries(updates)) this.setContext(collectionId, k, v);
      this.setScriptVariables(collectionId, vars);
      this.setScriptGlobals(collectionId, globals);
    }
    return results;
  }

  // Interpolates variables, adds auth and runs the pre-request script: the
  // request as sent. Auth comes before the script, so a script can override it.
  async prepare(source, collectionId) {
    await this.resolveRefs(source, collectionId);
    const variables = this.variables(collectionId);
    // A Postman script may set variables this request uses: the ones still
    // missing are filled in after it runs.
    const postman = usesPostmanApi(source.preRequestScript);
    const interpolate = postman ? (text) => fillVariables(text, variables) : (text) => interpolateVariables(text, variables);
    const url = interpolate(source.url);
    const headers = (source.headers || []).map((h) => ({
      name: h.name ? interpolate(h.name) : "",
      value: h.value ? interpolate(h.value) : "",
    }));
    // GraphQL: the JSON body hitop-core builds (hitop-wasm), from the query,
    // variables and operation name with variables filled in.
    const gql = isGraphql(source) ? { format: await loadFormat(), query: source.body || "", variables: source.graphql?.variables || "", operationName: source.graphql?.operationName || "" } : null;
    const graphqlBody = (fill) => gql.format.graphqlBody(fill(gql.query), fill(gql.variables), fill(gql.operationName));
    let body;
    if (gql) {
      try {
        body = graphqlBody(interpolate);
      } catch (e) {
        // Variables a Postman script is still to set may leave them unparsable for now.
        if (!postman || !/\$\{/.test(gql.variables)) throw e;
        body = gql.format.graphqlBody(interpolate(gql.query), "", interpolate(gql.operationName));
      }
      if (!headers.some((h) => h.name.toLowerCase() === "content-type" && h.value)) headers.push({ name: "Content-Type", value: "application/json" });
    } else {
      body = source.body ? interpolate(source.body) : source.body;
    }
    const auth = source.auth?.type === "oauth2" ? await this.oauthBearer(source.auth, interpolate) : source.auth;
    const authed = applyAuth(auth, { url, headers, body }, interpolate);
    const out = await this.runPreScript(source.preRequestScript, collectionId, source.method, authed.url, authed.headers, authed.body);
    if (!postman) return out;
    const after = this.variables(collectionId);
    const strict = (text) => (text ? interpolateVariables(text, after) : text);
    const sentBody = gql && out.body === authed.body ? graphqlBody(strict) : strict(out.body);
    return { url: strict(out.url), headers: out.headers.map((h) => ({ ...h, name: strict(h.name), value: strict(h.value) })), body: sentBody };
  }

  // OAuth 2.0: a Bearer preset with a token kept in memory until 30 s before it runs out.
  async oauthBearer(auth, fill) {
    if (auth.grant === "authorization_code") return this.oauthSignIn(auth, fill);
    const req = oauthTokenRequest(auth, fill);
    this.oauthTokens ??= new Map();
    const now = Date.now() / 1000;
    const have = this.oauthTokens.get(req.key);
    if (have && (have.until == null || now < have.until)) return { type: "bearer", token: have.token };
    const res = await platform().send(req.url, { method: "POST", headers: req.headers, body: req.body });
    const { token, lifetime } = oauthParseToken(res.status, await res.text());
    this.oauthTokens.set(req.key, { token, until: lifetime ? now + Math.max(0, lifetime - 30) : null });
    return { type: "bearer", token };
  }

  // OAuth 2.0 authorization code with PKCE (web app): the provider's sign-in page in a popup,
  // the code back through /app/oauth-callback.html (postMessage, this origin), traded through the
  // proxy. The token is kept in memory (a refresh token renews it); a page reload signs in again.
  async oauthSignIn(auth, fill) {
    if (platform().name === "tauri") throw new Error("OAuth 2.0 authorization code: use the web app or the CLI (hitop send, hitop run in a terminal); the desktop app can't receive the provider's redirect yet");
    this.oauthTokens ??= new Map();
    const key = `code\n${fill(auth.authUrl || "")}\n${fill(auth.clientId || "")}\n${fill(auth.scope || "")}`;
    const now = Date.now() / 1000;
    const have = this.oauthTokens.get(key);
    if (have && (have.until == null || now < have.until)) return { type: "bearer", token: have.token };
    const exchange = async (body, headers, url) => {
      const res = await platform().send(url, { method: "POST", headers, body });
      const text = await res.text();
      const { token, lifetime } = oauthParseToken(res.status, text);
      let refresh = null;
      try {
        refresh = JSON.parse(text).refresh_token ?? null;
      } catch {
        // no refresh token
      }
      this.oauthTokens.set(key, { token, refresh: refresh ?? have?.refresh ?? null, until: lifetime ? now + Math.max(0, lifetime - 30) : null });
      return { type: "bearer", token };
    };
    // Expired: the refresh token first.
    if (have?.refresh) {
      try {
        const base = oauthCodeRequest(auth, fill, "", "", "");
        const body = new URLSearchParams({ grant_type: "refresh_token", refresh_token: have.refresh, client_id: fill(auth.clientId || "") }).toString();
        return await exchange(body, base.headers, base.url);
      } catch {
        // sign in again below
      }
    }
    const redirect = fill(auth.redirectUri || "") || `${location.origin}${import.meta.env.BASE_URL}oauth-callback.html`;
    const { verifier, challenge } = await pkcePair();
    const state = [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, "0")).join("");
    const popup = window.open(oauthAuthorizeUrl(auth, fill, redirect, challenge, state), "hitop-oauth", "popup,width=520,height=720");
    if (!popup) throw new Error("OAuth 2.0: allow pop-ups for HITOP to sign in, then send again");
    const code = await new Promise((resolve, reject) => {
      const done = (fn, v) => {
        removeEventListener("message", on);
        clearInterval(watch);
        fn(v);
      };
      const on = (e) => {
        if (e.origin !== location.origin || !e.data?.hitopOAuth) return;
        const m = e.data.hitopOAuth;
        if (m.error) done(reject, new Error(`OAuth 2.0: the sign-in was refused: ${m.error}`));
        else if (m.state !== state) done(reject, new Error("OAuth 2.0: the sign-in answered for another request"));
        else done(resolve, m.code);
      };
      addEventListener("message", on);
      const watch = setInterval(() => popup.closed && done(reject, new Error("OAuth 2.0: the sign-in window was closed")), 500);
    });
    const req = oauthCodeRequest(auth, fill, code, verifier, redirect);
    return exchange(req.body, req.headers, req.url);
  }

  // --- sending ----------------------------------------------------------------

  // Prepares and sends one request; returns { prepared, res }. Handles form
  // bodies and the cookie jar (proxied requests only: the browser keeps
  // localhost cookies itself and hides Set-Cookie from scripts).
  // `signal` stops it (Cancel, the request timeout); the desktop gets the
  // timeout too, so Rust gives up as well.
  async dispatch(source, collectionId, { signal, onText } = {}) {
    signal ??= this.timeoutSignal();
    let prepared;
    try {
      prepared = await this.prepare(source, collectionId);
      // A URL whose variables leave it without a scheme gets one too; never a relative fetch.
      prepared = { ...prepared, url: (await loadFormat()).withScheme(prepared.url) };
    } catch (e) {
      e.stage = "prepare"; // a variable or pre-request script error: nothing was sent
      throw e;
    }
    const headers = [...prepared.headers];
    const hasHeader = (name) => headers.some((h) => h.name.toLowerCase() === name && h.value);
    let body = prepared.body;
    const mode = source.bodyMode || "raw";
    const scriptChangedBody = prepared.body !== (source.body ? interpolateVariables(source.body, this.variables(collectionId)) : source.body);
    if (BODY_METHODS.includes(source.method) && mode !== "raw" && !scriptChangedBody) {
      const variables = this.variables(collectionId);
      const fields = (source.formFields || []).map((f) => ({
        ...f,
        key: f.key ? interpolateVariables(f.key, variables) : "",
        value: f.type === "file" ? f.value : interpolateVariables(f.value || "", variables),
      }));
      const built = mode === "multipart" ? await multipartBody(fields, this.files) : urlEncodedBody(fields);
      body = built.body;
      if (!hasHeader("content-type")) headers.push({ name: "Content-Type", value: built.contentType });
    } else if (BODY_METHODS.includes(source.method) && body && typeof body === "string" && !hasHeader("content-type")) {
      // A raw body says what it is, unless the request does (GraphQL already has its own):
      // hitop-core's rules (spec/vectors/send.json), as the CLI, MCP and HitopKit send it.
      const type = (await loadFormat()).rawContentType(body);
      if (type) headers.push({ name: "Content-Type", value: type });
    }
    // AWS Signature v4 signs the request as it goes out (headers and body final).
    if (source.auth?.type === "awsv4") {
      const vars = this.variables(collectionId);
      const fill = (t) => interpolateVariables(t || "", vars);
      const aws = { accessKey: fill(source.auth.accessKey), secretKey: fill(source.auth.secretKey), sessionToken: fill(source.auth.sessionToken), region: fill(source.auth.region), service: fill(source.auth.service) };
      if (!aws.accessKey || !aws.secretKey || !aws.region || !aws.service) {
        const e = new Error("AWS Signature: give the access key, secret key, region and service");
        e.stage = "prepare";
        throw e;
      }
      const signed = await awsV4Headers(aws, source.method, prepared.url, headers, BODY_METHODS.includes(source.method) ? body : "");
      headers.splice(0, headers.length, ...signed.headers);
    }
    const p = platform();
    const proxied = p.usesProxy(prepared.url);
    const jar = p.managesCookies(prepared.url);
    if (jar && this.cookieJarEnabled && !hasHeader("cookie")) {
      const cookie = cookieHeader(this.cookies, prepared.url);
      if (cookie) headers.push({ name: "Cookie", value: cookie });
    }
    const options = { method: source.method, headers: headersObject(headers), ...(p.cookies ? { cookies: this.cookieJarEnabled } : {}) };
    if (signal) options.signal = signal;
    if (this.settings.timeoutMs) options.timeoutMs = this.settings.timeoutMs;
    if (body && BODY_METHODS.includes(source.method)) options.body = body;
    let res;
    try {
      res = onText && p.stream ? await p.stream(prepared.url, options, onText) : await p.send(prepared.url, options);
    } catch (e) {
      if (signal?.aborted) throw signal.reason;
      // fetch says only "Failed to fetch"; for direct requests the usual cause
      // is a server that doesn't allow CORS.
      if (p.name === "web" && !proxied && e instanceof TypeError)
        e.message = "Couldn't reach the server. Requests to localhost and private networks go straight from your browser, so the server must allow CORS from this site; the desktop and mobile apps don't need it.";
      throw e;
    }
    // Digest: the server's 401 challenge answered once.
    const challenge = source.auth?.type === "digest" && res.status === 401 ? parseDigestChallenge(res.headers.get("www-authenticate")) : null;
    if (challenge) {
      const vars = this.variables(collectionId);
      const fill = (t) => interpolateVariables(t || "", vars);
      const u = new URL(prepared.url);
      const cnonce = [...crypto.getRandomValues(new Uint8Array(8))].map((b) => b.toString(16).padStart(2, "0")).join("");
      const value = await digestAnswer(challenge, fill(source.auth.username), fill(source.auth.password), source.method, `${u.pathname}${u.search}`, cnonce);
      headers.splice(0, headers.length, ...headers.filter((h) => h.name.toLowerCase() !== "authorization"), { name: "Authorization", value });
      options.headers = headersObject(headers);
      res = await p.send(prepared.url, options);
    }
    if (Array.isArray(res.cookies)) this.cookies = res.cookies;
    if (jar && this.cookieJarEnabled && res.headers.pairs) {
      const lines = res.headers.pairs.filter(([k]) => k.toLowerCase() === "set-cookie").map(([, v]) => v);
      if (lines.length) this.cookies = mergeCookies(this.cookies, lines, res.url || prepared.url);
    }
    return { prepared: { ...prepared, headers }, res };
  }

  async send() {
    const index = this.currentTab;
    let tab = this.tabs[index];
    if (!tab.url.trim()) {
      this.urlErrors.set(tab.id, "Enter a URL, such as https://api.example.com/users");
      this.emit();
      this.notify("Enter a URL first.");
      return;
    }
    this.refValues = {};
    if (tab.kind === "websocket") return this.wsConnect();
    if (tab.kind === "grpc") return this.grpcCall();
    // "api.example.com/users" is sent (and kept) as https://api.example.com/users;
    // a URL with variables is checked once they are filled in (dispatch).
    if (!tab.url.includes("${")) {
      try {
        const url = (await loadFormat()).withScheme(tab.url);
        if (url !== tab.url) {
          this.tabs = tabService.updateTab(this.tabs, index, { url });
          tab = this.tabs[index];
        }
      } catch (e) {
        this.urlErrors.set(tab.id, e.message);
        this.emit();
        return;
      }
    }
    const source = {
      url: tab.url,
      method: tab.method,
      headers: tab.headers,
      body: tab.requestBody,
      preRequestScript: tab.preRequestScript,
      auth: tab.auth,
      ...SAVED_FIELDS(tab),
    };
    this.tabs = tabService.updateTab(this.tabs, index, { ...NO_RESPONSE, loading: true });
    // An event stream (Accept: text/event-stream, web app): its events as they arrive, in memory
    // (store.events by tab id), until it ends or Stop; no time limit.
    const streaming = Boolean(platform().stream) && asksForEvents(tab.headers);
    this.events ??= new Map();
    let onText;
    if (streaming) {
      const live = { events: [], open: true, rest: "" };
      this.events.set(tab.id, live);
      onText = (piece) => {
        const { events, rest } = parseEvents(live.rest + piece);
        live.rest = rest;
        if (!events.length) return;
        const at = Date.now();
        live.events = [...live.events, ...events.map((e) => ({ ...e, at }))].slice(-1000);
        this.version = (this.version ?? 0) + 1;
        this.dispatchEvent(new Event("change"));
      };
    } else this.events.delete(tab.id);
    // Cancel (store.cancel) and the request timeout (Settings) abort it.
    const call = this.startCall(tab.id, { timeout: !streaming });
    this.emit();
    const started = performance.now();
    try {
      const { prepared, res } = await this.dispatch(source, tab.loadedCollectionId, { signal: call.signal, onText });
      if (streaming) this.events.get(tab.id).open = false;
      const headers = {};
      res.headers.forEach((v, k) => (headers[k] = v));
      const { data, type, raw, jsonError } = await readResponse(res);
      const durationMs = res.timeMs ?? Math.round(performance.now() - started);
      // The body as received, for Raw (memory only: the tab keeps the formatted one).
      if (raw !== undefined) this.rawBodies.set(tab.id, raw);
      else this.rawBodies.delete(tab.id);
      this.patchTab(tab.id, {
        response: data,
        responseType: type,
        jsonError: jsonError ?? null,
        statusCode: res.status,
        responseHeaders: headers,
        durationMs,
        // DNS, connect, waiting, download (hitop-core timing.rs: the desktop and the web proxy).
        timings: res.timings ?? null,
        truncated: Boolean(res.truncated),
        loading: false,
      });
      this.history = historyService.addToHistory(
        this.history,
        historyService.createHistoryItem({
          url: tab.url,
          method: tab.method,
          headers: tab.headers,
          requestBody: tab.requestBody,
          statusCode: res.status,
          preRequestScript: tab.preRequestScript,
          postRequestScript: tab.postRequestScript,
        }),
      );
      this.history[this.history.length - 1].auth = cleanAuth(tab.auth);
      this.history[this.history.length - 1].response = maskSecrets(snapshotResponse({ status: res.status, headers, body: data, durationMs }), this.secretValuesFor(tab.loadedCollectionId));
      let tests = [];
      try {
        tests = await this.runPostScript(tab.postRequestScript, tab.loadedCollectionId, data, headers, res.status, durationMs, { ...prepared, method: tab.method });
      } catch (e) {
        tests = [...tests, { name: "Script after", pass: false, detail: e.message }];
        this.notify(`The script after the request failed: ${e.message}`);
      }
      this.patchTab(tab.id, { tests });
    } catch (e) {
      if (call.signal.aborted && call.signal.reason?.name !== "TimeoutError") {
        // Cancelled: no response, nothing in history.
        this.patchTab(tab.id, { response: null, responseType: "cancelled", statusCode: null, responseHeaders: null, loading: false });
      } else if (call.signal.aborted) {
        const limit = seconds(this.settings.timeoutMs);
        this.patchTab(tab.id, { response: `Error: No response within ${limit}. The time limit is in Settings, under Requests.`, responseType: "error", statusCode: null, responseHeaders: null, loading: false });
      } else if (e.stage === "prepare") {
        this.patchTab(tab.id, { loading: false });
        this.notify(`The request wasn't sent: ${e.message}`);
      } else {
        this.patchTab(tab.id, { response: `Error: ${e.message}`, responseType: "error", statusCode: null, responseHeaders: null, loading: false });
      }
    } finally {
      call.done();
      const live = this.events?.get(tab.id);
      if (live) live.open = false;
    }
    this.emit();
  }

  // A request in flight for this tab: { signal, done() }. The signal aborts on
  // Cancel (AbortError) or after the Settings time limit (TimeoutError).
  startCall(tabId, { timeout = true } = {}) {
    const controller = new AbortController();
    const ms = timeout ? this.settings.timeoutMs : 0;
    const timer = ms > 0 ? setTimeout(() => controller.abort(new DOMException(`No response within ${seconds(ms)}`, "TimeoutError")), ms) : null;
    this.inflight.set(tabId, controller);
    return {
      signal: controller.signal,
      done: () => {
        clearTimeout(timer);
        if (this.inflight.get(tabId) === controller) this.inflight.delete(tabId);
      },
    };
  }

  // Stops the tab's request in flight (the Cancel button).
  cancel(tabId = this.tab?.id) {
    this.inflight.get(tabId)?.abort(new DOMException("Cancelled", "AbortError"));
  }

  // The Settings time limit as a signal, for requests with no Cancel (runs, schema).
  timeoutSignal() {
    const ms = this.settings.timeoutMs;
    return ms > 0 && typeof AbortSignal.timeout === "function" ? AbortSignal.timeout(ms) : undefined;
  }

  setSettings(changes) {
    this.settings = { ...this.settings, ...changes };
    saveSettings(this.settings);
    this.emit();
  }

  // --- WebSocket --------------------------------------------------------------

  socket(tabId = this.tab?.id) {
    return this.sockets.get(tabId) ?? { status: "closed", protocol: null, log: [], conn: null };
  }

  wsLog(tabId, entry) {
    const s = this.socket(tabId);
    const log = [...s.log, { time: new Date().toISOString(), ...entry }].slice(-WS_LOG_MAX);
    this.sockets = new Map(this.sockets).set(tabId, { ...s, log });
    this.emit();
  }

  wsSet(tabId, changes) {
    this.sockets = new Map(this.sockets).set(tabId, { ...this.socket(tabId), ...changes });
    this.emit();
  }

  // Opens the current tab's connection: URL, headers, auth and subprotocols
  // with variables filled in. The browser sends no handshake headers (its
  // WebSocket can't), the desktop does (Rust); the log says which.
  async wsConnect() {
    const tab = this.tab;
    const id = tab.id;
    if (["open", "connecting"].includes(this.socket(id).status)) return;
    // This connection's identity: callbacks of an older (cancelled) one never touch a newer one.
    const token = {};
    this.wsSet(id, { status: "connecting", protocol: null, token });
    const current = () => this.socket(id).token === token;
    let url, headers, protocols;
    try {
      await this.resolveRefs({ url: tab.url, headers: tab.headers, auth: tab.auth, subprotocols: tab.subprotocols, wsMessages: tab.wsMessages }, tab.loadedCollectionId);
      const variables = this.variables(tab.loadedCollectionId);
      const fill = (text) => (text ? interpolateVariables(text, variables) : text);
      const authed = applyAuth(tab.auth, { url: fill(tab.url.trim()), headers: (tab.headers || []).filter((h) => h.name && h.enabled !== false).map((h) => ({ name: fill(h.name), value: fill(h.value || "") })), body: "" }, fill);
      url = wsUrl(authed.url);
      headers = authed.headers;
      protocols = (tab.subprotocols || []).map(fill).filter(Boolean);
    } catch (e) {
      if (current()) this.wsSet(id, { status: "closed", token: null });
      this.notify(`Not connected: ${errorText(e)}`);
      return;
    }
    if (!current()) return; // cancelled while references resolved
    const p = platform();
    this.wsLog(id, { kind: "system", text: `Connecting to ${url}${protocols.length ? ` (subprotocols ${protocols.join(", ")})` : ""}` });
    if (headers.length && !(p.websocketHeaders || p.websocketSendsHeaders?.(url)))
      this.wsLog(id, { kind: "system", text: `Not sent: ${headers.map((h) => h.name).join(", ")}. A browser can't set WebSocket handshake headers (auth included); the desktop app and hitop ws send them. An API key in the query works here.` });
    let opened = false;
    const conn = p.websocket(url, { headers, protocols }, {
      onOpen: ({ protocol }) => {
        if (!current()) return;
        opened = true;
        this.wsSet(id, { status: "open", protocol: protocol || null });
        this.wsLog(id, { kind: "system", text: `Connected${protocol ? `, subprotocol ${protocol}` : ""}` });
      },
      onMessage: ({ text, bytes }) => current() && this.wsLog(id, bytes ? { kind: "received", bytes: Array.from(bytes) } : { kind: "received", text }),
      onError: (message) => current() && this.wsLog(id, { kind: "error", text: opened ? message : `Couldn't connect: ${message}` }),
      onClose: ({ code, reason }) => {
        if (!current()) return;
        this.wsSet(id, { status: "closed", conn: null, token: null });
        if (opened || code) this.wsLog(id, { kind: "system", text: `Closed${code ? ` ${code}` : ""}${reason ? ` ${reason}` : ""}` });
      },
    });
    if (current()) this.wsSet(id, { conn });
    else conn?.close();
  }

  wsDisconnect(tabId = this.tab?.id) {
    const s = this.socket(tabId);
    if (s.status === "closed") return;
    s.conn?.close();
    // Cancel while connecting: closed now, and that connection's late callbacks are ignored (token).
    if (s.status === "connecting") this.wsSet(tabId, { status: "closed", conn: null, token: null });
  }

  // Sends text with variables filled in; logs it as sent.
  wsSend(text) {
    const tab = this.tab;
    const s = this.socket(tab.id);
    if (s.status !== "open" || !s.conn) {
      this.notify("Connect first.");
      return false;
    }
    let out;
    try {
      out = interpolateVariables(text ?? "", this.variables(tab.loadedCollectionId));
      s.conn.send(out);
    } catch (e) {
      this.notify(`Not sent: ${errorText(e)}`);
      return false;
    }
    this.wsLog(tab.id, { kind: "sent", text: out });
    return true;
  }

  wsClearLog(tabId = this.tab?.id) {
    this.wsSet(tabId, { log: [] });
  }

  // --- gRPC -------------------------------------------------------------------
  // Desktop only: the platform's `grpc` (Rust through Tauri) is null in a
  // browser, which can't make gRPC calls.

  grpcState(tabId = this.tab?.id) {
    return this.grpcTabs.get(tabId) ?? { status: "idle", log: [], call: null, schema: null, schemaLoading: false, schemaError: null };
  }

  grpcSet(tabId, changes) {
    this.grpcTabs = new Map(this.grpcTabs).set(tabId, { ...this.grpcState(tabId), ...changes });
    this.emit();
  }

  grpcLog(tabId, entry) {
    const g = this.grpcState(tabId);
    this.grpcSet(tabId, { log: [...g.log, { time: new Date().toISOString(), ...entry }].slice(-GRPC_LOG_MAX) });
  }

  grpcClearLog(tabId = this.tab?.id) {
    this.grpcSet(tabId, { log: [] });
  }

  // The server and schema source of a tab, variables filled in: the
  // address, plaintext, metadata (enabled headers and header auth), the
  // .proto files and import paths, and the collection's folder for
  // relative paths (desktop folders are paths).
  grpcServer(tab = this.tab) {
    const variables = this.variables(tab.loadedCollectionId);
    const fill = (text) => (text ? interpolateVariables(text, variables) : text);
    const authed = applyAuth(tab.auth, { url: "", headers: (tab.headers || []).filter((h) => h.name && h.enabled !== false).map((h) => ({ name: fill(h.name), value: fill(h.value || "") })), body: "" }, fill);
    const ref = tab.loadedCollectionId != null ? this.folderOf(tab.loadedCollectionId)?.ref : null;
    return {
      address: fill(tab.url.trim()),
      plaintext: !!tab.grpcPlaintext,
      metadata: authed.headers.map((h) => [h.name, h.value]),
      protos: (tab.grpcProtos || []).map(fill).filter(Boolean),
      importPaths: (tab.grpcImportPaths || []).map(fill).filter(Boolean),
      baseDir: typeof ref === "string" ? ref : null,
    };
  }

  // Reads the schema (reflection or the .proto files) for the service and
  // method pickers.
  async grpcLoadSchema() {
    const tab = this.tab;
    const id = tab.id;
    const g = platform().grpc;
    if (!g) return this.notify("gRPC is available in the desktop app and CLI.");
    if (!tab.url.trim()) return this.notify("Enter the server address first (host:port).");
    let server;
    try {
      server = this.grpcServer(tab);
    } catch (e) {
      return this.notify(`Schema not loaded: ${errorText(e)}`);
    }
    this.grpcSet(id, { schemaLoading: true, schemaError: null });
    try {
      const schema = await g.list(server);
      this.grpcSet(id, { schema, schemaLoading: false });
      const t = this.tabs.find((x) => x.id === id);
      // Pick the first service and method when none is chosen yet.
      if (t && !t.grpcService) {
        const svc = schema.services.find((x) => !x.name.startsWith("grpc.reflection.")) ?? schema.services[0];
        if (svc) this.updateTabById(id, { grpcService: svc.name, grpcMethod: svc.methods[0]?.name ?? "" });
      }
    } catch (e) {
      this.grpcSet(id, { schemaLoading: false, schemaError: errorText(e) });
    }
  }

  // Replaces the message with a template of the method's request message.
  async grpcTemplate() {
    const tab = this.tab;
    const g = platform().grpc;
    if (!g) return;
    if (!tab.grpcService || !tab.grpcMethod) return this.notify("Pick a service and method first.");
    try {
      const d = await g.describe(this.grpcServer(tab), `${tab.grpcService}/${tab.grpcMethod}`);
      if (d.template) this.updateTabById(tab.id, { grpcMessage: JSON.stringify(d.template, null, 2) });
    } catch (e) {
      this.notify(`No template: ${errorText(e)}`);
    }
  }

  // Calls the tab's method with its message, variables filled in; the
  // headers, messages and status go to the tab's log.
  async grpcCall() {
    const tab = this.tab;
    const id = tab.id;
    const g = platform().grpc;
    if (!g) return this.notify("gRPC is available in the desktop app and CLI.");
    if (this.grpcState(id).status === "running") return;
    if (!tab.grpcService || !tab.grpcMethod) return this.notify("Pick a service and method first (Load schema).");
    // This call's identity: events of a cancelled call never touch a newer one.
    const token = {};
    this.grpcSet(id, { status: "running", token });
    const current = () => this.grpcState(id).token === token;
    let args;
    try {
      await this.resolveRefs({ url: tab.url, headers: tab.headers, auth: tab.auth, grpcMessage: tab.grpcMessage }, tab.loadedCollectionId);
      const fill = (text) => (text ? interpolateVariables(text, this.variables(tab.loadedCollectionId)) : text);
      args = { ...this.grpcServer(tab), service: tab.grpcService, method: tab.grpcMethod, message: fill(tab.grpcMessage || "") };
    } catch (e) {
      if (current()) this.grpcSet(id, { status: "done", token: null });
      return this.notify(`Not sent: ${errorText(e)}`);
    }
    if (!current()) return; // cancelled while references resolved
    this.grpcLog(id, { kind: "sent", text: `${args.service}/${args.method} ${args.message.trim() || "{}"}` });
    const done = () => this.grpcSet(id, { status: "done", call: null, token: null });
    const call = g.call(args, {
      onHeaders: (metadata) => current() && metadata.length && this.grpcLog(id, { kind: "headers", text: metadata.map(([k, v]) => `${k}: ${v}`).join("\n") }),
      onMessage: (data) => current() && this.grpcLog(id, { kind: "message", text: JSON.stringify(data, null, 2) }),
      onStatus: ({ code, name, message, trailers }) => {
        if (!current()) return;
        const head = code === 0 ? "OK" : `${name} (${code})${message ? `: ${message}` : ""}`;
        const tail = (trailers || []).map(([k, v]) => `${k}: ${v}`).join("\n");
        this.grpcLog(id, { kind: "status", ok: code === 0, text: tail ? `${head}\n${tail}` : head });
        done();
      },
      onError: (message) => {
        if (!current()) return;
        this.grpcLog(id, { kind: "error", text: message });
        done();
      },
    });
    if (current()) this.grpcSet(id, { call });
    else call?.cancel?.();
  }

  grpcCancel(tabId = this.tab?.id) {
    const g = this.grpcState(tabId);
    if (g.status !== "running") return;
    g.call?.cancel();
    this.grpcSet(tabId, { status: "done", call: null, token: null });
    // `cancelled` marks the call's outcome for the status chip (grpc-view.js), next to status and error.
    this.grpcLog(tabId, { kind: "system", text: "Cancelled", cancelled: true });
  }

  // Fetches the GraphQL schema of the current tab's endpoint by
  // introspection: its URL, headers, auth and cookies, through the same
  // transport as Send (proxy or direct; Rust on the desktop). The tab's
  // script doesn't run. The schema feeds completion in the query editor.
  async fetchSchema() {
    const tab = this.tab;
    if (!tab.url.trim()) {
      this.notify("Enter a URL first.");
      return null;
    }
    const id = tab.id;
    const set = (entry) => {
      this.schemas = new Map(this.schemas).set(id, entry);
      this.emit();
    };
    set({ ...this.schemas.get(id), loading: true, error: null });
    try {
      const gql = await import("./graphql-schema.js");
      const source = {
        url: tab.url,
        method: "POST",
        headers: tab.headers,
        auth: tab.auth,
        body: gql.INTROSPECTION_QUERY,
        bodyType: "graphql",
        graphql: { operationName: "IntrospectionQuery" },
        preRequestScript: "",
      };
      const { prepared, res } = await this.dispatch(source, tab.loadedCollectionId);
      const text = await res.text();
      if (!res.ok && !text.includes("__schema")) throw new Error(`the server answered ${res.status}${text ? `: ${text.slice(0, 160)}` : ""}`);
      const schema = gql.schemaFromResponse(text);
      const types = Object.keys(schema.getTypeMap()).filter((n) => !n.startsWith("__")).length;
      set({ schema, types, url: prepared.url, loading: false, error: null });
      return schema;
    } catch (e) {
      set({ schema: this.schemas.get(id)?.schema ?? null, loading: false, error: errorText(e) });
      return null;
    }
  }

  // Runs a collection's requests in order, stopping at the first failure.
  async runCollection(id) {
    const collection = this.collection(id);
    if (!collection?.requests?.length) return [];
    this.run = { ...this.run, collectionId: id, results: [], running: true };
    this.emit();
    const results = [];
    // Secret values of the environment and of the globals this collection sees.
    this.refValues = {};
    const push = (r) => {
      r = maskSecrets(r, this.secretValuesFor(id));
      results.push(r);
      this.run = { ...this.run, results: [...results] };
      this.emit();
    };
    for (let i = 0; i < collection.requests.length; i++) {
      const request = collection.requests[i];
      // WebSocket requests connect on demand; a run skips them, as `hitop run` does.
      if (isWebsocket(request) || isGrpc(request)) continue;
      try {
        const started = performance.now();
        const { prepared, res } = await this.dispatch(request, id);
        const duration = res.timeMs ?? Math.round(performance.now() - started);
        const text = await res.text();
        const headers = {};
        res.headers.forEach((v, k) => (headers[k] = v));
        let tests = [];
        try {
          tests = await this.runPostScript(request.postRequestScript, id, text, headers, res.status, duration, { ...prepared, method: request.method });
        } catch (e) {
          tests = [...tests, { name: "Script after", pass: false, detail: e.message }];
        }
        // The run item keeps the request as sent: mask secret values in it.
        const response = snapshotResponse({ status: res.status, headers, body: text, durationMs: duration });
        const item = maskSecrets({ ...historyService.createCollectionRunHistoryItem(request, prepared, res, collection.name, i), auth: request.auth, response }, this.secretValuesFor(id));
        this.history = historyService.addToHistory(this.history, item);
        const failed = tests.filter((t) => !t.pass);
        const success = res.ok && failed.length === 0;
        push({ requestName: request.name, method: request.method, url: prepared.url, statusCode: res.status, success, tests, response: failed.length ? failed.map((t) => `${t.name}: ${t.detail}`).join("; ") : text.slice(0, 200), duration });
        if (!success) break;
      } catch (e) {
        push({ requestName: request.name, method: request.method, url: request.url, statusCode: e.stage === "prepare" ? "Script error" : "Error", success: false, response: e.message });
        break;
      }
    }
    this.run = { ...this.run, running: false };
    this.emit();
    return results;
  }


  // --- collection folders and zip files ------------------------------------

  folderOf(collectionId) {
    return this.folders.get(collectionId) ?? null;
  }

  // An environment's name in lists: a folder's environments say whose they are.
  environmentLabel(env) {
    const owner = this.folderEnvs.get(env.id);
    const col = owner && this.collection(owner.collectionId);
    return col ? `${env.name} (${col.name})` : env.name;
  }

  // What a folder holds now: the collection and its environments, secret
  // values left out (they go to the keychain or this browser instead).
  folderContent(collectionId) {
    const collection = this.collection(collectionId);
    const environments = this.environments
      .filter((e) => this.folderEnvs.get(e.id)?.collectionId === collectionId)
      .map((e) => withoutSecretValues({ ...e, id: this.folderEnvs.get(e.id).id }));
    // A folder's globals.yaml goes back as it came (left out: the file stays as it is).
    const globals = this.folders.get(collectionId)?.globals;
    return globals ? { collection, environments, globals: withoutSecretValues({ ...globals, id: "globals", name: "globals" }) } : { collection, environments };
  }

  folderSecretValues(collectionId) {
    const col = this.collection(collectionId);
    const out = {};
    if (!col) return out;
    for (const e of this.environments) {
      if (this.folderEnvs.get(e.id)?.collectionId !== collectionId) continue;
      for (const n of secretNames(e)) if (e.variables[n]) out[account(workspaceOf(col), e.name, n)] = e.variables[n];
    }
    return out;
  }

  // Puts a folder's collection and environments into the app, replacing the
  // collection with the same id (the folder wins).
  async attachFolder(ref, folder, { remember = true } = {}) {
    const p = platform();
    const col = folder.collection;
    const envs = (folder.environments || []).map((e) => ({ ...e, variables: { ...e.variables }, folderId: e.id, id: `folder:${col.id}:${e.id}` }));
    const accounts = envs.flatMap((e) => secretNames(e).map((n) => account(workspaceOf(col), e.name, n)));
    const values = accounts.length ? await p.folderSecrets.load(accounts) : {};
    for (const e of envs) for (const n of secretNames(e)) e.variables[n] = values[account(workspaceOf(col), e.name, n)] ?? "";
    // The folder's globals.yaml, with secret values from the keychain (desktop).
    let globals = null;
    if (folder.globals) {
      globals = { variables: { ...folder.globals.variables }, secrets: secretNames(folder.globals) };
      const gAccounts = globals.secrets.map((n) => account(workspaceOf(col), GLOBALS_ENV, n));
      const gValues = gAccounts.length ? await p.folderSecrets.load(gAccounts) : {};
      for (const n of globals.secrets) globals.variables[n] = gValues[account(workspaceOf(col), GLOBALS_ENV, n)] ?? "";
    }
    for (const [id, owner] of this.folderEnvs) if (owner.collectionId === col.id) this.folderEnvs.delete(id);
    for (const e of envs) {
      this.folderEnvs.set(e.id, { collectionId: col.id, id: e.folderId });
      delete e.folderId;
    }
    this.environments = [...this.environments.filter((e) => !e.id.startsWith?.(`folder:${col.id}:`)), ...envs];
    const at = this.collections.findIndex((c) => c.id === col.id);
    this.collections = at === -1 ? [...this.collections, col] : this.collections.map((c, i) => (i === at ? col : c));
    const entry = { ref, label: p.folders.label(ref), connected: true, error: null, written: null, secrets: {}, globals };
    this.folders.set(col.id, entry);
    entry.secrets = this.folderSecretValues(col.id);
    entry.written = JSON.stringify(this.folderContent(col.id));
    p.setFolderCollections?.([...this.folders.keys()]);
    if (remember) p.folders.remember(col.id, ref);
    this.emit();
    return col;
  }

  async openFolder() {
    const f = platform().folders;
    if (!f?.pick) return;
    let ref;
    try {
      ref = await f.pick();
    } catch (e) {
      this.notify(`Couldn't open the folder: ${errorText(e)}`);
      return;
    }
    if (!ref) return;
    let folder;
    try {
      folder = await f.read(ref);
    } catch (e) {
      const text = errorText(e);
      this.notify(text.includes("not a collection folder") ? `${f.label(ref)} has no hitop.yaml, so it isn't a collection folder. To start one, open a collection and choose Save to folder.` : `Couldn't open ${f.label(ref)}: ${text}`);
      return;
    }
    const col = await this.attachFolder(ref, folder);
    this.show("collection", col.id);
    this.notify(`Opened ${f.label(ref)}. Changes are saved to the folder as you make them.`);
  }

  // Writes a collection into a new folder and keeps saving there.
  async saveToFolder(collectionId) {
    const f = platform().folders;
    const col = this.collection(collectionId);
    if (!f?.pick || !col) return;
    let ref;
    try {
      ref = await f.pick();
    } catch (e) {
      this.notify(`Couldn't open the folder: ${errorText(e)}`);
      return;
    }
    if (!ref) return;
    try {
      await f.read(ref);
      this.notify(`${f.label(ref)} already holds a collection. Use Open folder to work on it, or pick an empty folder.`);
      return;
    } catch {
      // not a collection folder yet: good
    }
    const folder = { collection: col, environments: [] };
    try {
      await f.write(ref, folder);
    } catch (e) {
      this.notify(`Couldn't save to ${f.label(ref)}: ${errorText(e)}`);
      return;
    }
    await this.attachFolder(ref, folder);
    this.notify(`Saved ${col.name} to ${f.label(ref)}. Changes are saved there as you make them.`);
  }

  // A folder from an earlier visit (web): the browser asks again.
  async reconnectFolder(collectionId) {
    const f = platform().folders;
    const entry = this.folders.get(collectionId);
    if (!f || !entry) return;
    try {
      if (!(await f.reconnect(entry.ref))) return;
      await this.attachFolder(entry.ref, await f.read(entry.ref), { remember: false });
      this.notify(`Reconnected ${entry.label}`);
    } catch (e) {
      this.notify(`Couldn't read ${entry.label}: ${errorText(e)}`);
    }
  }

  // Takes the collection out of the app; the folder's files stay.
  closeFolder(collectionId) {
    const p = platform();
    this.folders.delete(collectionId);
    p.folders?.forget(collectionId);
    p.setFolderCollections?.([...this.folders.keys()]);
    for (const [id, owner] of this.folderEnvs) {
      if (owner.collectionId !== collectionId) continue;
      this.folderEnvs.delete(id);
      this.environments = this.environments.filter((e) => e.id !== id);
      if (this.activeEnvironmentId === id) this.activeEnvironmentId = null;
    }
  }

  closeAllFolders() {
    for (const id of [...this.folders.keys()]) this.closeFolder(id);
  }

  async restoreFolders() {
    const f = platform().folders;
    if (!f) return;
    for (const { collectionId, ref, connected } of await f.restore()) {
      if (!connected) {
        // Web: the browser needs a click before the app may read it again.
        if (this.collection(collectionId)) this.folders.set(collectionId, { ref, label: f.label(ref), connected: false, error: null, written: null, secrets: {} });
        else f.forget(collectionId);
        continue;
      }
      try {
        await this.attachFolder(ref, await f.read(ref), { remember: false });
      } catch (e) {
        f.forget(collectionId);
        this.notify(`Couldn't open the folder ${f.label(ref)}: ${errorText(e)}. Open it again with Open folder.`);
      }
    }
    this.emit();
  }

  // Live save: each connected folder gets what changed, one write at a time.
  syncFolders() {
    const p = platform();
    for (const [id, entry] of this.folders) {
      if (!entry.connected) continue;
      if (!this.collection(id)) {
        this.closeFolder(id);
        continue;
      }
      const secrets = this.folderSecretValues(id);
      const changed = Object.fromEntries([
        ...Object.entries(secrets).filter(([k, v]) => entry.secrets[k] !== v),
        ...Object.keys(entry.secrets).filter((k) => !(k in secrets)).map((k) => [k, null]),
      ]);
      if (Object.keys(changed).length) {
        p.folderSecrets.save(changed);
        entry.secrets = secrets;
      }
      const content = this.folderContent(id);
      const json = JSON.stringify(content);
      if (json === entry.written) continue;
      if (entry.writing) {
        entry.again = true;
        continue;
      }
      entry.writing = true;
      entry.saving = p.folders
        .write(entry.ref, content)
        .then(() => {
          entry.written = json;
          if (entry.error) {
            entry.error = null;
            this.dispatchEvent(new Event("change"));
          }
        })
        .catch((e) => {
          const first = !entry.error;
          entry.error = errorText(e);
          entry.written = json; // don't retry the same content on every keystroke
          if (first) this.notify(`Couldn't save to ${entry.label}: ${entry.error}`);
          this.dispatchEvent(new Event("change"));
        })
        .finally(() => {
          entry.writing = false;
          if (entry.again) {
            entry.again = false;
            this.syncFolders();
          }
        });
    }
  }

  // Writes what a folder collection has changed now, and waits for it (the
  // desktop's Git panel does this before it commits, pulls or switches).
  async flushFolder(collectionId) {
    const entry = this.folders.get(collectionId);
    if (!entry?.connected) return;
    this.save();
    for (let i = 0; i < 20 && (entry.writing || entry.again); i++) await entry.saving;
  }

  // Reads a folder collection again after its files changed on disk (a
  // pull, a branch switch, a resolved conflict): the folder wins.
  async reloadFolder(collectionId) {
    const f = platform().folders;
    const entry = this.folders.get(collectionId);
    if (!f || !entry) return null;
    try {
      const folder = await f.read(entry.ref);
      if (folder.collection.id !== collectionId) {
        // hitop.yaml now names another collection: replace the old one.
        this.closeFolder(collectionId);
        this.collections = this.collections.filter((c) => c.id !== collectionId);
      }
      const col = await this.attachFolder(entry.ref, folder);
      if (this.view.collectionId === collectionId) this.view = { ...this.view, collectionId: col.id };
      this.emit();
      return col.id;
    } catch (e) {
      this.notify(`Couldn't read ${entry.label} again: ${errorText(e)}`);
      return null;
    }
  }

  // A collection as a .zip of its folder. A folder collection takes its own
  // environments; any other takes the app's. Secret values are never in it.
  // The collection's documentation as one HTML page (as `hitop docs`): saved or downloaded.
  async exportDocs(collectionId) {
    const col = this.collection(collectionId);
    if (!col) return;
    try {
      const page = (await loadFormat()).collectionDocs(col);
      const name = `${(col.name || "collection").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "collection"}-docs.html`;
      if (await platform().saveFile(name, page, "text/html")) this.notify(`Saved ${name}: open it in a browser or publish it anywhere`);
    } catch (e) {
      this.notify(`Couldn't make the docs page of ${col.name}: ${errorText(e)}`);
    }
  }

  async exportZip(collectionId) {
    const col = this.collection(collectionId);
    if (!col) return;
    const content = this.folders.has(collectionId)
      ? this.folderContent(collectionId)
      : { collection: col, environments: this.environments.filter((e) => !this.folderEnvs.has(e.id)).map(withoutSecretValues) };
    try {
      const { name, bytes } = await folderToZip(content);
      if (await platform().saveFile(name, bytes, "application/zip")) this.notify(`Exported ${name}`);
    } catch (e) {
      this.notify(`Couldn't export ${col.name}: ${errorText(e)}`);
    }
  }

  // Adds the collection in a .zip of a folder, and its environments with
  // names you don't have yet (secrets come without values).
  async importZip(file) {
    let folder;
    try {
      folder = await zipToFolder(new Uint8Array(await file.arrayBuffer()), file.name);
    } catch (e) {
      this.notify(`Couldn't import ${file.name}: ${errorText(e)}`);
      return null;
    }
    const col = this.inOrg(folder.collection);
    if (this.collection(col.id)) col.id = generateUniqueId();
    this.collections = [...this.collections, col];
    const names = new Set(this.environments.map((e) => e.name));
    const added = (folder.environments || [])
      .filter((e) => !names.has(e.name))
      .map((e) => ({ ...e, id: crypto.randomUUID(), variables: { ...e.variables, ...Object.fromEntries(secretNames(e).map((n) => [n, ""])) } }));
    this.environments = [...this.environments, ...added];
    this.view = { kind: "collection", collectionId: col.id };
    this.emit();
    this.notify(`Imported ${col.name}${added.length ? ` and ${added.length} environment${added.length === 1 ? "" : "s"}` : ""}`);
    return col;
  }

  async runAll() {
    const started = performance.now();
    let total = 0;
    let ok = 0;
    let ran = 0;
    let planned = 0;
    let assertions = 0;
    let assertionsFailed = 0;
    this.run = { collectionId: null, results: [], running: true, completed: 0, summary: null };
    this.emit();
    for (const col of this.visibleCollections) {
      if (!col.requests?.length) continue;
      ran++;
      planned += col.requests.length;
      const results = await this.runCollection(col.id);
      total += results.length;
      ok += results.filter((r) => r.success).length;
      assertions += results.reduce((n, r) => n + (r.tests?.length ?? 0), 0);
      assertionsFailed += results.reduce((n, r) => n + (r.tests?.filter((t) => !t.pass).length ?? 0), 0);
      this.run = { ...this.run, running: true, completed: this.run.completed + 1 };
    }
    this.run = {
      collectionId: null,
      results: [],
      running: false,
      completed: 0,
      summary: { totalCollections: ran, plannedRequests: planned, totalRequests: total, successCount: ok, failedCount: total - ok, assertions, assertionsFailed, duration: Math.round(performance.now() - started) },
    };
    this.emit();
  }
}

export { generateUniqueId };
