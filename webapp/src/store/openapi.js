// The store's OpenAPI (a part of Store, src/store.js).
import tabService from "../../../shared/services/tabService.js";
import { NO_RESPONSE } from "./shared.js";
import { emptyAuth } from "../features.js";
import { generateUniqueId } from "../../../shared/services/idGenerator.js";
import { isBlankRequest, requestAutoName } from "../new-request.js";
import { loadFormat } from "../wasm/format.js";
import { platform } from "../platform/index.js";

export const OpenapiPart = (Base) =>
  class extends Base {
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

  // A spec's problems (hitop-core's `openapi lint`): { problems, errors, warnings }.
  async lintOpenapi(text) {
    return (await loadFormat()).lintOpenapi(text);
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
    this.closeTabsOf((t) => t.loadedCollectionId === collectionId && t.loadedRequestId != null && !keep.has(t.loadedRequestId));
    this.emit();
    const d = preview.diff;
    this.notify(`Synced ${preview.collection.name}: ${d.added.length} added, ${d.changed.length} changed, ${d.removed.length} removed${d.pruned && d.removed.length ? " (deleted)" : ""}`);
  }

  // FV1: a history item opens as a new request in My requests, filled in from the item and not
  // saved yet (the tab shows it as unsaved; closing it unsaved takes it out again).
  // History opens in one preview tab (its title in italics): opening another entry replaces it,
  // so browsing History doesn't leave a new unsaved request in the tree per entry. Editing it (or
  // saving) makes it a tab of its own; closing it untouched takes its request out again.
  openHistoryItem(item) {
    const preview = this.tabs.findIndex((t) => t.historyPreview && this.loadedRequest(t) && isBlankRequest(this.loadedRequest(t)));
    if (preview !== -1) {
      this.currentTab = preview;
      this.view = { kind: "request", collectionId: null };
      this.tabs = tabService.updateTab(this.tabs, preview, { ...NO_RESPONSE });
    } else {
      this.newTab(this.defaultCollection());
      this.tabs = tabService.updateTab(this.tabs, this.currentTab, { historyPreview: true });
    }
    this.tabs = tabService.updateTab(this.tabs, this.currentTab, {
      url: item.url,
      method: item.method,
      headers: item.headers?.length ? item.headers : [{ name: "", value: "" }],
      requestBody: item.body || "",
      preRequestScript: item.preRequestScript || "",
      postRequestScript: item.postRequestScript || "",
      auth: item.auth ? { ...emptyAuth(), ...item.auth } : emptyAuth(),
    });
    this.tabs = tabService.updateTab(this.tabs, this.currentTab, { title: requestAutoName(this.tabs[this.currentTab]) });
    // UX2: with the response History kept (status, headers, body up to 100 kB), shown as it was.
    const r = item.response;
    if (r && r.status != null) {
      const ct = Object.entries(r.headers || {}).find(([k]) => k.toLowerCase() === "content-type")?.[1] ?? "";
      let body = r.body ?? "";
      let type = /json/i.test(ct) ? "json" : /html/i.test(ct) ? "html" : /xml/i.test(ct) ? "xml" : "text";
      if (type === "json") {
        try {
          body = JSON.stringify(JSON.parse(body), null, 2);
        } catch {
          type = "text";
        }
      }
      if (r.binary) type = "binary";
      this.tabs = tabService.updateTab(this.tabs, this.currentTab, {
        response: body,
        responseType: type,
        responseSize: r.binary?.size ?? null,
        // A binary body isn't kept in History (its size and digest are): the view says to send again.
        responseCut: Boolean(r.binary),
        statusCode: r.status,
        responseHeaders: r.headers || {},
        durationMs: r.durationMs ?? null,
      });
    }
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
  };
