// hitop sync in the web app and the desktop app: a collection shared through a hitop sync server
// (sync-server/, AGPL), end to end encrypted. The same decisions and the same cryptography as
// `hitop sync` in the CLI: hitop-core's sync plan and hitop-sync-crypto, compiled to WASM
// (src/wasm/format.js `sync`). This file is the transport and the bookkeeping.
//
// A shared collection is a folder collection (store.js folders) whose folder lives either on
// disk (desktop, or a folder opened with the File System Access API) or, for a collection that
// had no folder, in this app's storage under the ref "hitop-sync:<workspace>". The sync unit is
// the folder's files (hitop.yaml, requests/, environments/, globals.yaml); secret values are
// never in them, only secret names.
//
// What stays on this device:
//   hitop_sync_account     { server, email, userId, keyId, deviceName }
//   the session and the device's X25519 secret key: platform().folderSecrets under
//                          "hitop-sync/<host>" (desktop: the OS keychain; web: this browser)
//   hitop_sync             { pins: { keyId: publicKey }, links: { collectionId: { workspace } },
//                            states: { workspace: hitop-core sync State } }
//   hitop_sync_files:<ws>  { path: text } for collections kept in app storage
import { loadFormat } from "./wasm/format.js";
import { platform } from "./platform/index.js";

export const DEFAULT_SERVER = "https://sync.hitop.rws8.tech";
const ACCOUNT_KEY = "hitop_sync_account";
const STATE_KEY = "hitop_sync";
const FILES_KEY = (ws) => `hitop_sync_files:${ws}`;
export const SYNC_REF = "hitop-sync:";
export const isSyncRef = (ref) => typeof ref === "string" && ref.startsWith(SYNC_REF);

const readJson = (key, fallback) => {
  try {
    return JSON.parse(localStorage.getItem(key) || "null") ?? fallback;
  } catch {
    return fallback;
  }
};
const writeJson = (key, value) => localStorage.setItem(key, JSON.stringify(value));
const host = (server) => {
  try {
    return new URL(server).host;
  } catch {
    return server;
  }
};
const secretAccount = (server) => `hitop-sync/${host(server)}`;

export class SyncError extends Error {
  constructor(status, body) {
    const hint = status === 401 ? " (sign in again)" : status === 403 ? " (your role in this workspace doesn't allow it)" : "";
    super(`${body?.message ?? `HTTP ${status}`}${hint}`);
    this.status = status;
    this.code = body?.error;
    this.body = body;
  }
}

// The collection files kept in app storage (collections shared without a folder).
export const syncFiles = {
  get: (ws) => readJson(FILES_KEY(ws), {}),
  set: (ws, files) => writeJson(FILES_KEY(ws), files),
  drop: (ws) => localStorage.removeItem(FILES_KEY(ws)),
};

// Folders under "hitop-sync:<ws>" refs next to the platform's own (disk, File System Access).
export function withSyncFolders(base) {
  const sync = {
    async read(ref) {
      const ws = ref.slice(SYNC_REF.length);
      const files = syncFiles.get(ws);
      if (!files["hitop.yaml"]) throw new Error("not a collection folder: the shared collection has no hitop.yaml yet");
      return (await loadFormat()).readFiles(files, "Shared collection");
    },
    async write(ref, folder) {
      const ws = ref.slice(SYNC_REF.length);
      const old = syncFiles.get(ws);
      const files = (await loadFormat()).toFiles(folder);
      // Without `globals` the globals file stays as it is (as on disk).
      for (const g of ["globals.yaml", "globals.yml"]) if (!folder.globals && old[g] != null) files[g] = old[g];
      // Files sync doesn't carry (none today) stay too.
      syncFiles.set(ws, files);
    },
  };
  const links = () => readJson(STATE_KEY, {}).links ?? {};
  return {
    pick: base?.pick,
    read: (ref) => (isSyncRef(ref) ? sync.read(ref) : base.read(ref)),
    write: (ref, folder) => (isSyncRef(ref) ? sync.write(ref, folder) : base.write(ref, folder)),
    label: (ref) => (isSyncRef(ref) ? "shared (hitop sync)" : base.label(ref)),
    async restore() {
      const own = base ? await base.restore() : [];
      const shared = Object.entries(links())
        .filter(([, l]) => l.ref && isSyncRef(l.ref))
        .map(([collectionId, l]) => ({ collectionId: Number.isNaN(+collectionId) ? collectionId : +collectionId, ref: l.ref, connected: true }));
      return [...own.filter((o) => !shared.some((s) => s.collectionId === o.collectionId)), ...shared];
    },
    reconnect: (ref) => (isSyncRef(ref) ? Promise.resolve(true) : base.reconnect(ref)),
    remember: (collectionId, ref) => (isSyncRef(ref) ? undefined : base?.remember(collectionId, ref)),
    forget: (collectionId) => base?.forget(collectionId),
  };
}

