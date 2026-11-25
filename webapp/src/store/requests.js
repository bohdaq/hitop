// The store's requests in collections (a part of Store, src/store.js).
import collectionService from "../../../shared/services/collectionService.js";
import tabService from "../../../shared/services/tabService.js";
import { SAVED_FIELDS, clean, tabForRequest } from "./shared.js";
import { UNTITLED, isMyRequests, isPristineSample, requestAutoName } from "../new-request.js";
import { addFolder as addFolderTo, deleteFolder as deleteFolderIn, moveFolder, moveRequest as moveRequestIn, renameStored, treeOf, within } from "../tree-moves.js";
import { cleanAuth } from "../features.js";
import { folderOf as folderOfRequest, folderPaths, renameFolderIn, requestFile, requestTree } from "../collection-tree.js";
import { generateUniqueId } from "../../../shared/services/idGenerator.js";
import { markSaved } from "../getting-started.js";

export const RequestsPart = (Base) =>
  class extends Base {
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

  // The collection a new request goes to: the one on screen (its page, its
  // variables, the open tab's), else the first one shown; My requests is
  // made only when there is none at all.
  contextCollection() {
    const { kind, collectionId } = this.view;
    const shown = this.visibleCollections;
    const byView = kind !== "request" && collectionId != null ? shown.find((c) => c.id === collectionId) : null;
    // UX2 (5): the untouched sample takes a new request only when it is targeted itself (its page,
    // its row's +, its folder); an open sample request or being first in the list doesn't count.
    const usable = (c) => c && !isPristineSample(c);
    const byTab = kind === "request" ? shown.find((c) => c.id === this.tab?.loadedCollectionId) : null;
    return byView ?? (usable(byTab) ? byTab : null) ?? shown.find(usable) ?? this.defaultCollection();
  }

  newRequestInFirstCollection() {
    return this.newTab(this.contextCollection());
  }

  /** A new request in a collection (and folder): the sidebar's "+" and the collection page. */
  newRequestIn(collectionId, folder = "") {
    const collection = this.collection(collectionId) ?? this.contextCollection();
    return this.newTab(collection, folder);
  }

  // The save dialog's starting values: the loaded request, or a new one.
  saveTarget() {
    const tab = this.tab;
    const request = tab.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId);
    // A new request still named after its URL is saved under that name.
    return request
      ? { name: this.savedName(tab, request), collectionId: tab.loadedCollectionId, overwrite: true }
      : { name: "", collectionId: (this.collection(tab.loadedCollectionId) ?? this.collections.find(isMyRequests) ?? this.collections[0])?.id ?? null, overwrite: false, folder: tab.folder || "" };
  }

  /** The name a save gives the tab's request: a new one's follows its URL (tab `autoName`). */
  savedName(tab, request = this.loadedRequest(tab)) {
    return tab?.autoName ? requestAutoName(tab) : request?.name ?? tab?.title ?? UNTITLED;
  }

  /** Where a request can go, for the Move dialog: every shown collection and its folders. */
  moveTargets() {
    const out = [];
    for (const c of this.visibleCollections) {
      out.push({ collectionId: c.id, folder: "", label: c.name });
      for (const path of folderPaths(treeOf(c, requestTree))) out.push({ collectionId: c.id, folder: path, label: `${c.name} › ${path.replace(/\//g, " › ")}` });
    }
    return out;
  }

  /** The request moved to a collection and folder (the Move dialog); open tabs follow it. */
  moveRequestTo(from, requestId, to, folder = "", beforeId = null) {
    return this.moveNode({ kind: "request", from, id: requestId, to, folder, before: beforeId == null ? null : { request: beforeId } }).ok;
  }

  /**
   * DD1: a request or folder moved (or copied) in the tree (tree-moves.js). `node` is { kind:
   * "request", id } or { kind: "folder", path }, from collection `from` to `to`, into `folder`
   * before `before` ({ request } | { folder } | null = the end). Open tabs follow; `undo` puts
   * the collections back as they were. → { ok, error?, path?, undo? }
   */
  moveNode({ kind, from, id, path, to, folder = "", before = null, after = null, copy = false }) {
    const source = this.collection(from);
    const target = this.collection(to);
    if (!source || !target) return { ok: false, error: "That place is gone." };
    if (!copy && this.isReadOnly(source)) return { ok: false, error: `${source.name} is read-only here: nothing in it can move.` };
    const refused = this.cannotTake(target);
    if (refused) return { ok: false, error: refused };
    const before0 = this.collections;
    const r =
      kind === "folder"
        ? moveFolder(this.collections, { from, path, to, parent: folder, before, after, copy, newId: generateUniqueId })
        : moveRequestIn(this.collections, { from, requestId: id, to, folder, before, after, copy, newId: generateUniqueId });
    if (!r.collections) return { ok: false, error: r.error ?? "" };
    this.collections = r.collections;
    // Open tabs of what moved follow it to the other collection (and back on Undo).
    const moved = copy ? new Set() : kind === "folder" ? new Set((source.requests || []).filter((x) => within(folderOfRequest(x), path)).map((x) => x.id)) : new Set([id]);
    const follow = (a, b) => {
      if (a !== b && moved.size) this.tabs = this.tabs.map((t) => (t.loadedCollectionId === a && moved.has(t.loadedRequestId) ? { ...t, loadedCollectionId: b } : t));
    };
    follow(from, to);
    this.emit();
    const moved0 = this.collections;
    return {
      ok: true,
      path: r.path,
      // Only while nothing else changed the collections (a library pull, a save, another move):
      // putting the old ones back then would drop that change. → false when it can't.
      undo: () => {
        if (this.collections !== moved0) return false;
        this.collections = before0;
        follow(to, from);
        this.emit();
        return true;
      },
    };
  }

  /** Why `c` can't take a request (sample, viewer, storage full), or "". */
  cannotTake(c) {
    if (this.isReadOnly(c)) return `${c.name} is read-only here.`;
    if (!this.canTakeRequest(c)) return `${c.name} can't take more requests now (its organization's storage is full).`;
    return "";
  }

  /** A shared collection where you are a viewer. */
  isReadOnly(c) {
    const shared = String(this.folders.get(c?.id)?.ref ?? "").startsWith("hitop-sync:");
    return shared && this.workspaceRoles?.get(this.syncClient?.link?.(c.id)?.workspace) === "viewer";
  }

  /** DD1: a new empty folder `name` under `parent`, kept in the collection (`folders`). → its path */
  addFolder(collectionId, parent, name) {
    const c = this.collection(collectionId);
    if (!c || !String(name ?? "").trim()) return "";
    const { collection, path } = addFolderTo(c, parent, name);
    this.collections = this.collections.map((x) => (x.id === collectionId ? collection : x));
    this.emit();
    return path;
  }

  /** DD1: a folder deleted with its requests and subfolders (their tabs close). → requests removed */
  deleteFolder(collectionId, path) {
    const c = this.collection(collectionId);
    if (!c || !path) return 0;
    const { collection, removed } = deleteFolderIn(c, path);
    this.collections = this.collections.map((x) => (x.id === collectionId ? collection : x));
    const gone = new Set(removed);
    this.closeTabsOf((t) => t.loadedCollectionId === collectionId && gone.has(t.loadedRequestId));
    this.emit();
    return removed.length;
  }

  /** Folder `path` of a collection renamed to `name` (its requests' `file` paths move with it). */
  renameFolder(collectionId, path, name) {
    const base = name.trim().replace(/[\\/]+/g, "-");
    const col = this.collection(collectionId);
    if (!base || !col || !path) return "";
    const at = path.lastIndexOf("/");
    const newPath = at === -1 ? base : `${path.slice(0, at + 1)}${base}`;
    if (newPath === path) return path;
    this.collections = this.collections.map((c) => (c.id === collectionId ? renameStored({ ...c, requests: renameFolderIn(c.requests || [], path, newPath) }, path, newPath) : c));
    this.emit();
    return newPath;
  }

  saveRequest(name, collectionId, folder = this.tab?.folder || "") {
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
    // A new request added to a folder got the file "<folder>/untitled": its first save under a
    // real name gives it a file named after it.
    if (existing && collectionId === tab.loadedCollectionId && title !== UNTITLED && typeof existing.file === "string") {
      const at = existing.file.lastIndexOf("/");
      if (/^untitled(-\d+)?$/.test(existing.file.slice(at + 1))) {
        const others = (this.collection(collectionId)?.requests || []).filter((r) => r.id !== existing.id);
        data.file = requestFile(others, at === -1 ? "" : existing.file.slice(0, at), title);
      }
    }
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
      // A name typed in the Save dialog is one's own: the name stops following the URL.
      this.tabs = tabService.updateTab(this.tabs, this.currentTab, { loadedCollectionId: collectionId, title, autoName: tab.autoName && title === requestAutoName(tab) ? true : undefined });
    } else {
      // A draft made in a folder (the sidebar's "+" on it) gets its file there.
      const target = this.collection(collectionId);
      const file = folder && target ? requestFile(target.requests || [], folder, title) : undefined;
      // createRequest knows the React fields only: the kind, websocket, grpc, graphql and form fields come from data.
      const request = clean({ ...data, ...collectionService.createRequest(title, data), ...(file ? { file } : {}) });
      this.collections = collectionService.addRequestToCollection(this.collections, collectionId, request);
      this.tabs = tabService.updateTab(this.tabs, this.currentTab, { loadedRequestId: request.id, loadedCollectionId: collectionId, title, folder: undefined });
    }
    try {
      markSaved(globalThis.localStorage);
    } catch {
      // storage blocked: the card decides from the collections alone
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
    this.tabs = this.tabs.map((t) => (t.loadedRequestId === requestId && t.loadedCollectionId === collectionId ? { ...t, title, autoName: undefined } : t));
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
    this.closeTabsOf((t) => t.loadedRequestId === request.id && t.loadedCollectionId === collectionId);
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
  };
