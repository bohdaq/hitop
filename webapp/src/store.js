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
  urlEncodedBody,
} from "./features.js";
import { platform } from "./platform/index.js";
import { fillVariables, forWebApp, isPostmanCollection, loadShim, runScript, scriptScans, usesPostmanApi } from "./pm.js";
import pmShimText from "../../shared/pm-shim.js?raw";
import { loadFormat } from "./wasm/format.js";
import { folderToZip, zipToFolder } from "./zip.js";

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

function loadCookies() {
  try {
    return { cookies: JSON.parse(localStorage.getItem(COOKIES_KEY) || "[]"), enabled: localStorage.getItem(COOKIE_JAR_KEY) !== "off" };
  } catch {
    return { cookies: [], enabled: true };
  }
}

// Request fields the web app adds; saved with requests and carried by tabs.
const EXTRA_FIELDS = (r) => ({
  bodyMode: r.bodyMode || "raw",
  formFields: r.formFields || [],
  disabledParams: r.disabledParams || [],
});

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
      return { data: JSON.stringify(JSON.parse(text), null, 2), type: "json" };
    } catch {
      return { data: text, type: "text" };
    }
  }
  if (contentType.includes("html")) return { data: text, type: "html" };
  if (contentType.includes("xml")) return { data: text, type: "xml" };
  return { data: text, type: "text" };
}

// Drops keys set to undefined (a cleared bodyType), so they aren't saved.
const clean = (r) => Object.fromEntries(Object.entries(r).filter(([, v]) => v !== undefined));

const headersObject = (headers) => {
  const out = {};
  for (const h of headers || []) if (h.name && h.value) out[h.name] = h.value;
  return out;
};

