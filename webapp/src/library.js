// The personal library (HITOP Cloud C8, spec/CLOUD_SERVICE.md "Personal library"): signed in to
// hitop sync, this app keeps its collections, environments and globals on the server, so every
// browser the person signs in to has them. Signed out, everything stays in this browser as before.
//
// The server can read the library (owner decision 2026-11-05); shared workspaces (sync.js) stay end
// to end and are never in it, nor are folder collections. Secret values go apart from the items
// (`secrets`): the server seals them at rest under its host key, or doesn't store them (then they
// stay in this browser: `secretsStored: false`).
//
//   start()     before the store reads storage: pulls what changed and writes it into storage
//   attach(s)   once the store exists: pushes what changed after each save (debounced), pulls on
//               focus and every 30 s, applies other browsers' changes to the store
//
// localStorage `hitop_library` = { account: "<server> <userId>", cursor, known: { "<kind>/<id>":
// { version, hash } }, localOnly: [collection ids never uploaded] }. A conflict (another browser
// changed the item first) keeps the server's version and keeps this browser's collection as a copy
// named "… (this browser)".
import { platform } from "./platform/index.js";
import { normalizeGlobals, secretNames } from "./features.js";

const STATE_KEY = "hitop_library";
// "off": this browser doesn't save to the library (the Sync page's switch); anything else: on.
const OFF_KEY = "hitop_library_off";
export const libraryOff = () => {
  try {
    return localStorage.getItem(OFF_KEY) === "off";
  } catch {
    return false;
  }
};
const PUSH_DELAY = 1000;
const PULL_EVERY = 30_000;

const readState = () => {
  try {
    return JSON.parse(localStorage.getItem(STATE_KEY) || "null");
  } catch {
    return null;
  }
};
const writeState = (s) => {
  try {
    localStorage.setItem(STATE_KEY, JSON.stringify(s));
  } catch {
    // storage full: the next start pulls everything again
  }
};

// A short, stable fingerprint of an item as pushed (data + secrets), to see what changed.
function hash(value) {
  const text = JSON.stringify(value);
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return `${h.toString(16)}-${text.length}`;
}

// --- items <-> app state ---------------------------------------------------------------------

/** An environment without its secret values, and those values. */
function splitEnv(env) {
  const names = secretNames(env);
  const variables = { ...(env.variables || {}) };
  const secrets = {};
  for (const n of names) {
    if (variables[n]) secrets[n] = variables[n];
    variables[n] = "";
  }
  return { data: { ...env, variables }, secrets };
}

/** Puts secret values back: from the server, else the ones this browser already had. */
function joinEnv(data, secrets, local) {
  const variables = { ...(data.variables || {}) };
  for (const n of secretNames(data)) variables[n] = secrets?.[n] ?? local?.variables?.[n] ?? variables[n] ?? "";
  return { ...data, variables };
}

function environmentsItem(environments, activeId) {
  const secrets = {};
  const list = environments.map((e) => {
    const { data, secrets: s } = splitEnv(e);
    if (Object.keys(s).length) secrets[e.id] = s;
    return data;
  });
  return { data: { environments: list, activeEnvironmentId: activeId ?? null }, secrets };
}

function globalsItem(globals) {
  const { data, secrets } = splitEnv(normalizeGlobals(globals));
  return { data, secrets };
}

const idOf = (c) => String(c.id);

export class Library extends EventTarget {
  /** `client`: a sync.js SyncClient (its account, session and HTTP). */
  constructor(client) {
    super();
    this.client = client;
    this.store = null;
    this.state = null;
    this.status = { on: false, saving: false, error: "", savedAt: 0, secretsStored: null };
    this.pushTimer = 0;
    this.pulling = null;
  }

  get signedIn() {
    return Boolean(this.client.account()) && !libraryOff();
  }

  /** The Sync page's switch: off keeps this browser's collections here only (shared workspaces still sync). */
  async setEnabled(on) {
    try {
      if (on) localStorage.removeItem(OFF_KEY);
      else localStorage.setItem(OFF_KEY, "off");
    } catch {
      // storage blocked: the switch lasts for this page
    }
    if (!on) {
      clearTimeout(this.pushTimer);
      this.state = null;
      try {
        localStorage.removeItem(STATE_KEY);
      } catch {
        // ignored
      }
      this.setStatus({ on: false, error: "" });
      return;
    }
    await this.checkAccount();
  }

