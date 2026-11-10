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
//                            states: { workspace: hitop-core sync State },
//                            memory: { workspace: hitop-sync-crypto WorkspaceMemory },
//                            recoveryHash: SHA-256 of the recovery data last sealed for the server }
//   the secret also holds `recovery` (the phrase's 32 bytes) and `password` (the password's bundle
//                          key, hitop-sync-crypto 0.4) when this device keeps that key bundle sealed
//
// Workspace keys are accepted only through hitop-sync-crypto's accept_wraps (the same policy as
// the CLI): a wrap must be authenticated by a pinned device key (or the inviter's, whose
// fingerprint the invite code carries), an epoch's key never changes, the epoch never goes back.
//   hitop_sync_files:<ws>  { path: text } for collections kept in app storage
import { loadFormat } from "./wasm/format.js";
import { platform } from "./platform/index.js";
import { requestFiles } from "./git.js";

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
// Hex SHA-256 of a text (the recovery data holds the device key: only its hash is kept in storage).
const sha256 = async (text) => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text))), (b) => b.toString(16).padStart(2, "0")).join("");

// A workspace key this device refused (forged, from a device not trusted, swapped, a hidden
// rotation...): `kind`, and for an untrusted sender its `fingerprint` and `senderKeyId`.
export class KeyError extends Error {
  constructor(workspace, error) {
    super(error.message);
    this.workspace = workspace;
    Object.assign(this, { kind: error.kind, fingerprint: error.fingerprint ?? null, senderKeyId: error.senderKeyId ?? null, epoch: error.epoch });
    this.security = ["forged", "keyChanged", "rollback"].includes(error.kind);
  }
}

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
  drop: (ws) => {
    localStorage.removeItem(FILES_KEY(ws));
    localStorage.removeItem(COMMENTS_KEY(ws));
  },
};

// Comments of a shared collection (since 1.3): `comments/<request file>/<id>.yaml`, one per
// file, end to end encrypted like the rest, kept here apart from the collection's own files (the
// folder, on disk or not, holds only the collection), and merged into every sync.
const COMMENTS_KEY = (ws) => `hitop_sync_comments:${ws}`;
export const isCommentPath = (p) => p.startsWith("comments/");
export const syncComments = {
  get: (ws) => readJson(COMMENTS_KEY(ws), {}),
  set: (ws, files) => writeJson(COMMENTS_KEY(ws), files),
};
const splitComments = (files) => {
  const own = {};
  const comments = {};
  for (const [p, t] of Object.entries(files)) (isCommentPath(p) ? comments : own)[p] = t;
  return { own, comments };
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
  const edit = (fn) => {
    const d = readJson(STATE_KEY, {});
    d.links ??= {};
    d.states ??= {};
    fn(d);
    writeJson(STATE_KEY, d);
  };
  return {
    pick: base?.pick,
    read: (ref) => (isSyncRef(ref) ? sync.read(ref) : base.read(ref)),
    write: (ref, folder) => (isSyncRef(ref) ? sync.write(ref, folder) : base.write(ref, folder)),
    label: (ref) => (isSyncRef(ref) ? "shared (hitop sync)" : base.label(ref)),
    async restore() {
      const own = base ? await base.restore() : [];
      // Shared collections closed (deleted) in the app since the last start: their local copy goes.
      edit((d) => {
        for (const [id, l] of Object.entries(d.links)) {
          if (!l.closed) continue;
          if (l.ref && isSyncRef(l.ref)) syncFiles.drop(l.workspace);
          delete d.states[l.workspace];
          delete d.links[id];
        }
      });
      const shared = Object.entries(links())
        .filter(([, l]) => l.ref && isSyncRef(l.ref))
        .map(([collectionId, l]) => ({ collectionId: Number.isNaN(+collectionId) ? collectionId : +collectionId, ref: l.ref, connected: true }));
      return [...own.filter((o) => !shared.some((s) => s.collectionId === o.collectionId)), ...shared];
    },
    reconnect: (ref) => (isSyncRef(ref) ? Promise.resolve(true) : base.reconnect(ref)),
    // Re-attached after a reload of its files: still shared.
    remember: (collectionId, ref) => {
      if (isSyncRef(ref)) {
        edit((d) => (d.links[collectionId] = { workspace: ref.slice(SYNC_REF.length), ref }));
        return undefined;
      }
      // A disk folder re-read (Git pull, branch switch) stays shared too.
      edit((d) => {
        if (d.links[collectionId]) delete d.links[collectionId].closed;
      });
      return base?.remember(collectionId, ref);
    },
    // Closed or deleted in the app: no longer synced here (the workspace on the server stays).
    // Marked now and cleaned up on the next start, since a reload closes and re-attaches too.
    forget: (collectionId) => {
      edit((d) => {
        if (d.links[collectionId]) d.links[collectionId].closed = true;
      });
      return base?.forget(collectionId);
    },
  };
}

const defaultDeviceName = () => (platform().name === "tauri" ? "HITOP desktop" : `HITOP web (${navigator.userAgent.match(/(Firefox|Edg|Chrome|Safari)/)?.[1] ?? "browser"})`);

export class SyncClient {
  constructor(store) {
    this.store = store;
    this.status = new Map(); // collectionId → { busy, message, error, at }
    this.notices = []; // warnings from accepting keys, for the page to show (takeNotices)
  }

  // The warnings collected since the last call ([{ kind, message, ... }]), once each.
  takeNotices() {
    const seen = new Set();
    const out = this.notices.filter((n) => !seen.has(n.message) && seen.add(n.message));
    this.notices = [];
    return out;
  }

  // --- local bookkeeping ----------------------------------------------------------------

  get data() {
    const d = readJson(STATE_KEY, {});
    return { pins: d.pins ?? {}, links: d.links ?? {}, states: d.states ?? {}, memory: d.memory ?? {}, recoveryHash: d.recoveryHash ?? null, shareVersions: d.shareVersions ?? {} };
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
    const l = this.data.links[collectionId];
    return l && !l.closed ? l : null;
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
    let value;
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
    let json;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = null;
    }
    return { status: res.status, json, text };
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

  serverUrl(server) {
    server = (server || DEFAULT_SERVER).trim().replace(/\/+$/, "");
    const u = new URL(server);
    if (u.protocol !== "https:" && !["localhost", "127.0.0.1", "[::1]"].includes(u.hostname)) throw new Error("The sync server must be https:// (http:// only for localhost)");
    return server;
  }

  async startSignIn(server, email) {
    server = this.serverUrl(server);
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
    await this.completeSignIn(server, r, email, deviceName);
    this.pending = null;
  }