function autoTitle(tab) {
  if (tab.loadedRequestId) return tab.title;
  try {
    return `${tab.method} ${new URL(tab.url).pathname || "/"}`;
  } catch {
    return "New request";
  }
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
    // Globals scripts set (pm.globals.set): app-wide (folder collections: their own), until reload.
    this.scriptGlobals = {};
    const jar = loadCookies();
    // The desktop keeps the jar in Rust (hitop-core), the web app in localStorage.
    this.cookies = platform().cookies ? platform().cookies.load() : jar.cookies;
    this.cookieJarEnabled = jar.enabled;
    // Files picked for multipart fields, by field id. Never stored: a page
    // reload asks for them again.
    this.files = new Map();
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
    this.ready = this.restoreFolders();
  }

  // --- plumbing -----------------------------------------------------------

  emit() {
    // Views get the store by reference; this changing number re-renders them.
    this.version = (this.version ?? 0) + 1;
    // Saving serializes everything, responses included (up to 5 MB), so it
    // runs at most every 250 ms rather than on each keystroke.
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => this.save(), 250);
    this.dispatchEvent(new Event("change"));
  }

  save() {
    clearTimeout(this.saveTimer);
    const st = storage();
    st.saveCollections(this.collections);
    st.saveTabs(this.tabs);
    st.saveCurrentTab(this.currentTab);
    st.saveHistory(this.history);
    st.saveContexts(this.contexts);
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
    let col = this.collections.find((c) => c.name === "Default");
    if (!col) {
      col = collectionService.createCollection("Default");
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
    let tabs = tabService.updateTab(this.tabs, index, updates);
    const t = tabs[index];
    const title = autoTitle(t);
    if (title !== t.title) tabs = tabService.updateTab(tabs, index, { title });
    this.tabs = tabs;
    this.emit();
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

  closeTab(index) {
    if (this.tabs.length <= 1) return;
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
      ...EXTRA_FIELDS(source),
    });
    if (!request.auth) delete request.auth;
    this.collections = collectionService.addRequestToCollection(this.collections, collectionId, request);
    this.tabs = [...this.tabs, tabForRequest(request, collectionId)];
    this.currentTab = this.tabs.length - 1;
    this.emit();
  }

  deleteTabRequest(index) {
    const tab = this.tabs[index];
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
      ...EXTRA_FIELDS(tab),
    };
    const existing = tab.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId);
    // A body type label from a folder is kept only while the body is the same.
    if (existing?.bodyType && existing.body !== data.body) data.bodyType = undefined;
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

  // --- collections ----------------------------------------------------------

  addCollection(name) {
    if (!name.trim()) return;
    this.collections = [...this.collections, collectionService.createCollection(name.trim())];
    this.emit();
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

  reorderCollections(from, to) {
    if (from === to) return;
    this.collections = collectionService.reorderCollections(this.collections, from, to);
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
  // carry over, unsupported pm.* calls by script line), else null.
  async importJson(json) {
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

  // --- environments -----------------------------------------------------------

  get activeEnvironment() {
    return this.environments.find((e) => e.id === this.activeEnvironmentId) ?? null;
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
  variables(collectionId) {
    const out = { ...this.globalsFor(collectionId), ...collectionService.getCollectionVariables(this.collections, collectionId), ...(this.activeEnvironment?.variables ?? {}) };
    for (const [k, v] of Object.entries(this.scriptVariables[collectionId] ?? {})) {
      if (v === null) delete out[k];
      else out[k] = v;
    }
    return out;
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
    const env = { id: crypto.randomUUID(), name: name.trim(), variables: {} };
    this.environments = [...this.environments, env];
    if (!this.activeEnvironmentId) this.activeEnvironmentId = env.id;
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
    const body = source.body ? interpolate(source.body) : source.body;
    const authed = applyAuth(source.auth, { url, headers, body }, interpolate);
    const out = await this.runPreScript(source.preRequestScript, collectionId, source.method, authed.url, authed.headers, authed.body);
    if (!postman) return out;
    const after = this.variables(collectionId);
    const strict = (text) => (text ? interpolateVariables(text, after) : text);
    return { url: strict(out.url), headers: out.headers.map((h) => ({ ...h, name: strict(h.name), value: strict(h.value) })), body: strict(out.body) };
  }

  // --- sending ----------------------------------------------------------------

  // Prepares and sends one request; returns { prepared, res }. Handles form
  // bodies and the cookie jar (proxied requests only: the browser keeps
  // localhost cookies itself and hides Set-Cookie from scripts).
  async dispatch(source, collectionId) {
    let prepared;
    try {
      prepared = await this.prepare(source, collectionId);
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
    }
    const p = platform();
    const proxied = p.usesProxy(prepared.url);
    const jar = p.managesCookies(prepared.url);
    if (jar && this.cookieJarEnabled && !hasHeader("cookie")) {
      const cookie = cookieHeader(this.cookies, prepared.url);
      if (cookie) headers.push({ name: "Cookie", value: cookie });
    }
    const options = { method: source.method, headers: headersObject(headers), ...(p.cookies ? { cookies: this.cookieJarEnabled } : {}) };
    if (body && BODY_METHODS.includes(source.method)) options.body = body;
    let res;
    try {
      res = await p.send(prepared.url, options);
    } catch (e) {
      // fetch says only "Failed to fetch"; for direct requests the usual cause
      // is a server that doesn't allow CORS.
      if (p.name === "web" && !proxied && e instanceof TypeError)
        e.message = "Couldn't reach the server. Requests to localhost and private networks go straight from your browser, so the server must allow CORS from this site; the desktop and mobile apps don't need it.";
      throw e;
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
    const tab = this.tabs[index];
    if (!tab.url.trim()) {
      this.notify("Enter a URL first.");
      return;
    }
    const source = {
      url: tab.url,
      method: tab.method,
      headers: tab.headers,
      body: tab.requestBody,
      preRequestScript: tab.preRequestScript,
      auth: tab.auth,
      bodyMode: tab.bodyMode,
      formFields: tab.formFields,
    };
    this.tabs = tabService.updateTab(this.tabs, index, { loading: true, response: null, statusCode: null, responseHeaders: null, durationMs: null, tests: null });
    this.emit();
    const started = performance.now();
    try {
      const { prepared, res } = await this.dispatch(source, tab.loadedCollectionId);
      const headers = {};
      res.headers.forEach((v, k) => (headers[k] = v));
      const { data, type } = await readResponse(res);
      const durationMs = res.timeMs ?? Math.round(performance.now() - started);
      this.tabs = tabService.updateTab(this.tabs, index, {
        response: data,
        responseType: type,
        statusCode: res.status,
        responseHeaders: headers,
        durationMs,
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
      let tests = [];
      try {
        tests = await this.runPostScript(tab.postRequestScript, tab.loadedCollectionId, data, headers, res.status, durationMs, { ...prepared, method: tab.method });
      } catch (e) {
        tests = [...tests, { name: "Script after", pass: false, detail: e.message }];
        this.notify(`The script after the request failed: ${e.message}`);
      }
      this.tabs = tabService.updateTab(this.tabs, index, { tests });
    } catch (e) {
      if (e.stage === "prepare") {
        this.tabs = tabService.updateTab(this.tabs, index, { loading: false });
        this.notify(`The request wasn't sent: ${e.message}`);
      } else {
        this.tabs = tabService.updateTab(this.tabs, index, { response: `Error: ${e.message}`, responseType: "error", statusCode: null, responseHeaders: null, loading: false });
      }
    }
    this.emit();
  }

  // Runs a collection's requests in order, stopping at the first failure.
  async runCollection(id) {
    const collection = this.collection(id);
    if (!collection?.requests?.length) return [];
    this.run = { ...this.run, collectionId: id, results: [], running: true };
    this.emit();
    const results = [];
    // Secret values of the environment and of the globals this collection sees.
    const own = this.folders.get(id)?.globals ?? this.globals;
    const secrets = [...secretValues(this.activeEnvironment), ...secretValues(own)];
    const push = (r) => {
      r = maskSecrets(r, secrets);
      results.push(r);
      this.run = { ...this.run, results: [...results] };
      this.emit();
    };
    for (let i = 0; i < collection.requests.length; i++) {
      const request = collection.requests[i];
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
        const item = maskSecrets({ ...historyService.createCollectionRunHistoryItem(request, prepared, res, collection.name, i), auth: request.auth }, secrets);
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
    if (!f) return;
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
    if (!f || !col) return;
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
      p.folders
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

  // A collection as a .zip of its folder. A folder collection takes its own
  // environments; any other takes the app's. Secret values are never in it.
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
    const col = folder.collection;
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
    for (const col of this.collections) {
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