export class SyncClient {
  constructor(store) {
    this.store = store;
    this.status = new Map(); // collectionId → { busy, message, error, at }
  }

  // --- local bookkeeping ----------------------------------------------------------------

  get data() {
    const d = readJson(STATE_KEY, {});
    return { pins: d.pins ?? {}, links: d.links ?? {}, states: d.states ?? {} };
  }

  update(fn) {
    const d = this.data;
    fn(d);
    writeJson(STATE_KEY, d);
  }

  account() {
    return readJson(ACCOUNT_KEY, null);
  }

  link(collectionId) {
    return this.data.links[collectionId] ?? null;
  }

  state(ws) {
    return this.data.states[ws] ?? { seq: 0, entries: {}, conflicts: {}, losers: {} };
  }

  saveState(ws, state) {
    this.update((d) => (d.states[ws] = state));
  }

  async secret() {
    const a = this.account();
    if (!a) return null;
    // Kept in memory once read: the desktop's keychain writes don't wait.
    if (this.cached?.server === a.server) return this.cached.value;
    const acc = secretAccount(a.server);
    const v = (await platform().folderSecrets.load([acc]))[acc];
    let value = null;
    try {
      value = v ? JSON.parse(v) : null;
    } catch {
      value = null;
    }
    if (value) this.cached = { server: a.server, value };
    return value;
  }

  async saveSecret(server, value) {
    this.cached = value ? { server, value } : null;
    await platform().folderSecrets.save({ [secretAccount(server)]: value ? JSON.stringify(value) : null });
  }

  // --- HTTP ---------------------------------------------------------------------------

