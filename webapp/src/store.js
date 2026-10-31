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
import { sendRequest } from "../../frontend/src/services/transportService.jsx";
import { generateUniqueId } from "../../frontend/src/utils/idGenerator.jsx";

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
export const BODY_METHODS = ["POST", "PUT", "PATCH"];

const emptyRequest = () =>
  collectionService.createRequest("New Request", {
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
    return "New Request";
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
    // What the main area shows: the request tabs, all collections, one
    // collection, or one collection's variables.
    this.view = { kind: "request", collectionId: null };
    this.run = { collectionId: null, results: [], running: false, completed: 0, summary: null };
  }

  // --- plumbing -----------------------------------------------------------

  emit() {
    // Views get the store by reference; this changing number re-renders them.
    this.version = (this.version ?? 0) + 1;
    storageService.saveCollections(this.collections);
    storageService.saveTabs(this.tabs);
    storageService.saveCurrentTab(this.currentTab);
    storageService.saveHistory(this.history);
    storageService.saveContexts(this.contexts);
    this.dispatchEvent(new Event("change"));
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

  show(kind, collectionId = null) {
    this.view = { kind, collectionId };
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

  newTab() {
    const col = this.defaultCollection();
    const request = emptyRequest();
    this.collections = collectionService.addRequestToCollection(this.collections, col.id, request);
    this.tabs = [...this.tabs, tabForRequest(request, col.id)];
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
      this.tabs = [tabService.createNewTab()];
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
    if (!this.collections.length) return this.newTab();
    const col = this.collections[0];
    const request = emptyRequest();
    this.collections = collectionService.addRequestToCollection(this.collections, col.id, request);
    this.tabs = [...this.tabs, tabForRequest(request, col.id)];
    this.currentTab = this.tabs.length - 1;
    this.view = { kind: "request", collectionId: null };
    this.emit();
  }

  // The save dialog's starting values: the loaded request, or a new one.
  saveTarget() {
    const tab = this.tab;
    const request = tab.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId);
    return request
      ? { name: request.name, collectionId: tab.loadedCollectionId, overwrite: true }
      : { name: "", collectionId: (this.collections.find((c) => c.name === "Default") ?? this.collections[0])?.id ?? null, overwrite: false };
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

  reorderRequest(collectionId, requestId, targetRequestId) {
    if (requestId === targetRequestId) return;
    const request = this.collection(collectionId)?.requests.find((r) => r.id === requestId);
    if (!request) return;
    this.collections = collectionService.reorderRequest(this.collections, collectionId, collectionId, request, targetRequestId);
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
      title: item.isCollectionRun ? `${item.collectionName} → ${item.requestName}` : `${item.method} ${item.url}`,
    });
    this.emit();
  }

  clearHistory() {
    this.history = [];
    this.emit();
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
    const variables = collectionService.getCollectionVariables(this.collections, collectionId);
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

  runPostScript(script, collectionId, text, headers, status) {
    if (!script?.trim()) return;
    let parsed = text;
    try {
      parsed = JSON.parse(text);
    } catch {
      // not JSON
    }
    const context = contextService.getCollectionContext(this.contexts, collectionId);
    const variables = collectionService.getCollectionVariables(this.collections, collectionId);
    const updates = {};
    const ctx = {
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
    new Function("ctx", `with(ctx) { ${script} }`)(ctx);
    for (const [k, v] of Object.entries(updates)) this.setContext(collectionId, k, v);
  }

  // Interpolates variables and runs the pre-request script: the request as sent.
  prepare(source, collectionId) {
    const variables = collectionService.getCollectionVariables(this.collections, collectionId);
    const url = interpolateVariables(source.url, variables);
    const headers = (source.headers || []).map((h) => ({
      name: h.name ? interpolateVariables(h.name, variables) : "",
      value: h.value ? interpolateVariables(h.value, variables) : "",
    }));
    const body = source.body ? interpolateVariables(source.body, variables) : source.body;
    return this.runPreScript(source.preRequestScript, collectionId, source.method, url, headers, body);
  }

  // --- sending ----------------------------------------------------------------

  async send() {
    const index = this.currentTab;
    const tab = this.tabs[index];
    if (!tab.url.trim()) {
      this.notify("Enter a URL first.");
      return;
    }
    let prepared;
    try {
      prepared = this.prepare({ url: tab.url, method: tab.method, headers: tab.headers, body: tab.requestBody, preRequestScript: tab.preRequestScript }, tab.loadedCollectionId);
    } catch (e) {
      this.notify(`The request wasn't sent: ${e.message}`);
      return;
    }
    this.tabs = tabService.updateTab(this.tabs, index, { loading: true, response: null, statusCode: null, responseHeaders: null, durationMs: null });
    this.emit();
    const started = performance.now();
    try {
      const options = { method: tab.method, headers: headersObject(prepared.headers) };
      if (prepared.body && BODY_METHODS.includes(tab.method)) options.body = prepared.body;
      const res = await sendRequest(prepared.url, options);
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
      try {
        this.runPostScript(tab.postRequestScript, tab.loadedCollectionId, data, headers, res.status);
      } catch (e) {
        this.notify(`Post-request script failed: ${e.message}`);
      }
    } catch (e) {
      this.tabs = tabService.updateTab(this.tabs, index, { response: `Error: ${e.message}`, responseType: "error", statusCode: null, responseHeaders: null, loading: false });
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
      let prepared;
      try {
        prepared = this.prepare(request, id);
      } catch (e) {
        push({ requestName: request.name, method: request.method, url: request.url, statusCode: "Script error", success: false, response: e.message });
        break;
      }
      try {
        const options = { method: request.method, headers: headersObject(prepared.headers) };
        if (prepared.body && BODY_METHODS.includes(request.method)) options.body = prepared.body;
        const started = performance.now();
        const res = await sendRequest(prepared.url, options);
        const duration = res.timeMs ?? Math.round(performance.now() - started);
        const text = await res.text();
        const headers = {};
        res.headers.forEach((v, k) => (headers[k] = v));
        try {
          this.runPostScript(request.postRequestScript, id, text, headers, res.status);
        } catch (e) {
          this.notify(`Post-request script in "${request.name}" failed: ${e.message}`);
        }
        this.history = historyService.addToHistory(this.history, historyService.createCollectionRunHistoryItem(request, prepared, res, collection.name, i));
        push({ requestName: request.name, method: request.method, url: prepared.url, statusCode: res.status, success: res.ok, response: text.slice(0, 200), duration });
        if (!res.ok) break;
      } catch (e) {
        push({ requestName: request.name, method: request.method, url: request.url, statusCode: "Error", success: false, response: e.message });
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
    this.run = { collectionId: null, results: [], running: true, completed: 0, summary: null };
    this.emit();
    for (const col of this.collections) {
      if (!col.requests?.length) continue;
      ran++;
      const results = await this.runCollection(col.id);
      total += results.length;
      ok += results.filter((r) => r.success).length;
      this.run = { ...this.run, running: true, completed: this.run.completed + 1 };
    }
    this.run = {
      collectionId: null,
      results: [],
      running: false,
      completed: 0,
      summary: { totalCollections: ran, totalRequests: total, successCount: ok, failedCount: total - ok, duration: Math.round(performance.now() - started) },
    };
    this.emit();
  }
}

export { generateUniqueId };
