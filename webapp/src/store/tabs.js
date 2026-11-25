// The store's tabs (a part of Store, src/store.js).
import collectionService from "../../../shared/services/collectionService.js";
import tabService from "../../../shared/services/tabService.js";
import { SAVED_FIELDS, autoTitle, tabForRequest, untitledRequest } from "./shared.js";
import { UNTITLED, isBlankRequest } from "../new-request.js";
import { cleanAuth } from "../features.js";
import { requestFile } from "../collection-tree.js";

export const TabsPart = (Base) =>
  class extends Base {
  updateTab(updates, index = this.currentTab) {
    if ("url" in updates && this.tabs[index]) this.urlErrors.delete(this.tabs[index].id);
    // An edit keeps a History preview tab: it is no longer replaced by the next entry opened.
    if (this.tabs[index]?.historyPreview) updates = { ...updates, historyPreview: undefined };
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

  /**
   * UX2 (2): a parsed curl command (curl.js parseCurl) into the open request: method, URL, headers,
   * body and auth as the command says. Returns an undo: the tab's fields as they were.
   */
  applyCurl(r, index = this.currentTab) {
    const t = this.tabs[index];
    if (!t) return null;
    const keys = ["url", "method", "kind", "headers", "requestBody", "bodyMode", "formFields", "auth", "disabledParams"];
    const before = Object.fromEntries(keys.map((k) => [k, t[k]]));
    this.updateTab(
      {
        url: r.url,
        method: r.method,
        kind: undefined,
        headers: r.headers.length ? r.headers : [{ name: "", value: "" }],
        requestBody: r.body,
        bodyMode: r.bodyMode,
        formFields: r.formFields,
        auth: r.auth ?? { type: "inherit" },
        disabledParams: [],
      },
      index,
    );
    const id = t.id;
    return () => this.updateTabById(id, before);
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

  // FV1: a new request lands in a collection at once: an "Untitled" request is added to
  // `collection` (in `folder`) and opens in a tab linked to it. Edits show as unsaved until saved
  // (Mod+S), as for any saved request; its name follows the URL until it is renamed (tab
  // `autoName`); closing it untouched takes it out again (closeTab). A collection that can't take
  // one here (canTakeRequest: a viewer's shared collection, cloud storage full) sends it to My
  // requests. Returns { collectionId, folder, fallback } (fallback: it went to My requests instead).
  newTab(collection = this.contextCollection(), folder = "") {
    let target = collection ?? this.defaultCollection();
    let where = folder;
    let fallback = !this.canTakeRequest(target);
    if (fallback) {
      target = this.defaultCollection();
      // My requests itself full: it still gets it (the save is what the server refuses), no toast.
      if (target.id === collection?.id) fallback = false;
      else where = "";
    }
    const file = where ? requestFile(target.requests || [], where, UNTITLED) : undefined;
    const request = untitledRequest(file ? { file } : {});
    this.collections = collectionService.addRequestToCollection(this.collections, target.id, request);
    this.tabs = [...this.tabs, { ...tabForRequest(request, target.id), autoName: true }];
    this.currentTab = this.tabs.length - 1;
    this.view = { kind: "request", collectionId: null };
    this.emit();
    if (fallback) this.notify(`${collection?.name ?? "That collection"} can't take new requests here, so this one is in ${target.name}`);
    return { collectionId: target.id, folder: where, fallback };
  }

  // A new request's tab closed with nothing ever saved in it: the request goes too.
  dropIfUntouched(tab) {
    if (!tab?.autoName) return;
    const request = this.loadedRequest(tab);
    if (!request || !isBlankRequest(request) || request.name !== UNTITLED) return;
    if (this.tabs.some((t) => t !== tab && t.loadedRequestId === request.id && t.loadedCollectionId === tab.loadedCollectionId)) return;
    this.collections = collectionService.deleteRequestFromCollection(this.collections, tab.loadedCollectionId, request.id);
  }

  /** Moves a tab (the tab strip's drag or Alt+Shift+Arrow); the open tab stays open. */
  moveTab(from, to) {
    const n = this.tabs.length;
    if (from < 0 || to < 0 || from >= n || to >= n) return;
    // DD2: pinned tabs stay before the others.
    const pinned = this.tabs.filter((t, i) => t.pinned && i !== from).length;
    to = this.tabs[from].pinned ? Math.min(to, pinned) : Math.max(to, pinned);
    if (from === to) return;
    const openId = this.tabs[this.currentTab]?.id;
    const tabs = [...this.tabs];
    const [moved] = tabs.splice(from, 1);
    tabs.splice(to, 0, moved);
    this.tabs = tabs;
    this.currentTab = Math.max(0, tabs.findIndex((t) => t.id === openId));
    this.emit();
  }

  /** DD2: a tab pinned (to the end of the pinned ones, which stay first) or unpinned. */
  togglePin(index) {
    const t = this.tabs[index];
    if (!t) return;
    const open = this.tabs[this.currentTab];
    const { pinned: _p, ...rest } = t;
    const next = t.pinned ? rest : { ...t, pinned: true };
    const tabs = this.tabs.filter((_, i) => i !== index);
    tabs.splice(tabs.filter((x) => x.pinned).length, 0, next);
    this.tabs = tabs;
    this.currentTab = Math.max(0, tabs.indexOf(open === t ? next : open));
    this.emit();
  }

  /**
   * DD2: the tabs at `indices` closed, except pinned ones and ones with unsaved changes (those
   * stay: nothing is discarded without asking). → { closed, kept } (kept = unsaved ones left).
   */
  closeTabsAt(indices) {
    let closed = 0;
    let kept = 0;
    for (const i of [...new Set(indices)].sort((a, b) => b - a)) {
      const t = this.tabs[i];
      if (!t || t.pinned || this.tabs.length <= 1) continue;
      if (this.isDirty(t)) {
        kept++;
        continue;
      }
      this.closeTab(i);
      closed++;
    }
    return { closed, kept };
  }

  closeTab(index) {
    if (this.tabs.length <= 1) return;
    this.cancel(this.tabs[index].id);
    this.wsDisconnect(this.tabs[index].id);
    this.grpcCancel(this.tabs[index].id);
    this.dropIfUntouched(this.tabs[index]);
    const { tabs, newCurrentTab } = tabService.closeTab(this.tabs, index, this.currentTab);
    this.tabs = tabs;
    this.currentTab = newCurrentTab;
    this.emit();
  }

  renameTab(index, name) {
    const title = name.trim();
    if (!title) return;
    // A name of one's own: it no longer follows the URL.
    this.tabs = tabService.updateTab(this.tabs, index, { autoName: undefined });
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
      this.emit();
    } else {
      // The last tab: a new request takes its place (in the collection it was in, when it can).
      const col = this.collection(tab.loadedCollectionId);
      this.tabs = [];
      this.newTab(col && this.inCurrentOrg(col) ? col : this.contextCollection());
    }
  }

  // Tabs on requests just deleted (`gone(tab)`: the request, or its whole collection) close; when
  // none is left a new request opens. FV1: no tab is outside a collection.
  closeTabsOf(gone) {
    if (!this.tabs.some(gone)) return;
    const openId = this.tab?.id;
    for (const t of this.tabs.filter(gone)) {
      this.cancel(t.id);
      this.wsDisconnect(t.id);
      this.grpcCancel(t.id);
    }
    this.tabs = this.tabs.filter((t) => !gone(t));
    if (!this.tabs.length) {
      const view = this.view;
      this.newTab(this.contextCollection());
      this.view = view;
      return;
    }
    const at = this.tabs.findIndex((t) => t.id === openId);
    this.currentTab = at === -1 ? Math.min(this.currentTab, this.tabs.length - 1) : at;
  }
  };
