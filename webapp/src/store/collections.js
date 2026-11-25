// The store's collections (a part of Store, src/store.js).
import collectionService from "../../../shared/services/collectionService.js";
import { SAMPLE_NAME, SAMPLE_REQUESTS, markImported } from "../getting-started.js";
import { cleanAuth } from "../features.js";
import { sampleCollection } from "./shared.js";

export const CollectionsPart = (Base) =>
  class extends Base {
  /** A new, empty collection in the current organization; returns it (null for a blank name). */
  addCollection(name) {
    if (!name.trim()) return null;
    const col = this.inOrg(collectionService.createCollection(name.trim()));
    this.collections = [...this.collections, col];
    this.emit();
    return col;
  }

  // A first collection to try: JSONPlaceholder (public, through the proxy on the web), with a
  // variable, a script that passes a value on, and tests.
  addSampleCollection() {
    const col = this.inOrg(sampleCollection());
    this.collections = [...this.collections, col];
    this.view = { kind: "collection", collectionId: col.id };
    markImported(globalThis.localStorage, [col.id]);
    this.emit();
    return col;
  }

  // Nothing of one's own yet: no request with a URL in any shown collection (the sample's own
  // requests don't count), no history.
  get isFresh() {
    const own = (c, r) => r.url && !(c.name === SAMPLE_NAME && SAMPLE_REQUESTS.includes(r.name));
    return !this.history.length && this.visibleCollections.every((c) => (c.requests || []).every((r) => !own(c, r)));
  }

  /** The open tab of a request whose name follows its URL (tab `autoName`): the sidebar shows its title. */
  autoNameTab(r, collectionId) {
    return this.tabs.find((t) => t.autoName && t.loadedRequestId === r.id && t.loadedCollectionId === collectionId) ?? null;
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
    const view = this.view.collectionId === id ? { kind: "collections", collectionId: null } : this.view;
    this.closeTabsOf((t) => t.loadedCollectionId === id);
    this.view = view;
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

  /**
   * Auth and headers every request of the collection (`folder` "") or of one folder gets unless it
   * sets its own: `auth`/`headers` on the collection, `folderDefaults[path]` for a folder (hitop.yaml
   * keeps both; the CLI applies them the same way).
   */
  setDefaults(id, folder, { auth, headers }) {
    const a = cleanAuth(auth);
    const rows = (headers || []).filter((h) => h.name).map((h) => ({ name: h.name, value: h.value ?? "" }));
    this.collections = this.collections.map((c) => {
      if (c.id !== id) return c;
      const next = { ...c };
      if (!folder) {
        if (a && a.type !== "none") next.auth = a;
        else delete next.auth;
        if (rows.length) next.headers = rows;
        else delete next.headers;
        return next;
      }
      const all = { ...(c.folderDefaults || {}) };
      const d = { ...(a ? { auth: a } : {}), ...(rows.length ? { headers: rows } : {}) };
      if (Object.keys(d).length) all[folder] = d;
      else delete all[folder];
      if (Object.keys(all).length) next.folderDefaults = all;
      else delete next.folderDefaults;
      return next;
    });
    this.emit();
  }

  /** Adds `name` with an empty value to the collection's variables (the "Not sent" card's action). */
  defineVariable(id, name) {
    const c = this.collection(id);
    if (!c || Object.prototype.hasOwnProperty.call(c.variables || {}, name)) return;
    this.setVariables(id, { ...(c.variables || {}), [name]: "" });
  }

  setVariables(id, variables) {
    this.collections = collectionService.updateCollectionVariables(this.collections, id, variables);
    this.emit();
  }
  };
