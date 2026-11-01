// App state and every action, without UI. A port of frontend/src/App.jsx's
// logic onto the same services, so collections, tabs, history and contexts
// keep their localStorage keys and shapes: the React app and this one read
// each other's data. Views subscribe with `store.addEventListener("change")`
// and report problems through the `notify` event (never window.alert).
import collectionService from "../../frontend/src/services/collectionService.jsx";
import contextService from "../../frontend/src/services/contextService.jsx";
import historyService from "../../frontend/src/services/historyService.jsx";
import storageService from "../../frontend/src/services/storageService.jsx";
import tabService from "../../frontend/src/services/tabService.jsx";
import { interpolateVariables } from "../../frontend/src/services/variableInterpolation.jsx";
import { sendRequest, usesProxy } from "../../frontend/src/services/transportService.jsx";
import { generateUniqueId } from "../../frontend/src/utils/idGenerator.jsx";
import {
  applyAuth,
  cleanAuth,
  cookieHeader,
  emptyAuth,
  loadEnvironments,
  makeAssert,
  mergeCookies,
  multipartBody,
  parseEnvironments,
  saveEnvironments,
  urlEncodedBody,
} from "./features.js";

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
  const collections = storageService.loadCollections();
  const tabs = storageService.loadTabs();
  const current = storageService.loadCurrentTab();
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
    this.history = storageService.loadHistory() || [];
    this.contexts = storageService.loadContexts() || {};
    const envs = loadEnvironments();
    this.environments = envs.environments;
    this.activeEnvironmentId = this.environments.some((e) => e.id === envs.activeEnvironmentId) ? envs.activeEnvironmentId : null;
    const jar = loadCookies();
    this.cookies = jar.cookies;
    this.cookieJarEnabled = jar.enabled;
    // Files picked for multipart fields, by field id. Never stored: a page
    // reload asks for them again.
    this.files = new Map();
    // What the main area shows: the request tabs, all collections, one
    // collection, or one collection's variables.
    this.view = { kind: "request", collectionId: null };
    this.run = { collectionId: null, results: [], running: false, completed: 0, summary: null };
    addEventListener("pagehide", () => this.save());
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
    storageService.saveCollections(this.collections);
    storageService.saveTabs(this.tabs);
    storageService.saveCurrentTab(this.currentTab);
    storageService.saveHistory(this.history);
    storageService.saveContexts(this.contexts);
    saveEnvironments(this.environments, this.activeEnvironmentId);
    try {
      localStorage.setItem(COOKIES_KEY, JSON.stringify(this.cookies));
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
    const request = collectionService.createRequest(name, {
      url: source.url,
      method: source.method,
      headers: source.headers,
      body: source.requestBody,
      preRequestScript: source.preRequestScript,
      postRequestScript: source.postRequestScript,
      auth: cleanAuth(source.auth),
    });
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
    const existing = tab.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.some((r) => r.id === tab.loadedRequestId);
    if (existing) {
      if (collectionId !== tab.loadedCollectionId) {
        let cols = collectionService.deleteRequestFromCollection(this.collections, tab.loadedCollectionId, tab.loadedRequestId);
        cols = collectionService.addRequestToCollection(cols, collectionId, { id: tab.loadedRequestId, ...data });
        this.collections = cols;
      } else {
        this.collections = collectionService.updateRequestInCollection(this.collections, collectionId, tab.loadedRequestId, data);
      }
      this.tabs = tabService.updateTab(this.tabs, this.currentTab, { loadedCollectionId: collectionId, title });
    } else {
      const request = collectionService.createRequest(title, data);
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

  importJson(json) {
    const imported = collectionService.importCollections(json); // throws on bad JSON
    this.collections = imported;
    const first = imported.find((c) => c.requests?.length);
    this.tabs = first ? [tabForRequest(first.requests[0], first.id)] : [tabService.createNewTab()];
    this.currentTab = 0;
    this.view = { kind: "request", collectionId: null };
    this.emit();
    this.notify(`Imported ${imported.length} collection${imported.length === 1 ? "" : "s"}`);
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

  // The variables a request sees: its collection's, overridden by the
  // active environment's.
  variables(collectionId) {
    return { ...collectionService.getCollectionVariables(this.collections, collectionId), ...(this.activeEnvironment?.variables ?? {}) };
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

  setEnvironmentVariables(id, variables) {
    this.environments = this.environments.map((e) => (e.id === id ? { ...e, variables } : e));
    this.emit();
  }

  deleteEnvironment(id) {
    this.environments = this.environments.filter((e) => e.id !== id);
    if (this.activeEnvironmentId === id) this.activeEnvironmentId = null;
    this.emit();
  }

  importEnvironments(json) {
    const imported = parseEnvironments(json); // throws on bad input
    this.environments = [...this.environments, ...imported];
    this.emit();
    this.notify(`Imported ${imported.length} environment${imported.length === 1 ? "" : "s"}`);
  }

  exportEnvironments() {
    return JSON.stringify(this.environments.map(({ name, variables }) => ({ name, variables })), null, 2);
  }

  // --- scripts --------------------------------------------------------------

  setContext(collectionId, key, value) {
    this.contexts = contextService.updateCollectionContext(this.contexts, collectionId, key, value);
  }

  // Scripts run in this page, as in the React web build (`with(ctx)`); the
  // extension build keeps its sandboxed iframe.
  runPreScript(script, collectionId, method, url, headers, body) {
    if (!script?.trim()) return { url, headers, body };
    const context = contextService.getCollectionContext(this.contexts, collectionId);
    const variables = this.variables(collectionId);
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
    new Function("ctx", `with(ctx) { ${script} }`)(ctx);
    for (const [k, v] of Object.entries(updates)) this.setContext(collectionId, k, v);
    return { url: ctx.url, headers: ctx.headers, body: ctx.body };
  }

  // Returns the assert.* results (an empty list without a script).
  runPostScript(script, collectionId, text, headers, status, durationMs) {
    if (!script?.trim()) return [];
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
    try {
      new Function("ctx", `with(ctx) { ${script} }`)(ctx);
    } finally {
      for (const [k, v] of Object.entries(updates)) this.setContext(collectionId, k, v);
    }
    return results;
  }

  // Interpolates variables, adds auth and runs the pre-request script: the
  // request as sent. Auth comes before the script, so a script can override it.
  prepare(source, collectionId) {
    const variables = this.variables(collectionId);
    const interpolate = (text) => interpolateVariables(text, variables);
    const url = interpolate(source.url);
    const headers = (source.headers || []).map((h) => ({
      name: h.name ? interpolate(h.name) : "",
      value: h.value ? interpolate(h.value) : "",
    }));
    const body = source.body ? interpolate(source.body) : source.body;
    const authed = applyAuth(source.auth, { url, headers, body }, interpolate);
    return this.runPreScript(source.preRequestScript, collectionId, source.method, authed.url, authed.headers, authed.body);
  }

  // --- sending ----------------------------------------------------------------

  // Prepares and sends one request; returns { prepared, res }. Handles form
  // bodies and the cookie jar (proxied requests only: the browser keeps
  // localhost cookies itself and hides Set-Cookie from scripts).
  async dispatch(source, collectionId) {
    let prepared;
    try {
      prepared = this.prepare(source, collectionId);
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
    const proxied = usesProxy(prepared.url);
    if (proxied && this.cookieJarEnabled && !hasHeader("cookie")) {
      const cookie = cookieHeader(this.cookies, prepared.url);
      if (cookie) headers.push({ name: "Cookie", value: cookie });
    }
    const options = { method: source.method, headers: headersObject(headers) };
    if (body && BODY_METHODS.includes(source.method)) options.body = body;
    let res;
    try {
      res = await sendRequest(prepared.url, options);
    } catch (e) {
      // fetch says only "Failed to fetch"; for direct requests the usual cause
      // is a server that doesn't allow CORS.
      if (!proxied && e instanceof TypeError)
        e.message = "Couldn't reach the server. Requests to localhost and private networks go straight from your browser, so the server must allow CORS from this site; the desktop and mobile apps don't need it.";
      throw e;
    }
    if (proxied && this.cookieJarEnabled && res.headers.pairs) {
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
      const { res } = await this.dispatch(source, tab.loadedCollectionId);
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
        tests = this.runPostScript(tab.postRequestScript, tab.loadedCollectionId, data, headers, res.status, durationMs);
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
    const push = (r) => {
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
          tests = this.runPostScript(request.postRequestScript, id, text, headers, res.status, duration);
        } catch (e) {
          tests = [...tests, { name: "Script after", pass: false, detail: e.message }];
        }
        this.history = historyService.addToHistory(this.history, { ...historyService.createCollectionRunHistoryItem(request, prepared, res, collection.name, i), auth: request.auth });
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