  // Single sign-on: the sync server signs in with its identity provider. The page it opens ends
  // on the server, which shows a one-time code to paste here; finishing also needs a verifier
  // that never leaves this page, so a code seen by anyone else is useless to them.
  async ssoInfo(server) {
    const r = await this.http(this.serverUrl(server), "GET", "/v1/auth/sso");
    return r.status === 200 && r.json?.enabled ? r.json : null;
  }

  async startSso(server, email) {
    server = this.serverUrl(server);
    const bytes = crypto.getRandomValues(new Uint8Array(32));
    const verifier = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)));
    const challenge = btoa(String.fromCharCode(...digest)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    const r = await this.http(server, "POST", "/v1/auth/sso/start", { challenge, ...(email ? { email } : {}) });
    if (r.status === 404) throw new Error("This sync server has no single sign-on; sign in by email");
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    this.pendingSso = { server, verifier, url: r.json.url };
    return r.json.url;
  }

  async finishSso(code, deviceName) {
    const { server, verifier } = this.pendingSso ?? {};
    if (!server) throw new Error("Start the SSO sign-in first");
    const r = await this.http(server, "POST", "/v1/auth/sso/finish", { code: String(code).trim(), verifier });
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    await this.completeSignIn(server, r, r.json.user.email, deviceName);
    this.pendingSso = null;
  }

  // After a sign-in (`r.json` = {session, user}): keep this device's key or register one.
  async completeSignIn(server, r, email, deviceName) {
    const session = r.json.session;
    const w = (await loadFormat()).sync;
    // The same account on this device again keeps its key.
    const old = this.account();
    const oldSecret = old && old.userId === r.json.user.id && old.server === server ? await this.secret() : null;
    const me = await this.http(server, "GET", "/v1/me", undefined, session);
    let keyId, secret, name, recovery, password;
    if (oldSecret && me.json.keys.some((k) => k.id === old.keyId)) {
      ({ keyId, deviceName: name } = old);
      secret = oldSecret.secret;
      recovery = oldSecret.recovery;
      password = oldSecret.password;
    } else {
      const k = w.keygen();
      name = deviceName || defaultDeviceName();
      const added = await this.http(server, "POST", "/v1/me/keys", { name, publicKey: k.publicKey }, session);
      if (added.status !== 201) throw new SyncError(added.status, added.json);
      keyId = added.json.id;
      secret = k.secret;
    }
    await this.saveSecret(server, { session, secret, ...(recovery ? { recovery } : {}), ...(password ? { password } : {}) });
    writeJson(ACCOUNT_KEY, { server, email: r.json.user.email ?? email, userId: r.json.user.id, keyId, deviceName: name });
    this.update((d) => (d.pins[keyId] = w.publicKey(secret)));
  }

  // What a sign-in page can offer on `server`: {password, emailCode, passkey, sso, google, github}
  // (null when the server is older than C1b or unreachable).
  async authMethods(server) {
    try {
      const r = await this.http(this.serverUrl(server), "GET", "/v1/auth/methods");
      return r.status === 200 ? r.json : null;
    } catch {
      return null;
    }
  }

  // --- passwords (sync-server/THREAT_MODEL.md "Passwords") ---------------------------------
  // The password never leaves this device: hitop-sync-crypto derives an auth key (sent to the
  // server) and a bundle key (kept) from it with the account's Argon2id parameters. The bundle
  // key seals the same key bundle as the recovery phrase, so a password sign-in on a new device
  // becomes the account's device with no trust step.

  async passwordParams(server, email) {
    const r = await this.http(server, "POST", "/v1/auth/password/params", { email });
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    return r.json.kdf;
  }

  async signUp(server, email, password) {
    server = this.serverUrl(server);
    const w = (await loadFormat()).sync;
    w.passwordCheck(password);
    const kdf = w.passwordKdfNew();
    const keys = w.passwordKeys(password, kdf);
    const r = await this.http(server, "POST", "/v1/auth/password/signup", { email, kdf, authKey: keys.authKey });
    if (r.status !== 202) throw new SyncError(r.status, r.json);
    this.pendingSignUp = { server, email, bundleKey: keys.bundleKey };
  }

  // The code from the confirmation mail: signs in (a new account, or an existing one gets the password).
  async finishSignUp(code, deviceName) {
    const { server, email, bundleKey } = this.pendingSignUp ?? {};
    if (!server) throw new Error("Create the account first");
    const r = await this.http(server, "POST", "/v1/auth/password/verify", { token: String(code).trim().split("#").pop() });
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    this.pendingSignUp = null;
    return this.completePasswordSignIn(server, r, email, bundleKey, deviceName);
  }

  async passwordSignIn(server, email, password, deviceName) {
    server = this.serverUrl(server);
    const w = (await loadFormat()).sync;
    // Refuses parameters weaker than hitop's floor (a server can't weaken them).
    const keys = w.passwordKeys(password, await this.passwordParams(server, email));
    const r = await this.http(server, "POST", "/v1/auth/password/login", { email, authKey: keys.authKey });
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    return this.completePasswordSignIn(server, r, email, keys.bundleKey, deviceName);
  }

  // After a password sign-in: a browser without an identity for the account becomes the device
  // the password's key bundle holds (no new key, no trust step); otherwise it registers or keeps
  // its key, and seals its own identity when the account has no bundle yet.
  // → { unlocked: true, deviceName, fingerprint } | { unlocked: false }
  async completePasswordSignIn(server, r, email, bundleKey, deviceName) {
    const w = (await loadFormat()).sync;
    const session = r.json.session;
    const userId = r.json.user.id;
    const old = this.account();
    const oldSecret = old && old.userId === userId && old.server === server ? await this.secret() : null;
    const me = await this.http(server, "GET", "/v1/me", undefined, session);
    if (me.status !== 200) throw new SyncError(me.status, me.json);
    const kept = Boolean(oldSecret && me.json.keys.some((k) => k.id === old.keyId));
    const b = await this.http(server, "GET", "/v1/me/recovery?method=password", undefined, session);
    let data = null;
    if (b.status === 200) {
      try {
        data = JSON.parse(w.bundleOpen(bundleKey, userId, b.json.blob));
      } catch {
        data = null;
      }
    }
    if (!kept && data) {
      const got = await this.adopt(server, session, me.json, data, { password: bundleKey }, { email: r.json.user.email ?? email, userId, deviceName });
      return { unlocked: true, ...got };
    }
    await this.completeSignIn(server, r, email, deviceName);
    const a = this.account();
    const s = await this.secret();
    // No bundle yet (or one this password doesn't open): this device seals its identity. A bundle
    // of this very device: keep its key to re-seal it. Another device's: leave it alone.
    if (!data || data.keyId === a.keyId) {
      await this.saveSecret(server, { ...s, password: bundleKey });
      this.update((d) => (d.recoveryHash = null));
      await this.refreshRecovery();
    }
    return { unlocked: false };
  }

  // Becomes the device in a key bundle (`data`): the server must list its key id with the same
  // public key. `extra` goes into the stored secret (`recovery` or `password`).
  async adopt(server, session, me, data, extra, account) {
    const w = (await loadFormat()).sync;
    const publicKey = w.publicKey(data.secret);
    const listed = me.keys.find((k) => k.id === data.keyId);
    if (!listed) throw new Error(`The device ${data.keyId} in your key bundle is no longer on this account`);
    if (listed.publicKey !== publicKey) throw new Error(`The server lists another key for the device ${data.keyId} in your key bundle: refusing it`);
    this.update((d) => {
      Object.assign(d.pins, data.pins ?? {}, { [data.keyId]: publicKey });
      Object.assign(d.memory, data.memory ?? {});
      d.recoveryHash = null;
    });
    await this.saveSecret(server, { session, secret: data.secret, ...extra });
    const deviceName = data.deviceName || account.deviceName || defaultDeviceName();
    writeJson(ACCOUNT_KEY, { server, email: account.email, userId: account.userId, keyId: data.keyId, deviceName });
    return { deviceName, fingerprint: w.fingerprint(publicKey) };
  }

  // Signed in another way (code, passkey, Google, GitHub) on a new device: the password opens the
  // key bundle and this device becomes that one (the key its sign-in registered is deleted).
  async unlockWithPassword(password) {
    const a = this.account();
    const w = (await loadFormat()).sync;
    const keys = w.passwordKeys(password, await this.passwordParams(a.server, a.email));
    const r = await this.call("GET", "/v1/me/recovery?method=password", undefined, { allow: [404] });
    if (r.status === 404) throw new Error(`${a.email} has no key bundle sealed with a password on ${a.server}: sign in with your password on a device that has your workspaces, or use your recovery phrase`);
    let data;
    try {
      data = JSON.parse(w.bundleOpen(keys.bundleKey, a.userId, r.json.blob));
    } catch {
      throw new Error("That password doesn't open your key bundle");
    }
    const s = await this.secret();
    const fresh = a.keyId;
    const me = await this.api("GET", "/v1/me");
    this.update((d) => delete d.pins[fresh]);
    const got = await this.adopt(a.server, s.session, me, data, { password: keys.bundleKey, ...(s.recovery ? { recovery: s.recovery } : {}) }, a);
    if (fresh !== data.keyId) await this.call("DELETE", `/v1/me/keys/${fresh}`, undefined, { allow: [404, 409] });
    return got;
  }

  async forgotPassword(server, email) {
    server = this.serverUrl(server);
    const r = await this.http(server, "POST", "/v1/auth/password/forgot", { email });
    if (r.status !== 202) throw new SyncError(r.status, r.json);
    this.pendingReset = { server, email };
  }

  // The code from the reset mail and a new password: signs in. The old password's key bundle is
  // gone (`bundleLost`); a browser that still is the account's device keeps its identity and seals
  // it under the new password, a new one gets a new key and no workspace keys (`keptDevice` false).
  async resetPassword(code, password, deviceName) {
    const { server, email } = this.pendingReset ?? {};
    if (!server) throw new Error("Ask for a reset code first");
    const w = (await loadFormat()).sync;
    w.passwordCheck(password);
    const kdf = w.passwordKdfNew();
    const keys = w.passwordKeys(password, kdf);
    const r = await this.http(server, "POST", "/v1/auth/password/reset", { token: String(code).trim().split("#").pop(), kdf, authKey: keys.authKey });
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    this.pendingReset = null;
    const old = this.account();
    const before = old && old.userId === r.json.user.id && old.server === server ? old.keyId : null;
    await this.completeSignIn(server, r, email, deviceName);
    // This device (if it was the account's already) seals itself under the new password. A new
    // device doesn't claim the password's bundle: it holds no workspace key, and the next
    // password sign-in of a device that does should be what the password unlocks (as the CLI).
    const keptDevice = Boolean(before) && before === this.account().keyId;
    if (keptDevice) {
      const s = await this.secret();
      await this.saveSecret(server, { ...s, password: keys.bundleKey });
      this.update((d) => (d.recoveryHash = null));
      await this.refreshRecovery();
    }
    return { bundleLost: Boolean(r.json.bundleLost), keptDevice };
  }

  // Sets (no password yet: `current` unused) or changes the password; the key bundle is re-sealed
  // under the new one in the same request.
  async changePassword(current, next) {
    const a = this.account();
    const w = (await loadFormat()).sync;
    w.passwordCheck(next);
    const me = await this.api("GET", "/v1/me");
    let currentKeys = null;
    if (me.password?.set) currentKeys = w.passwordKeys(current, await this.passwordParams(a.server, a.email));
    let text = null;
    if (currentKeys) {
      const b = await this.call("GET", "/v1/me/recovery?method=password", undefined, { allow: [404] });
      if (b.status === 200) {
        try {
          text = w.bundleOpen(currentKeys.bundleKey, a.userId, b.json.blob);
        } catch {
          text = null;
        }
      }
    }
    // The bundle of another device stays that device's; otherwise this device's identity.
    const mine = !text || JSON.parse(text).keyId === a.keyId;
    if (mine) text = await this.recoveryData();
    const kdf = w.passwordKdfNew();
    const keys = w.passwordKeys(next, kdf);
    await this.api("POST", "/v1/me/password", { ...(currentKeys ? { currentAuthKey: currentKeys.authKey } : {}), kdf, authKey: keys.authKey, bundle: w.bundleSeal(keys.bundleKey, a.userId, text) });
    const s = await this.secret();
    const { password, ...rest } = s;
    void password;
    await this.saveSecret(a.server, mine ? { ...rest, password: keys.bundleKey } : rest);
    if (mine) this.update((d) => (d.recoveryHash = null));
  }

  // "Continue with Google" / "Continue with GitHub": the same pasted one-time code as SSO.
  async startOAuth(server, kind) {
    server = this.serverUrl(server);
    const bytes = crypto.getRandomValues(new Uint8Array(32));
    const verifier = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)));
    const challenge = btoa(String.fromCharCode(...digest)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    const r = await this.http(server, "POST", `/v1/auth/${kind}/start`, { challenge });
    if (r.status === 404) throw new Error(`This sync server has no sign-in with ${kind === "github" ? "GitHub" : "Google"}`);
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    this.pendingOAuth = { server, verifier, kind, url: r.json.url };
    return r.json.url;
  }

  async finishOAuth(code, deviceName) {
    const { server, verifier, kind } = this.pendingOAuth ?? {};
    if (!server) throw new Error("Start the sign-in first");
    const r = await this.http(server, "POST", `/v1/auth/${kind}/finish`, { code: String(code).trim(), verifier });
    if (r.status !== 200) throw new SyncError(r.status, r.json);
    await this.completeSignIn(server, r, r.json.user.email, deviceName);
    this.pendingOAuth = null;
  }

  async signOut() {
    const a = this.account();
    if (!a) return;
    await this.call("POST", "/v1/auth/logout", undefined, { allow: [401] }).catch(() => {});
    await this.saveSecret(a.server, null);
    localStorage.removeItem(ACCOUNT_KEY);
  }

  // --- recovery phrase (sync-server/THREAT_MODEL.md "Recovery") -----------------------------
  // The phrase seals this device's recovery data (its key, pins and key memory; the CLI's
  // `hitop sync recovery` writes and reads the same JSON) for the server to keep. On a new device,
  // after signing in, the phrase brings the old device back. This device keeps the phrase's bytes
  // with its secret key and re-seals the data when its pins or memory change.

  async recoveryData() {
    const a = this.account();
    const s = await this.secret();
    const d = this.data;
    return JSON.stringify({ v: 1, keyId: a.keyId, secret: s.secret, deviceName: a.deviceName ?? "", pins: d.pins, memory: d.memory });
  }

  async uploadRecovery(entropy, text) {
    const w = (await loadFormat()).sync;
    await this.api("PUT", "/v1/me/recovery", { blob: w.recoverySeal(entropy, this.account().userId, text) });
    const hash = await sha256(text);
    this.update((d) => (d.recoveryHash = hash));
  }

  // { set, updatedAt, kept, password, passwordBundle, passwordKept } (kept: this device updates
  // the data; password: the account has a password; passwordBundle: a key bundle sealed with it).
  async recoveryStatus() {
    const me = await this.api("GET", "/v1/me");
    const s = await this.secret();
    return {
      set: Boolean(me.recovery),
      updatedAt: me.recovery?.updatedAt ?? null,
      kept: Boolean(s?.recovery),
      password: Boolean(me.password?.set),
      passwordBundle: Boolean(me.bundles?.password),
      passwordKept: Boolean(s?.password),
    };
  }

  // A new phrase: returns its 24 words, to show once.
  async createRecovery() {
    const w = (await loadFormat()).sync;
    const p = w.recoveryNew();
    await this.uploadRecovery(p.entropy, await this.recoveryData());
    const s = await this.secret();
    await this.saveSecret(this.account().server, { ...s, recovery: p.entropy });
    return p.words;
  }

  // On a new device after signing in: become the lost device again.
  async useRecovery(words) {
    const w = (await loadFormat()).sync;
    const entropy = w.recoveryParse(words);
    const a = this.account();
    const r = await this.call("GET", "/v1/me/recovery", undefined, { allow: [404] });
    if (r.status === 404) throw new Error(`${a.email} has no recovery phrase set on ${a.server}`);
    let data;
    try {
      data = JSON.parse(w.recoveryOpen(entropy, a.userId, r.json.blob));
    } catch {
      throw new Error(`This recovery phrase doesn't open ${a.email}'s recovery data (a wrong phrase, or one replaced since)`);
    }
    const publicKey = w.publicKey(data.secret);
    const me = await this.api("GET", "/v1/me");
    const listed = me.keys.find((k) => k.id === data.keyId);
    if (!listed) throw new Error(`The recovered device ${data.keyId} is no longer on this account`);
    if (listed.publicKey !== publicKey) throw new Error(`The server lists another key for the recovered device ${data.keyId}: refusing it`);
    const fresh = a.keyId;
    const s = await this.secret();
    this.update((d) => {
      delete d.pins[fresh];
      Object.assign(d.pins, data.pins ?? {}, { [data.keyId]: publicKey });
      Object.assign(d.memory, data.memory ?? {});
    });
    // A password key this device holds now seals the recovered device (refreshRecovery).
    await this.saveSecret(a.server, { session: s.session, secret: data.secret, recovery: entropy, ...(s.password ? { password: s.password, passwordFresh: true } : {}) });
    writeJson(ACCOUNT_KEY, { ...a, keyId: data.keyId, deviceName: data.deviceName || a.deviceName });
    this.update((d) => (d.recoveryHash = null));
    if (fresh !== data.keyId) await this.call("DELETE", `/v1/me/keys/${fresh}`, undefined, { allow: [404, 409] });
    return { deviceName: data.deviceName || a.deviceName, fingerprint: w.fingerprint(publicKey) };
  }

  async removeRecovery() {
    await this.api("DELETE", "/v1/me/recovery");
    const s = await this.secret();
    const { recovery, ...rest } = s;
    void recovery;
    await this.saveSecret(this.account().server, rest);
  }

  // After an action: re-seal the key bundle of every method this device keeps (the phrase, the
  // password) if the data changed. A password bundle is re-sealed only while the server's still
  // opens with this device's key and holds this device (a password changed elsewhere, or another
  // device's bundle, makes this device forget its key instead).
  async refreshRecovery() {
    const a = this.account();
    if (!a) return;
    const s = await this.secret();
    if (!s?.recovery && !s?.password) return;
    const text = await this.recoveryData();
    const hash = await sha256(text);
    if (this.data.recoveryHash === hash) return;
    if (s.password) {
      const w = (await loadFormat()).sync;
      const cur = await this.call("GET", "/v1/me/recovery?method=password", undefined, { allow: [404] });
      let ours = cur.status === 404 || Boolean(s.passwordFresh);
      if (cur.status === 200 && !ours) {
        try {
          ours = JSON.parse(w.bundleOpen(s.password, a.userId, cur.json.blob)).keyId === a.keyId;
        } catch {
          ours = false;
        }
      }
      const { passwordFresh, password, ...rest } = s;
      void passwordFresh;
      if (ours) {
        await this.api("PUT", "/v1/me/recovery?method=password", { blob: w.bundleSeal(password, a.userId, text) });
        if (s.passwordFresh) await this.saveSecret(a.server, { ...rest, password });
      } else {
        await this.saveSecret(a.server, rest);
      }
    }
    if (s.recovery) await this.uploadRecovery(s.recovery, text);
    else this.update((d) => (d.recoveryHash = hash));
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

  // Trusts the device that handed this device a workspace key (KeyError "untrustedSender"), after
  // the person compared its fingerprint: the server's key for it must still have that fingerprint.
  async trustSender(ws, senderKeyId, fingerprint) {
    const info = await this.api("GET", `/v1/workspaces/${ws}`);
    const k = info.members.flatMap((m) => m.keys).find((x) => x.id === senderKeyId);
    const w = (await loadFormat()).sync;
    if (!k || w.normalizeFingerprint(await this.fp(k.publicKey)) !== w.normalizeFingerprint(fingerprint)) throw new Error("That device's key isn't the one you compared any more: not trusting it");
    if (this.trustOf(k.id, k.publicKey) === "changed") throw new Error("That device's key changed since you trusted it: not trusting the new one");
    this.trust([k]);
  }

  memoryOf(ws) {
    return this.data.memory[ws] ?? {};
  }

  saveMemory(ws, memory) {
    this.update((d) => (d.memory[ws] = memory));
  }

  // Every key must be trusted; a changed key is refused (that's what a substituted key looks like).
  checkTrusted(keys, who = "") {
    const bad = keys.filter((k) => this.trustOf(k.id, k.publicKey) !== "trusted");
    if (bad.some((k) => this.trustOf(k.id, k.publicKey) === "changed")) throw new Error(`A device key of ${who || "a member"} changed since you trusted it: not sharing the workspace key with it`);
    if (bad.length) throw new Error(`Check and trust the device keys of ${who || "every member"} first (compare the fingerprints with them)`);
  }

  // --- keys -------------------------------------------------------------------------------

  // The keys this device may use for a workspace: { epoch, keys: { epoch: key }, legacy } (legacy:
  // some of this device's wraps are unauthenticated ones from hitop 1.1). Throws a KeyError for a
  // refused key; warnings go to this.notices.
  async workspaceKeys(ws) {
    const a = this.account();
    const s = await this.secret();
    const w = (await loadFormat()).sync;
    const r = await this.api("GET", `/v1/workspaces/${ws}/wraps?keyId=${a.keyId}`);
    const pinned = this.data.pins;
    const headers = r.wraps.map((x) => w.wrapHeader(x.wrapped));
    // The member list only when a sender isn't trusted: its fingerprint, or the inviter's.
    const listed = {};
    if (headers.some((h) => !h.legacy && !pinned[h.sender])) {
      const info = await this.api("GET", `/v1/workspaces/${ws}`);
      for (const k of info.members.flatMap((m) => m.keys)) listed[k.id] = k.publicKey;
    }
    const out = w.acceptWraps(s.secret, a.keyId, ws, r.epoch, r.wraps.map((x) => ({ epoch: x.epoch, wrapped: x.wrapped })), pinned, listed, this.memoryOf(ws));
    if (!out.ok) {
      if (out.error.kind === "missingKey") throw new Error("This device has no key for the workspace's current epoch yet: ask a member to open Sync → Members (it hands the key to new devices)");
      throw new KeyError(ws, out.error);
    }
    this.update((d) => {
      d.memory[ws] = out.memory;
      for (const [id, pk] of Object.entries(out.newPins)) d.pins[id] = pk;
    });
    this.notices.push(...out.notices);
    return { epoch: out.current, keys: out.keys, legacy: headers.some((h) => h.legacy) };
  }

  // Authenticated wraps of `key` (epoch `epoch`) made by this device for `keys` [{ id, publicKey }];
  // `head` = the workspace's current epoch.
  async wrapsFor(keys, ws, epoch, key, head = epoch) {
    const w = (await loadFormat()).sync;
    const s = await this.secret();
    const me = this.account().keyId;
    return keys.map((k) => ({ keyId: k.id, wrapped: w.sealWrap(s.secret, me, k.id, k.publicKey, ws, epoch, head, key) }));
  }

  // As an owner: replaces unauthenticated (hitop 1.1) wraps with authenticated ones for every
  // trusted device of every member, for every epoch this device holds. Runner credentials made by
  // hitop 1.1 can't check them, so theirs stay. Returns { upgraded, untrusted: [names] }.
  async upgradeWraps(ws, { epoch, keys }) {
    const info = await this.api("GET", `/v1/workspaces/${ws}`);
    if (info.role !== "owner") return { upgraded: 0, untrusted: [] };
    const byEpoch = {};
    const untrusted = [];
    for (const m of info.members) {
      for (const k of m.keys) {
        const epochs = (k.legacyEpochs ?? []).filter((e) => keys[e]);
        if (!epochs.length || m.runner) continue;
        if (this.trustOf(k.id, k.publicKey) !== "trusted") {
          untrusted.push(`${m.email} (${k.name})`);
          continue;
        }
        for (const e of epochs) (byEpoch[e] ??= []).push(k);
      }
    }
    let upgraded = 0;
    for (const [e, list] of Object.entries(byEpoch)) {
      const r = await this.api("POST", `/v1/workspaces/${ws}/wraps`, { epoch: +e, wraps: await this.wrapsFor(list, ws, +e, keys[e], epoch) });
      upgraded += r.stored ?? 0;
    }
    return { upgraded, untrusted };
  }

  // --- the files of a collection --------------------------------------------------------

  async localFiles(collectionId) {
    const l = this.link(collectionId);
    await this.store.flushFolder(collectionId);
    const own = l?.ref && isSyncRef(l.ref) ? syncFiles.get(l.workspace) : (await loadFormat()).toFiles(this.store.folderContent(collectionId));
    return { ...splitComments(own).own, ...(l ? syncComments.get(l.workspace) : {}) };
  }

  async applyFiles(collectionId, all) {
    const l = this.link(collectionId);
    const entry = this.store.folderOf(collectionId);
    const { own: files, comments } = splitComments(all);
    syncComments.set(l.workspace, comments);
    if (l?.ref && isSyncRef(l.ref)) syncFiles.set(l.workspace, files);
    else if (entry) await platform().folders.write(entry.ref, (await loadFormat()).readFiles(files, this.store.collection(collectionId)?.name ?? "Shared"));
    const id = await this.store.reloadFolder(collectionId);
    // reloadFolder closes and re-attaches the collection: keep it shared (under its new id when
    // hitop.yaml changed it).
    this.update((d) => {
      delete d.links[collectionId];
      d.links[id ?? collectionId] = { workspace: l.workspace, ...(l.ref ? { ref: l.ref } : {}) };
    });
    return id ?? collectionId;
  }

  async changes(ws, since, keys) {
    const w = (await loadFormat()).sync;
    const out = [];
    let seq;
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
  // --- comments (shared collections) --------------------------------------------------------

  /** The request's path in the folder (`orders/create-order`), as hitop-core names its file. */
  requestPath(collectionId, request) {
    if (request?.file) return request.file;
    const c = this.store.collection(collectionId);
    const at = c?.requests?.findIndex((r) => r.id === request?.id) ?? -1;
    return at < 0 ? null : requestFiles(c.requests)[at].replace(/\.yaml$/, "");
  }

  /** The comments on `request`: [{path, author, at, text, mine}], oldest first. */
  comments(collectionId, request) {
    const l = this.link(collectionId);
    const file = l && this.requestPath(collectionId, request);
    if (!file) return [];
    const prefix = `comments/${file}/`;
    const me = this.account()?.email;
    return Object.entries(syncComments.get(l.workspace))
      .filter(([p]) => p.startsWith(prefix) && !p.slice(prefix.length).includes("/"))
      .map(([path, text]) => {
        let c;
        try {
          c = JSON.parse(text);
        } catch {
          // a comment written by hand: shown as text
          c = { text };
        }
        return { path, author: String(c.author ?? "?"), at: String(c.at ?? ""), text: String(c.text ?? ""), mine: c.author === me };
      })
      .sort((a, b) => a.at.localeCompare(b.at) || a.path.localeCompare(b.path));
  }

  /** Adds a comment on `request` and syncs: everyone in the workspace sees it, encrypted. */
  async addComment(collectionId, request, text) {
    const l = this.link(collectionId);
    if (!l) throw new Error("Comments are for shared collections");
    const file = this.requestPath(collectionId, request);
    if (!file) throw new Error("Save the request first: comments belong to a saved request");
    const body = String(text).trim();
    if (!body) return;
    if (body.length > 10_000) throw new Error("A comment is at most 10,000 characters");
    const id = `${Date.now()}-${[...crypto.getRandomValues(new Uint8Array(4))].map((b) => b.toString(16).padStart(2, "0")).join("")}`;
    const all = syncComments.get(l.workspace);
    // YAML is a superset of JSON: the file is JSON, readable as YAML by every hitop.
    all[`comments/${file}/${id}.yaml`] = JSON.stringify({ author: this.account().email, at: new Date().toISOString(), request: request.name, text: body }, null, 2) + "\n";
    syncComments.set(l.workspace, all);
    return this.syncNow(collectionId);
  }

  /** Removes a comment of your own and syncs. */
  async deleteComment(collectionId, path) {
    const l = this.link(collectionId);
    const all = syncComments.get(l.workspace);
    delete all[path];
    syncComments.set(l.workspace, all);
    return this.syncNow(collectionId);
  }

  async syncNow(collectionId, { push = true } = {}) {
    const l = this.link(collectionId);
    if (!l) throw new Error("This collection isn't shared");
    const ws = l.workspace;
    const me = this.account().keyId;
    const w = (await loadFormat()).sync;
    const first = await this.workspaceKeys(ws);
    const { legacy } = first;
    // Re-read below when the server says the epoch moved on (stale_epoch).
    let { epoch, keys } = first;
    // Keys from hitop 1.1 here: an owner's sync upgrades them (members see a warning).
    if (legacy) {
      const u = await this.upgradeWraps(ws, { epoch, keys });
      if (u.upgraded) this.notices.push({ kind: "upgraded", message: `Upgraded ${u.upgraded} workspace key wrap(s) to authenticated ones. Members confirm your fingerprint once.` });
      if (u.untrusted.length) this.notices.push({ kind: "notUpgraded", message: `Not upgraded, devices you haven't trusted: ${u.untrusted.join(", ")}` });
    }
    for (let attempt = 0; attempt < 5; attempt++) {
      let state = this.state(ws);
      const { remote, seq } = await this.changes(ws, state.seq, keys);
      const local = await this.localFiles(collectionId);
      const plan = w.plan(local, state, remote, me);
      plan.state.seq = seq;
      if (Object.keys(plan.write).length || plan.remove.length) {
        const files = { ...local, ...plan.write };
        for (const p of plan.remove) delete files[p];
        // Only comments changed: the collection itself stays as it is.
        if ([...Object.keys(plan.write), ...plan.remove].every(isCommentPath)) syncComments.set(ws, splitComments(files).comments);
        else collectionId = await this.applyFiles(collectionId, files);
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
      if (!again) {
        // HITOP Cloud C4: the shared environment values this device can open, for its runs and
        // sends (memory only); as a sharer, hand them to devices without a copy.
        await this.refreshShared(ws, result.collectionId);
        return result;
      }
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
    // In the collection's organization (the top bar's switcher), when signed in to HITOP Cloud.
    const c = this.store?.collection?.(collectionId);
    const org = c && this.store.orgOf?.(c);
    await this.api("POST", "/v1/workspaces", { id: ws, wraps: await this.wrapsFor(me.keys, ws, 1, key), ...(org ? { org } : {}) });
    if (org) this.store.workspaceOrgs?.set(ws, org);
    // Made here: only authenticated keys from now on.
    this.saveMemory(ws, { strict: true });
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
    const { own, comments } = splitComments(plan.write);
    syncFiles.set(ws, own);
    syncComments.set(ws, comments);
    this.saveState(ws, plan.state);
    const folder = (await loadFormat()).readFiles(own, "Shared collection");
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
    const withFp = async (keys) => Promise.all((keys ?? []).map(async (k) => ({ ...k, fingerprint: await this.fp(k.publicKey), trust: this.trustOf(k.id, k.publicKey), legacy: (k.legacyEpochs ?? []).length > 0 })));
    for (const m of info.members) m.keys = await withFp(m.keys);
    for (const i of invites) if (i.accepted) i.accepted.keys = await withFp(i.accepted.keys);
    return { ...info, workspace: ws, invites };
  }

  // An invite: the server's answer plus `code` = "<token>.<this device's fingerprint>" and a link
  // with it, so the invitee's hitop checks that the workspace key comes from this device. (A
  // mailed invite carries only the token: the server writes the mail.)
  async invite(collectionId, email, role) {
    const ws = this.link(collectionId).workspace;
    const r = await this.api("POST", `/v1/workspaces/${ws}/invites`, { role, ...(email ? { email } : {}) });
    const fp = (await loadFormat()).sync.normalizeFingerprint(await this.fingerprint());
    const code = `${r.token}.${fp}`;
    return { ...r, code, link: r.link.includes("#") ? `${r.link.split("#")[0]}#${code}` : r.link };
  }

  // Accepts an invite code ("<token>.<fingerprint>" from hitop 1.2, or a bare token). From now on
  // this device takes only authenticated keys for the workspace, and trusts the device the code
  // names. Returns { workspaceId, role, inviter } (inviter: the fingerprint, or null).
  async accept(code) {
    const text = String(code).trim().split("#").pop();
    const [token, fpPart] = text.split(".");
    const w = (await loadFormat()).sync;
    const inviter = fpPart ? w.normalizeFingerprint(fpPart) : null;
    if (inviter !== null && inviter.length !== 32) throw new Error("This invite code is damaged (the part after the '.' must be a key fingerprint)");
    const r = await this.api("POST", "/v1/invites/accept", { token });
    const m = this.memoryOf(r.workspaceId);
    const expected = new Set(m.expectedSenders ?? []);
    if (inviter) expected.add(inviter);
    this.saveMemory(r.workspaceId, { ...m, strict: true, expectedSenders: [...expected] });
    return { ...r, inviter: inviter ? inviter.match(/.{4}/g).join(" ") : null };
  }

  // Adds an account (that accepted an invite) after its keys were checked and trusted here.
  async addMember(collectionId, email, role, keys) {
    const ws = this.link(collectionId).workspace;
    this.checkTrusted(keys, email);
    const { epoch, keys: epochKeys } = await this.workspaceKeys(ws);
    await this.api("POST", `/v1/workspaces/${ws}/members`, { email, role, wraps: await this.wrapsFor(keys, ws, epoch, epochKeys[epoch], epoch) });
    for (const [e, k] of Object.entries(epochKeys)) if (+e !== epoch) await this.api("POST", `/v1/workspaces/${ws}/wraps`, { epoch: +e, wraps: await this.wrapsFor(keys, ws, +e, k, epoch) });
    // In the same step: the shared environment values they are meant to have (the member is
    // added either way).
    try {
      return await this.resealShares(ws);
    } catch (e) {
      this.notices.push({ kind: "shares", message: `Shared environment values not handed to them: ${e.message}` });
      return 0;
    }
  }

  // Hands the workspace key to members' new devices whose keys are trusted.
  async shareKeysWithNewDevices(collectionId) {
    const info = await this.members(collectionId);
    const waiting = info.members.flatMap((m) => m.keys.filter((k) => !k.hasKey && k.trust === "trusted"));
    if (!waiting.length || info.role === "viewer") return 0;
    const { epoch, keys } = await this.workspaceKeys(info.workspace);
    for (const [e, k] of Object.entries(keys)) await this.api("POST", `/v1/workspaces/${info.workspace}/wraps`, { epoch: +e, wraps: await this.wrapsFor(waiting, info.workspace, +e, k, epoch) });
    await this.resealShares(info.workspace).catch((e) => this.notices.push({ kind: "shares", message: `Shared environment values not handed to new devices: ${e.message}` }));
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
    await this.api("DELETE", `/v1/workspaces/${ws}/members/${userId}`, { epoch: next, wraps: await this.wrapsFor(remaining, ws, next, key, next) });
    // Their devices are no longer trusted here: keys they hand out from now on are refused.
    const removed = info.members.find((m) => m.userId === userId)?.keys ?? [];
    this.update((d) => {
      for (const k of removed) delete d.pins[k.id];
    });
    const { keys } = await this.workspaceKeys(ws);
    const { remote } = await this.changes(ws, 0, keys);
    const me = this.account().keyId;
    let n = 0;
    for (const b of remote.filter((b) => !b.deleted)) {
      const vv = { ...b.vv, [me]: (b.vv[me] ?? 0) + 1 };
      if (!(await this.put(ws, { path: b.path, blob: b.blob, version: b.version + 1, vv, text: b.text }, next, keys[next]))) n++;
    }
    // Shared environment values: a new key for whoever is still meant to have them. What the
    // removed person saw stays theirs: the page says to rotate those secrets.
    const shares = await this.rekeyShares(ws).catch((e) => {
      this.notices.push({ kind: "shares", message: `Shared environment values not re-keyed: ${e.message}` });
      return {};
    });
    return { epoch: next, reencrypted: n, ...shares };
  }

  // The workspace's audit log (owners), newest first, and the server's check of its hash chain.
  async audit(collectionId, { limit = 50, event = "" } = {}) {
    const ws = this.link(collectionId).workspace;
    const q = new URLSearchParams({ limit: String(limit), ...(event ? { event } : {}) });
    const [log, check] = await Promise.all([this.api("GET", `/v1/workspaces/${ws}/audit?${q}`), this.api("GET", `/v1/workspaces/${ws}/audit/verify`)]);
    return { events: log.events, more: log.next !== null, check };
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
    // The server names the runner's key only when it makes the runner, and an authenticated wrap
    // names its recipient: the first wrap is a placeholder (unauthenticated, of a random key, which
    // the runner never accepts) that the authenticated ones replace right after.
    const r = await this.api("POST", `/v1/workspaces/${ws}/runners`, { name: name || "runner", publicKey: k.publicKey, wrapped: w.wrapLegacy(k.publicKey, ws, epoch, w.newKey()) });
    for (const [e, key] of Object.entries(keys)) await this.api("POST", `/v1/workspaces/${ws}/wraps`, { epoch: +e, wraps: await this.wrapsFor([{ id: r.keyId, publicKey: k.publicKey }], ws, +e, key, epoch) });
    this.update((d) => (d.pins[r.keyId] = k.publicKey));
    // The runner accepts keys from this device and the owners' and editors' devices trusted here.
    const info = await this.api("GET", `/v1/workspaces/${ws}`);
    const pins = { [this.account().keyId]: w.publicKey((await this.secret()).secret) };
    for (const m of info.members.filter((x) => x.role === "owner" || x.role === "editor")) for (const key of m.keys) if (this.trustOf(key.id, key.publicKey) === "trusted") pins[key.id] = key.publicKey;
    const cred = { v: 1, server: this.account().server, workspace: ws, token: r.token, keyId: r.keyId, secret: k.secret, pins };
    const b64 = btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(cred)))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    return { credential: `hitop-runner-1.${b64}`, userId: r.userId };
  }

  // --- HITOP Cloud C4: run history, alerts, shared environment values -------------------

  // Status rows, newest first: { runs, more, series, historyDays, sealedHistory }.
  async runs(collectionId, { limit = 50 } = {}) {
    const ws = this.link(collectionId).workspace;
    return this.api("GET", `/v1/workspaces/${ws}/runs?limit=${limit}`);
  }

  // One run's sealed report, opened on this device: { row, report, mismatch } (mismatch: the
  // server's status row says otherwise than the report; the report is what ran).
  async runReport(collectionId, id) {
    const ws = this.link(collectionId).workspace;
    const row = await this.api("GET", `/v1/workspaces/${ws}/runs/${encodeURIComponent(id)}`);
    if (!row.detail) return { row, report: null, mismatch: false };
    const w = (await loadFormat()).sync;
    const { keys } = await this.workspaceKeys(ws);
    const epoch = w.runEpoch(row.detail);
    if (!keys[epoch]) throw new Error(`The report is under key epoch ${epoch}, which this device doesn't have`);
    const report = JSON.parse(w.runOpen(keys[epoch], ws, id, row.detail));
    const mismatch = row.status !== "sealed" && row.status !== (report.passed ? "passed" : "failed");
    return { row, report, mismatch };
  }

  async alertSettings(collectionId) {
    const ws = this.link(collectionId).workspace;
    return this.api("GET", `/v1/workspaces/${ws}/alerts`);
  }

  async saveAlerts(collectionId, settings) {
    const ws = this.link(collectionId).workspace;
    return this.api("PUT", `/v1/workspaces/${ws}/alerts`, settings);
  }

  async alertLog(collectionId) {
    const ws = this.link(collectionId).workspace;
    return (await this.api("GET", `/v1/workspaces/${ws}/alert-deliveries`)).deliveries;
  }

  // The shares of a workspace this device can open: { opened: [{ id, version, content, key,
  // wrappedFor }], closed: [reason] }. A wrap is used only when the device it names is trusted
  // here; an older version than one seen before is refused.
  async openShares(ws) {
    const r = await this.call("GET", `/v1/workspaces/${ws}/env-shares`, undefined, { allow: [404] });
    if (r.status === 404) return { opened: [], closed: [] };
    const w = (await loadFormat()).sync;
    const a = this.account();
    const s = await this.secret();
    const myPublic = w.publicKey(s.secret);
    const pins = this.data.pins;
    const seen = this.data.shareVersions[ws] ?? {};
    const opened = [];
    const closed = [];
    for (const sh of r.json.shares ?? []) {
      const mine = sh.wraps.find((x) => x.keyId === a.keyId);
      if (!mine) {
        closed.push(`${sh.id}: not shared with this device`);
        continue;
      }
      const sender = w.shareWrapSender(mine.wrapped);
      const senderKey = sender === a.keyId ? myPublic : pins[sender];
      if (!senderKey) {
        closed.push(`${sh.id}: shared by a device you haven't trusted`);
        continue;
      }
      if (sh.version < (seen[sh.id] ?? 0)) throw new Error(`The server hands out version ${sh.version} of a shared environment this device saw at version ${seen[sh.id]}: refusing an older version`);
      const o = JSON.parse(w.shareOpen(s.secret, a.keyId, senderKey, ws, sh.id, sh.version, mine.wrapped, sh.sealed));
      opened.push({ id: sh.id, version: sh.version, content: JSON.parse(o.content), key: o.key, wrappedFor: sh.wraps.map((x) => x.keyId) });
    }
    this.update((d) => {
      d.shareVersions[ws] = { ...(d.shareVersions[ws] ?? {}), ...Object.fromEntries(opened.map((o) => [o.id, o.version])) };
    });
    return { opened, closed };
  }

  // The devices a share is for (its content says: owners and editors, viewers when it says so,
  // the runner credentials it names), from GET /v1/workspaces/:ws.
  shareRecipients(info, c) {
    return info.members
      .filter((m) => (m.runner ? c.allRunners || (c.runners ?? []).includes(m.userId) : m.role === "owner" || m.role === "editor" || (m.role === "viewer" && c.viewers)))
      .flatMap((m) => m.keys);
  }

  async shareWraps(keys, ws, id, version, key) {
    const w = (await loadFormat()).sync;
    const s = await this.secret();
    const me = this.account().keyId;
    return keys.map((k) => ({ keyId: k.id, wrapped: w.shareWrap(s.secret, me, k.id, k.publicKey, ws, id, version, key) }));
  }

  // Hands every share this device can open to the trusted devices meant to have it that have no
  // copy. Returns how many wraps were stored.
  async resealShares(ws) {
    const { opened } = await this.openShares(ws);
    if (!opened.length) return 0;
    const info = await this.api("GET", `/v1/workspaces/${ws}`);
    if (info.role === "viewer") return 0;
    const me = this.account().keyId;
    let n = 0;
    for (const o of opened) {
      const missing = this.shareRecipients(info, o.content).filter((k) => !o.wrappedFor.includes(k.id) && (k.id === me || this.trustOf(k.id, k.publicKey) === "trusted"));
      if (!missing.length) continue;
      const r = await this.api("POST", `/v1/workspaces/${ws}/env-shares/${o.id}/wraps`, { version: o.version, wraps: await this.shareWraps(missing, ws, o.id, o.version, o.key) });
      n += r.stored ?? 0;
    }
    return n;
  }

  // After a removal: a new key (next version) for every share this device can open.
  async rekeyShares(ws) {
    const { opened, closed } = await this.openShares(ws);
    if (!opened.length && !closed.length) return { sharesRekeyed: 0, sharesNotRekeyed: 0 };
    const info = await this.api("GET", `/v1/workspaces/${ws}`);
    const w = (await loadFormat()).sync;
    const me = this.account().keyId;
    for (const o of opened) {
      const to = this.shareRecipients(info, o.content).filter((k) => k.id === me || this.trustOf(k.id, k.publicKey) === "trusted");
      const version = o.version + 1;
      const key = w.newKey();
      const sealed = w.shareSeal(key, ws, o.id, version, JSON.stringify(o.content));
      await this.api("PUT", `/v1/workspaces/${ws}/env-shares/${o.id}`, { version, sealed, wraps: await this.shareWraps(to, ws, o.id, version, key) });
    }
    return { sharesRekeyed: opened.length, sharesNotRekeyed: closed.length };
  }

  // After a sync: the shared values (by environment name) go to the store, in memory, under the
  // collection's own values; and a sharer hands them to new devices.
  async refreshShared(ws, collectionId) {
    try {
      const { opened } = await this.openShares(ws);
      const byEnv = Object.fromEntries(opened.map((o) => [o.content.environment, o.content.values ?? {}]));
      this.store.setSharedValues?.(collectionId, byEnv);
      if (opened.length) await this.resealShares(ws);
    } catch (e) {
      this.notices.push({ kind: "shares", message: `Shared environment values not used: ${e.message}` });
    }
  }

  // [{ environment, version, names, recipients }] for the Account page and Share dialog: names only, never values.
  async sharedEnvironments(collectionId) {
    const ws = this.link(collectionId).workspace;
    const { opened, closed } = await this.openShares(ws);
    return { shared: opened.map((o) => ({ environment: o.content.environment, version: o.version, names: Object.keys(o.content.values ?? {}), devices: o.wrappedFor.length })), closed: closed.length };
  }
}
