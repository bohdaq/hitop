// The store's form fields (a part of Store, src/store.js).
import collectionService from "../../../shared/services/collectionService.js";
import { MY_REQUESTS, isMyRequests } from "../new-request.js";

export const FormFieldsPart = (Base) =>
  class extends Base {
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

  /** My requests (or an older "Default"), made when the current org has none. */
  defaultCollection() {
    const shown = this.visibleCollections;
    let col = shown.find((c) => c.name === MY_REQUESTS) ?? shown.find(isMyRequests);
    if (!col) {
      col = this.inOrg(collectionService.createCollection(MY_REQUESTS));
      this.collections = [...this.collections, col];
    }
    return col;
  }

  /**
   * FV1: whether a new request may be added to `c` here. Not a shared collection where this
   * account is a viewer (`/v1/workspaces` role, orgs.js), nor one kept in HITOP Cloud (shared or
   * the library) while its organization's storage is full (`orgUsage`): adding would be refused.
   */
  canTakeRequest(c) {
    if (!c) return false;
    const folder = this.folders.get(c.id);
    const shared = String(folder?.ref ?? "").startsWith("hitop-sync:");
    if (shared && this.workspaceRoles?.get(this.syncClient?.link?.(c.id)?.workspace) === "viewer") return false;
    const lib = this.library;
    const cloud = shared || (!folder && lib?.status?.on && !(lib.state?.localOnly ?? []).includes(String(c.id)));
    if (!cloud) return true;
    const u = this.orgUsage?.get(this.orgOf(c));
    const max = u?.limits?.storageBytes;
    return !(max != null && (u?.usage?.storageBytes ?? 0) >= max);
  }
  };