  accountKey() {
    const a = this.client.account();
    return a ? `${a.server} ${a.userId}` : "";
  }

  setStatus(patch) {
    this.status = { ...this.status, ...patch };
    // The environments page says where secret values are kept.
    const p = platform();
    p.secretsNoteLocal ??= p.secretsNote;
    p.secretsNote = this.status.on && this.status.secretsStored
      ? "Secret values are saved to HITOP Cloud, encrypted at rest and shown only to you. They are masked here and in history, and never written to exports."
      : p.secretsNoteLocal;
    this.dispatchEvent(new Event("change"));
  }

  /** This account's state; a new account (or none yet) starts again, keeping today's collections local. */
  loadState(collections) {
    const key = this.accountKey();
    const s = readState();
    if (s && s.account === key) return s;
    // The first account this browser links uploads what is here; a different account later
    // doesn't (those collections stay in this browser only). The empty "Default" collection a
    // fresh browser makes for itself is never uploaded: every browser would add one.
    const untouched = (c) => c.name === "Default" && (c.requests || []).every((r) => !r.url && !r.body) && !Object.keys(c.variables || {}).length;
    const localOnly = collections.filter((c) => (s && s.account) || untouched(c)).map(idOf);
    return { account: key, cursor: null, known: {}, localOnly };
  }

  async api(method, path, body, allow = []) {
    return this.client.call(method, path, body, { allow });
  }

  // --- start: before the store -----------------------------------------------------------

  /** Pulls into storage (both adapters' load/save), so the store starts with the library. */
  async start() {
    if (!this.signedIn) return;
    const st = platform().storage;
    const collections = st.loadCollections() || [];
    this.state = this.loadState(collections);
    let r;
    try {
      r = await this.pullRaw();
    } catch (e) {
      this.setStatus({ on: true, error: `Couldn't reach HITOP Cloud: ${e.message || e}. Working in this browser until it answers.` });
      return;
    }
    const envs = st.loadEnvironments();
    const next = this.applyItems(r.items, { collections, environments: envs.environments, activeEnvironmentId: envs.activeEnvironmentId, globals: st.loadGlobals?.() });
    st.saveCollections(next.collections);
    st.saveEnvironments(next.environments, next.activeEnvironmentId);
    if (next.globals) st.saveGlobals?.(next.globals);
    this.setStatus({ on: true, error: "" });
  }

  async pullRaw() {
    const since = this.state.cursor;
    const r = (await this.api("GET", since == null ? "/v1/library" : `/v1/library?since=${since}`)).json;
    this.state.cursor = r.cursor;
    this.setStatus({ secretsStored: r.secretsStored });
    writeState(this.state);
    return r;
  }

  /** Server items over `app` ({collections, environments, activeEnvironmentId, globals}); returns the new app state. */
  applyItems(items, app) {
    let collections = [...app.collections];
    let { environments, activeEnvironmentId, globals } = app;
    for (const it of items) {
      const key = `${it.kind}/${it.id}`;
      const known = this.state.known[key];
      if (known && known.version >= it.version) continue; // our own write, or older
      if (it.kind === "collection") {
        const i = collections.findIndex((c) => idOf(c) === it.id);
        // A folder or shared-workspace collection is never the library's to change.
        if (i >= 0 && this.store?.folders.has(collections[i].id)) {
          delete this.state.known[key];
          continue;
        }
        if (it.deleted) {
          // Only a collection this browser holds from the library goes (not after our own delete).
          if (known && i >= 0) collections.splice(i, 1);
          delete this.state.known[key];
          continue;
        }
        if (i >= 0) collections[i] = it.data;
        else collections.push(it.data);
        this.state.localOnly = this.state.localOnly.filter((x) => x !== it.id);
        this.state.known[key] = { version: it.version, hash: hash({ data: it.data, secrets: {} }) };
      } else if (it.kind === "environments" && !it.deleted) {
        const byId = new Map(environments.map((e) => [e.id, e]));
        const list = (it.data.environments || []).map((e) => joinEnv(e, it.secrets?.[e.id], byId.get(e.id)));
        environments = list;
        activeEnvironmentId = list.some((e) => e.id === it.data.activeEnvironmentId) ? it.data.activeEnvironmentId : activeEnvironmentId;
        this.state.known[key] = { version: it.version, hash: hash(environmentsItem(list, activeEnvironmentId)) };
      } else if (it.kind === "globals" && !it.deleted) {
        globals = normalizeGlobals(joinEnv(it.data, it.secrets, globals));
        this.state.known[key] = { version: it.version, hash: hash(globalsItem(globals)) };
      }
    }
    writeState(this.state);
    return { collections, environments, activeEnvironmentId, globals };
  }