  async http(server, method, path, body, session) {
    const url = server.replace(/\/+$/, "") + path;
    const headers = { "Content-Type": "application/json", ...(session ? { Authorization: `Bearer ${session}` } : {}) };
    const opts = { method, headers, ...(body === undefined ? {} : { body: JSON.stringify(body) }) };
    // The desktop goes through Rust (no CORS); the browser straight to the server (its CORS
    // allows the web app), never through the web proxy.
    const p = platform();
    const res = p.name === "tauri" ? await p.send(url, opts) : await fetch(url, opts);
    const text = await res.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = null;
    }
    return { status: res.status, json };
  }

  async call(method, path, body, { allow = [] } = {}) {
    const a = this.account();
    if (!a) throw new Error("Sign in to hitop sync first");
    const s = await this.secret();
    const r = await this.http(a.server, method, path, body, s?.session);
    if ((r.status >= 200 && r.status < 300) || allow.includes(r.status)) return r;
    throw new SyncError(r.status, r.json);
  }

  async api(method, path, body) {
    return (await this.call(method, path, body)).json;
  }

  // --- account ------------------------------------------------------------------------

  async startSignIn(server, email) {
    server = (server || DEFAULT_SERVER).trim().replace(/\/+$/, "");
    const u = new URL(server);
    if (u.protocol !== "https:" && !["localhost", "127.0.0.1", "[::1]"].includes(u.hostname)) throw new Error("The sync server must be https:// (http:// only for localhost)");
    const r = await this.http(server, "POST", "/v1/auth/email", { email });
    if (r.status !== 202) throw new SyncError(r.status, r.json);
    this.pending = { server, email };
  }

  async finishSignIn(code, deviceName) {
    const { server, email } = this.pending ?? {};
    if (!server) throw new Error("Ask for a code first");
    code = String(code).trim().split("#").pop();
    const r = await this.http(server, "POST", "/v1/auth/email/verify", { token: code });
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    const session = r.json.session;
    const w = (await loadFormat()).sync;
    // The same account on this device again keeps its key.
    const old = this.account();
    const oldSecret = old && old.userId === r.json.user.id && old.server === server ? await this.secret() : null;
    const me = await this.http(server, "GET", "/v1/me", undefined, session);
    let keyId, secret, name;
    if (oldSecret && me.json.keys.some((k) => k.id === old.keyId)) {
      ({ keyId, deviceName: name } = old);
      secret = oldSecret.secret;
    } else {
      const k = w.keygen();
      name = deviceName || (platform().name === "tauri" ? "HITOP desktop" : `HITOP web (${navigator.userAgent.match(/(Firefox|Edg|Chrome|Safari)/)?.[1] ?? "browser"})`);
      const added = await this.http(server, "POST", "/v1/me/keys", { name, publicKey: k.publicKey }, session);
      if (added.status !== 201) throw new SyncError(added.status, added.json);
      keyId = added.json.id;
      secret = k.secret;
    }
    await this.saveSecret(server, { session, secret });
    writeJson(ACCOUNT_KEY, { server, email: r.json.user.email ?? email, userId: r.json.user.id, keyId, deviceName: name });
    this.update((d) => (d.pins[keyId] = w.publicKey(secret)));
    this.pending = null;
  }

  async signOut() {
    const a = this.account();
    if (!a) return;
    await this.call("POST", "/v1/auth/logout", undefined, { allow: [401] }).catch(() => {});
    await this.saveSecret(a.server, null);
    localStorage.removeItem(ACCOUNT_KEY);
  }

  async fingerprint() {
    const s = await this.secret();
    if (!s) return "";
    const w = (await loadFormat()).sync;
    return w.fingerprint(w.publicKey(s.secret));
  }

  // --- trust on first use ---------------------------------------------------------------

  async fp(publicKey) {
    return (await loadFormat()).sync.fingerprint(publicKey);
  }

  // "trusted" | "changed" | "new"
  trustOf(keyId, publicKey) {
    const p = this.data.pins[keyId];
    return p === publicKey ? "trusted" : p ? "changed" : "new";
  }

  trust(keys) {
    this.update((d) => {
      for (const k of keys) if (!d.pins[k.id]) d.pins[k.id] = k.publicKey;
    });
  }

  // Every key must be trusted; a changed key is refused (that's what a substituted key looks like).
  checkTrusted(keys, who = "") {
    const bad = keys.filter((k) => this.trustOf(k.id, k.publicKey) !== "trusted");
    if (bad.some((k) => this.trustOf(k.id, k.publicKey) === "changed")) throw new Error(`A device key of ${who || "a member"} changed since you trusted it: not sharing the workspace key with it`);
    if (bad.length) throw new Error(`Check and trust the device keys of ${who || "every member"} first (compare the fingerprints with them)`);
  }

  // --- keys -------------------------------------------------------------------------------

  async workspaceKeys(ws) {
    const a = this.account();
    const s = await this.secret();
    const w = (await loadFormat()).sync;
    const r = await this.api("GET", `/v1/workspaces/${ws}/wraps?keyId=${a.keyId}`);
    const keys = {};
    for (const x of r.wraps) keys[x.epoch] = w.unwrap(s.secret, ws, x.epoch, x.wrapped);
    if (!keys[r.epoch]) throw new Error(`This device has no key for the workspace's current epoch yet: ask a member to open Sync → Members (it hands the key to new devices)`);
    return { epoch: r.epoch, keys };
  }

  async wrapsFor(keys, ws, epoch, key) {
    const w = (await loadFormat()).sync;
    return keys.map((k) => ({ keyId: k.id, wrapped: w.wrap(k.publicKey, ws, epoch, key) }));
  }

  // --- the files of a collection --------------------------------------------------------

  async localFiles(collectionId) {
    const l = this.link(collectionId);
    if (l?.ref && isSyncRef(l.ref)) {
      await this.store.flushFolder(collectionId);
      return syncFiles.get(l.workspace);
    }
    await this.store.flushFolder(collectionId);
    return (await loadFormat()).toFiles(this.store.folderContent(collectionId));
  }

  async applyFiles(collectionId, files) {
    const l = this.link(collectionId);
    const entry = this.store.folderOf(collectionId);
    if (l?.ref && isSyncRef(l.ref)) syncFiles.set(l.workspace, files);
    else if (entry) await platform().folders.write(entry.ref, (await loadFormat()).readFiles(files, this.store.collection(collectionId)?.name ?? "Shared"));
    const id = await this.store.reloadFolder(collectionId);
    if (id != null && id !== collectionId) this.update((d) => ((d.links[id] = d.links[collectionId]), delete d.links[collectionId]));
    return id ?? collectionId;
  }

  async changes(ws, since, keys) {
    const w = (await loadFormat()).sync;
    const out = [];
    let seq = since;
    let cursor = since;
    for (;;) {
      const r = await this.api("GET", `/v1/workspaces/${ws}/changes?since=${cursor}`);
      seq = r.seq;
      for (const c of r.changes) {
        cursor = Math.max(cursor, c.seq);
        if (c.deleted) {
          out.push({ blob: c.id, version: c.version, vv: c.vv, deleted: true });
          continue;
        }
        const b = await this.api("GET", `/v1/workspaces/${ws}/blobs/${c.id}`);
        const epoch = w.blobEpoch(b.content);
        if (!keys[epoch]) throw new Error(`A file is under key epoch ${epoch}, which this device doesn't have`);
        const { path, text } = w.open(keys[epoch], ws, c.id, b.version, b.content);
        out.push({ blob: c.id, version: b.version, vv: b.vv, deleted: false, path, text });
      }
      if (!r.more || !r.changes.length) break;
    }
    return { remote: out, seq };
  }

  async put(ws, push, epoch, key) {
    const w = (await loadFormat()).sync;
    const body = { version: push.version, vv: push.vv, epoch, ...(push.text == null ? { deleted: true } : { content: w.seal(key, ws, push.blob, push.version, epoch, push.path, push.text) }) };
    const r = await this.call("PUT", `/v1/workspaces/${ws}/blobs/${push.blob}`, body, { allow: [409] });
    return r.status === 409 ? r.json?.error ?? "conflict" : null;
  }

  // Pull and push one shared collection. Returns { pulled, removed, pushed, merged, conflicts }.
  async syncNow(collectionId, { push = true } = {}) {
    const l = this.link(collectionId);
    if (!l) throw new Error("This collection isn't shared");
    const ws = l.workspace;
    const me = this.account().keyId;
    const w = (await loadFormat()).sync;
    let { epoch, keys } = await this.workspaceKeys(ws);
    for (let attempt = 0; attempt < 5; attempt++) {
      let state = this.state(ws);
      const { remote, seq } = await this.changes(ws, state.seq, keys);
      const local = await this.localFiles(collectionId);
      const plan = w.plan(local, state, remote, me);
      plan.state.seq = seq;
      if (Object.keys(plan.write).length || plan.remove.length) {
        const files = { ...local, ...plan.write };
        for (const p of plan.remove) delete files[p];
        collectionId = await this.applyFiles(collectionId, files);
      }
      state = plan.state;
      this.saveState(ws, state);
      const result = { pulled: Object.keys(plan.write), removed: plan.remove, merged: plan.merged, conflicts: plan.conflicts, pushed: [], collectionId };
      if (!push) return result;
      let again = false;
      for (const p of plan.push) {
        const refused = await this.put(ws, p, epoch, keys[epoch]);
        if (refused) {
          if (refused === "stale_epoch") ({ epoch, keys } = await this.workspaceKeys(ws));
          again = true;
          break;
        }
        state = w.pushed(state, p);
        this.saveState(ws, state);
        if (p.path) result.pushed.push(p.path);
      }
      if (!again) return result;
    }
    throw new Error("The workspace kept changing while syncing; try again");
  }

  // [{ path, base, mine, theirs }]
  async conflicts(collectionId) {
    const l = this.link(collectionId);
    if (!l) return [];
    const st = this.state(l.workspace);
    const paths = Object.keys(st.conflicts ?? {});
    if (!paths.length) return [];
    const local = await this.localFiles(collectionId);
    return paths.map((path) => ({ path, base: st.conflicts[path].base ?? null, theirs: st.conflicts[path].theirs ?? null, mine: local[path] ?? null }));
  }

  async resolve(collectionId, path, keepMine) {
    const l = this.link(collectionId);
    const w = (await loadFormat()).sync;
    const { state, resolution } = w.resolve(this.state(l.workspace), path, keepMine);
    this.saveState(l.workspace, state);
    if (resolution.action !== "keepMine") {
      const files = await this.localFiles(collectionId);
      if (resolution.action === "write") files[path] = resolution.text;
      else delete files[path];
      collectionId = await this.applyFiles(collectionId, files);
    }
    // Keeping mine is pushed by the next sync; do it now.
    return this.syncNow(collectionId);
  }

  // --- workspaces -----------------------------------------------------------------------

  async workspaces() {
    return (await this.api("GET", "/v1/workspaces")).workspaces;
  }

  // Shares a collection: a new workspace, epoch 1's key wrapped for every device of this account.
  async share(collectionId) {
    const w = (await loadFormat()).sync;
    const me = await this.api("GET", "/v1/me");
    this.checkTrusted(me.keys, "your other devices");
    const ws = w.randomId();
    const key = w.newKey();
    await this.api("POST", "/v1/workspaces", { id: ws, wraps: await this.wrapsFor(me.keys, ws, 1, key) });
    const entry = this.store.folderOf(collectionId);
    let ref = entry?.ref;
    if (!entry) {
      // No folder: keep its files in app storage from now on.
      ref = `${SYNC_REF}${ws}`;
      const files = (await loadFormat()).toFiles(this.store.folderContent(collectionId));
      syncFiles.set(ws, files);
      this.update((d) => (d.links[collectionId] = { workspace: ws, ref }));
      await this.store.attachFolder(ref, (await loadFormat()).readFiles(files, this.store.collection(collectionId).name), { remember: false });
    } else {
      this.update((d) => (d.links[collectionId] = { workspace: ws }));
    }
    return { workspace: ws, ...(await this.syncNow(collectionId)) };
  }

  // Opens a workspace you're a member of as a new collection.
  async join(ws) {
    const w = (await loadFormat()).sync;
    const { keys } = await this.workspaceKeys(ws);
    const { remote, seq } = await this.changes(ws, 0, keys);
    const plan = w.plan({}, { seq: 0, entries: {}, conflicts: {}, losers: {} }, remote, this.account().keyId);
    plan.state.seq = seq;
    if (!plan.write["hitop.yaml"]) throw new Error("This workspace has no collection in it yet");
    const ref = `${SYNC_REF}${ws}`;
    syncFiles.set(ws, plan.write);
    this.saveState(ws, plan.state);
    const folder = (await loadFormat()).readFiles(plan.write, "Shared collection");
    this.update((d) => (d.links[folder.collection.id] = { workspace: ws, ref }));
    const col = await this.store.attachFolder(ref, folder, { remember: false });
    return col.id;
  }

  // Stops syncing (the workspace and its files on the server stay).
  unlink(collectionId) {
    const l = this.link(collectionId);
    if (!l) return;
    this.update((d) => {
      delete d.links[collectionId];
      delete d.states[l.workspace];
    });
  }

  // --- teams ----------------------------------------------------------------------------

  async members(collectionId) {
    const ws = this.link(collectionId).workspace;
    const info = await this.api("GET", `/v1/workspaces/${ws}`);
    const invites = info.role === "owner" ? (await this.api("GET", `/v1/workspaces/${ws}/invites`)).invites : [];
    const withFp = async (keys) => Promise.all((keys ?? []).map(async (k) => ({ ...k, fingerprint: await this.fp(k.publicKey), trust: this.trustOf(k.id, k.publicKey) })));
    for (const m of info.members) m.keys = await withFp(m.keys);
    for (const i of invites) if (i.accepted) i.accepted.keys = await withFp(i.accepted.keys);
    return { ...info, workspace: ws, invites };
  }

  async invite(collectionId, email, role) {
    const ws = this.link(collectionId).workspace;
    return this.api("POST", `/v1/workspaces/${ws}/invites`, { role, ...(email ? { email } : {}) });
  }

  async accept(code) {
    return this.api("POST", "/v1/invites/accept", { token: String(code).trim().split("#").pop() });
  }

  // Adds an account (that accepted an invite) after its keys were checked and trusted here.
  async addMember(collectionId, email, role, keys) {
    const ws = this.link(collectionId).workspace;
    this.checkTrusted(keys, email);
    const { epoch, keys: epochKeys } = await this.workspaceKeys(ws);
    await this.api("POST", `/v1/workspaces/${ws}/members`, { email, role, wraps: await this.wrapsFor(keys, ws, epoch, epochKeys[epoch]) });
    for (const [e, k] of Object.entries(epochKeys)) if (+e !== epoch) await this.api("POST", `/v1/workspaces/${ws}/wraps`, { epoch: +e, wraps: await this.wrapsFor(keys, ws, +e, k) });
  }

  // Hands the workspace key to members' new devices whose keys are trusted.
  async shareKeysWithNewDevices(collectionId) {
    const info = await this.members(collectionId);
    const waiting = info.members.flatMap((m) => m.keys.filter((k) => !k.hasKey && k.trust === "trusted"));
    if (!waiting.length || info.role === "viewer") return 0;
    const { keys } = await this.workspaceKeys(info.workspace);
    for (const [e, k] of Object.entries(keys)) await this.api("POST", `/v1/workspaces/${info.workspace}/wraps`, { epoch: +e, wraps: await this.wrapsFor(waiting, info.workspace, +e, k) });
    return waiting.length;
  }

  // Removes a member: a new key (next epoch) for every remaining device, then every file is
  // re-encrypted under it.
  async removeMember(collectionId, userId) {
    const info = await this.members(collectionId);
    const ws = info.workspace;
    const remaining = info.members.filter((m) => m.userId !== userId).flatMap((m) => m.keys);
    this.checkTrusted(remaining, "the remaining members");
    const w = (await loadFormat()).sync;
    const next = info.epoch + 1;
    const key = w.newKey();
    await this.api("DELETE", `/v1/workspaces/${ws}/members/${userId}`, { epoch: next, wraps: await this.wrapsFor(remaining, ws, next, key) });
    const { keys } = await this.workspaceKeys(ws);
    const { remote } = await this.changes(ws, 0, keys);
    const me = this.account().keyId;
    let n = 0;
    for (const b of remote.filter((b) => !b.deleted)) {
      const vv = { ...b.vv, [me]: (b.vv[me] ?? 0) + 1 };
      if (!(await this.put(ws, { path: b.path, blob: b.blob, version: b.version + 1, vv, text: b.text }, next, keys[next]))) n++;
    }
    return { epoch: next, reencrypted: n };
  }

  async setRole(collectionId, userId, role) {
    const ws = this.link(collectionId).workspace;
    return this.api("PUT", `/v1/workspaces/${ws}/members/${userId}`, { role });
  }

  // A read-only runner credential for scheduled runs (`hitop sync pull` with HITOP_SYNC_RUNNER).
  async createRunner(collectionId, name) {
    const ws = this.link(collectionId).workspace;
    const w = (await loadFormat()).sync;
    const k = w.keygen();
    const { epoch, keys } = await this.workspaceKeys(ws);
    const r = await this.api("POST", `/v1/workspaces/${ws}/runners`, { name: name || "runner", publicKey: k.publicKey, wrapped: w.wrap(k.publicKey, ws, epoch, keys[epoch]) });
    for (const [e, key] of Object.entries(keys)) if (+e !== epoch) await this.api("POST", `/v1/workspaces/${ws}/wraps`, { epoch: +e, wraps: [{ keyId: r.keyId, wrapped: w.wrap(k.publicKey, ws, +e, key) }] });
    this.update((d) => (d.pins[r.keyId] = k.publicKey));
    const cred = { v: 1, server: this.account().server, workspace: ws, token: r.token, keyId: r.keyId, secret: k.secret };
    const b64 = btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(cred)))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    return { credential: `hitop-runner-1.${b64}`, userId: r.userId };
  }
}
