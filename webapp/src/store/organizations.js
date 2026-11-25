// The store's organizations (a part of Store, src/store.js).
import { ORG_KEY } from "./shared.js";

export const OrganizationsPart = (Base) =>
  class extends Base {
  /** Signed in: the account's orgs (`GET /v1/orgs`) and its shared workspaces' orgs. */
  setOrgs(orgs, workspaceOrgs = new Map(), workspaceRoles = new Map()) {
    this.orgs = orgs ?? [];
    this.personalOrg = this.orgs.find((o) => o.personal)?.id ?? this.orgs[0]?.id ?? null;
    this.workspaceOrgs = workspaceOrgs;
    // FV1: this account's role per shared workspace (canTakeRequest: viewers add nothing).
    this.workspaceRoles = workspaceRoles;
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

  /** U49: an org's usage and limits (orgs.js loadUsage), or null to forget it. */
  setOrgUsage(org, usage) {
    this.orgUsage ??= new Map();
    if (usage) this.orgUsage.set(org, usage);
    else this.orgUsage.delete(org);
    this.emit();
  }

  clearOrgs() {
    this.orgUsage?.clear();
    if (!this.orgs.length && !this.currentOrg) return;
    this.orgs = [];
    this.personalOrg = null;
    this.currentOrg = null;
    this.workspaceOrgs = new Map();
    this.workspaceRoles = new Map();
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
  };