  // --- attached to the store -----------------------------------------------------------------

  attach(store) {
    this.store = store;
    const st = platform().storage;
    // Every save goes through these three: push what changed a moment later.
    for (const name of ["saveCollections", "saveEnvironments", "saveGlobals"]) {
      const original = st[name];
      if (!original) continue;
      st[name] = (...args) => {
        const r = original.apply(st, args);
        this.schedulePush();
        return r;
      };
    }
    addEventListener("focus", () => this.pull());
    document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && this.pull());
    setInterval(() => document.visibilityState !== "hidden" && this.checkAccount().then(() => this.pull()), PULL_EVERY);
    // A sign-in on the Sync page (or a sign-out) while the app runs: the page changes the store.
    store.addEventListener("change", () => this.checkAccount());
    if (this.signedIn) this.schedulePush(0);
  }

  async checkAccount() {
    const key = this.signedIn ? this.accountKey() : "";
    if (!key) {
      if (this.status.on) this.setStatus({ on: false, error: "" });
      return;
    }
    if (this.state?.account === key) return;
    // Signed in just now: link this browser's library and pull it into the app.
    this.state = this.loadState(this.store.collections.filter((c) => !this.store.folders.has(c.id)));
    this.setStatus({ on: true, error: "" });
    await this.pull();
    this.schedulePush(0);
  }

  schedulePush(delay = PUSH_DELAY) {
    if (!this.store || !this.signedIn) return;
    clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => this.push(), delay);
  }

  /** Pushes what is waiting now and resolves when the library has it (tests, Sync now). */
  async flush() {
    for (let i = 0; i < 3; i++) {
      clearTimeout(this.pushTimer);
      while (this.pushing) await new Promise((r) => setTimeout(r, 50));
      await this.push();
      if (!this.dirty()) return;
    }
  }

  /** A collection kept in this browser only (here before the first sign-in) goes to HITOP Cloud now. */
  async upload(collectionId) {
    if (!this.state) return;
    this.state.localOnly = this.state.localOnly.filter((x) => x !== String(collectionId));
    writeState(this.state);
    this.setStatus({});
    await this.flush();
  }

  /** Something here differs from what the library last took. */
  dirty() {
    if (!this.store || !this.state) return false;
    const want = this.desired();
    const changed = Object.entries(want).some(([key, item]) => this.state.known[key]?.hash !== hash({ data: item.data, secrets: item.secrets }) && (key.startsWith("collection/") || this.state.known[key]));
    return changed || Object.keys(this.state.known).some((key) => key.startsWith("collection/") && !want[key]);
  }

  /** What the library should hold now: {key: {kind, id, data, secrets}}. */
  desired() {
    const s = this.store;
    const out = {};
    const localOnly = new Set(this.state.localOnly);
    for (const c of s.collections) {
      if (s.folders.has(c.id) || localOnly.has(idOf(c))) continue;
      out[`collection/${idOf(c)}`] = { kind: "collection", id: idOf(c), data: c, secrets: {} };
    }
    const envs = s.environments.filter((e) => !s.folderEnvs.has(e.id));
    const active = envs.some((e) => e.id === s.activeEnvironmentId) ? s.activeEnvironmentId : null;
    out["environments/all"] = { kind: "environments", id: "all", ...environmentsItem(envs, active) };
    out["globals/all"] = { kind: "globals", id: "all", ...globalsItem(s.globals) };
    return out;
  }

  async push() {
    if (!this.store || !this.signedIn || !this.state || this.state.account !== this.accountKey()) return;
    if (this.pushing) return this.schedulePush();
    this.pushing = true;
    this.setStatus({ saving: true });
    try {
      if (this.pulling) await this.pulling;
      const want = this.desired();
      for (const [key, item] of Object.entries(want)) {
        const body = { data: item.data, secrets: item.secrets };
        const h = hash(body);
        const known = this.state.known[key];
        if (known?.hash === h) continue;
        if (!known && item.kind !== "collection" && !Object.keys(item.data.environments ?? item.data.variables ?? {}).length) continue;
        const r = await this.api("PUT", `/v1/library/${item.kind}/${encodeURIComponent(item.id)}`, { ...body, baseVersion: known?.version ?? 0 }, [402, 409, 413]);
        if (r.status === 200) {
          this.state.known[key] = { version: r.json.version, hash: h };
        } else if (r.status === 409) {
          this.conflict(item, r.json.current);
        } else {
          const why = r.status === 402 ? `HITOP Cloud storage is full (${r.json?.message || "plan limit"}).` : `"${item.data.name ?? item.kind}" is over 5 MB.`;
          this.setStatus({ error: `${why} It stays in this browser.` });
          this.store.notify(`${why} It stays in this browser.`);
        }
        writeState(this.state);
      }
      // Collections gone from this browser go from the library (a delete, a move to a folder or a share).
      for (const key of Object.keys(this.state.known)) {
        if (!key.startsWith("collection/") || want[key]) continue;
        const id = key.slice("collection/".length);
        const r = await this.api("DELETE", `/v1/library/collection/${encodeURIComponent(id)}?baseVersion=${this.state.known[key].version}`, undefined, [409]);
        if (r.status === 200) delete this.state.known[key];
        else this.conflict({ kind: "collection", id, data: null }, r.json.current);
        writeState(this.state);
      }
      this.setStatus({ saving: false, savedAt: Date.now(), error: this.status.error.includes("over 5 MB") || this.status.error.includes("storage is full") ? this.status.error : "" });
    } catch (e) {
      this.setStatus({ saving: false, error: `Couldn't save to HITOP Cloud: ${e.message || e}. Saved in this browser; trying again.` });
      this.schedulePush(15_000);
    } finally {
      this.pushing = false;
    }
  }

  /** Another browser changed `item` first: take the server's, keep ours as a copy. */
  conflict(item, current) {
    const s = this.store;
    const key = `${item.kind}/${item.id}`;
    if (item.kind === "collection") {
      const mine = item.data;
      const theirs = current && !current.deleted ? current.data : null;
      let cols = s.collections.filter((c) => idOf(c) !== item.id);
      if (theirs) cols.push(theirs);
      if (mine) {
        const copyId = Date.now() * 1000 + Math.floor(Math.random() * 1000);
        cols.push({ ...mine, id: copyId, name: `${mine.name} (this browser)` });
        s.notify(`"${mine.name}" changed in another browser. Kept theirs and saved yours as "${mine.name} (this browser)".`);
      }
      s.collections = cols;
      if (current && !current.deleted) this.state.known[key] = { version: current.version, hash: hash({ data: theirs, secrets: {} }) };
      else delete this.state.known[key];
    } else {
      const next = this.applyItems([{ ...current, version: current.version }], { collections: s.collections, environments: s.environments.filter((e) => !s.folderEnvs.has(e.id)), activeEnvironmentId: s.activeEnvironmentId, globals: s.globals });
      this.replaceSettings(next);
      s.notify(item.kind === "globals" ? "Globals changed in another browser: showing theirs." : "Environments changed in another browser: showing theirs.");
    }
    writeState(this.state);
    s.emit();
  }

  replaceSettings(next) {
    const s = this.store;
    s.environments = [...next.environments, ...s.environments.filter((e) => s.folderEnvs.has(e.id))];
    s.activeEnvironmentId = next.activeEnvironmentId;
    if (next.globals) s.globals = next.globals;
  }

  /** Other browsers' changes into the running app. */
  pull() {
    if (!this.store || !this.signedIn || !this.state || this.state.account !== this.accountKey()) return Promise.resolve();
    if (this.pulling) return this.pulling;
    this.pulling = (async () => {
      try {
        const r = await this.pullRaw();
        const fresh = r.items.filter((it) => !(this.state.known[`${it.kind}/${it.id}`]?.version >= it.version));
        if (!fresh.length) return;
        const s = this.store;
        const next = this.applyItems(fresh, { collections: s.collections, environments: s.environments.filter((e) => !s.folderEnvs.has(e.id)), activeEnvironmentId: s.activeEnvironmentId, globals: s.globals });
        s.collections = next.collections;
        this.replaceSettings(next);
        s.emit();
      } catch {
        // offline: the next focus or tick tries again
      } finally {
        this.pulling = null;
      }
    })();
    return this.pulling;
  }
}
