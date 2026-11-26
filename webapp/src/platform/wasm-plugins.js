// Plugins in the web app and the browser extensions (docs/PLUGINS_THREAT_MODEL.md, part 2). A plugin is
// a file the person installs in this browser: kept in IndexedDB (its own database, never in
// localStorage, so the library, sync, folder writes and exports, which read the app's localStorage keys
// and collections, can't carry one), with the SHA-256 taken at install and checked before each call.
// A hook runs in the plugins worker (plugins-worker.js), one call at a time, stopped after LIMIT_MS;
// after a stop or a trap the worker is replaced.
import wasmUrl from "hitop-scripts-wasm/wasm?url";
import { loadFormat } from "../wasm/format.js";

export const LIMIT_MS = 5_000;
export const MAX_BYTES = 1024 * 1024;
// Loading the wasm isn't the plugin's time.
const START_MS = 60_000;
const DB = "hitop-plugins";
const STORE = "plugins";

let worker = null;
let started = null;
let next = 1;
const pending = new Map();

function stop(reason) {
  worker?.terminate();
  worker = null;
  started = null;
  for (const [, c] of pending) {
    clearTimeout(c.timer);
    c.reject(new Error(reason));
  }
  pending.clear();
}

function post(message, limitMs, late) {
  const id = next++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      const call = pending.get(id);
      pending.delete(id);
      stop("Stopped with a plugin that ran too long");
      call?.reject(new Error(late));
    }, limitMs);
    pending.set(id, { resolve, reject, timer });
    worker.postMessage({ id, wasm: new URL(wasmUrl, location.href).href, ...message });
  });
}

function spawn() {
  worker = new Worker(new URL("../plugins-worker.js", import.meta.url), { type: "module" });
  worker.addEventListener("message", (e) => {
    const m = e.data;
    const call = pending.get(m?.id);
    if (!call) return;
    pending.delete(m.id);
    clearTimeout(call.timer);
    if (m.trap) stop("Stopped with a plugin that failed");
    if (m.error) call.reject(Object.assign(new Error(m.error), { trap: !!m.trap }));
    else call.resolve(m.result);
  });
  worker.addEventListener("error", (e) => stop(e.message || "The plugin runner didn't start"));
  started = post({ kind: "ready" }, START_MS, "The plugin runner didn't start");
}

// One call at a time: a call's 5 s start when it does, not while another one runs.
let queue = Promise.resolve();

function runHook(source, hook, args, name) {
  const run = async () => {
    if (!worker) spawn();
    await started;
    try {
      return await post({ kind: "hook", source, hook, args }, LIMIT_MS, `plugin '${name}' was stopped: it ran longer than ${LIMIT_MS / 1000} s`);
    } catch (e) {
      if (e.trap) throw new Error(`plugin '${name}' stopped: it used too much memory or failed (${e.message})`, { cause: e });
      throw e;
    }
  };
  const p = queue.then(run, run);
  queue = p.catch(() => {});
  return p;
}

// IndexedDB: { name → { manifest, source, sha256, installedAt } }.
function open() {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined" || !indexedDB) return reject(new Error("Plugins need IndexedDB, which this browser has turned off"));
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
    req.onblocked = () => reject(new Error("IndexedDB is blocked"));
  });
}

async function tx(mode, fn) {
  const db = await open();
  try {
    return await new Promise((resolve, reject) => {
      const t = db.transaction(STORE, mode);
      const req = fn(t.objectStore(STORE));
      t.oncomplete = () => resolve(req?.result);
      t.onerror = () => reject(t.error);
      t.onabort = () => reject(t.error);
    });
  } finally {
    db.close();
  }
}

export async function sha256(text) {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

const summary = (p) => ({ ...p.manifest, sha256: p.sha256 });

export const wasmPlugins = {
  async list() {
    const all = (await tx("readonly", (s) => s.getAll())) ?? [];
    return all.map(summary).sort((a, b) => a.name.localeCompare(b.name));
  },

  /** Checks and installs a plugin file's text; a second one with the same name needs `force`. */
  async install(source, { force = false } = {}) {
    if (new TextEncoder().encode(source).length > MAX_BYTES) throw new Error(`a plugin file may have at most ${MAX_BYTES} bytes`);
    const manifest = JSON.parse((await loadFormat()).pluginManifest(source));
    const old = await tx("readonly", (s) => s.get(manifest.name));
    if (old && !force) {
      const e = new Error(`A plugin named '${manifest.name}' is installed already (version ${old.manifest.version}, hooks: ${old.manifest.hooks.join(", ")}); this file is version ${manifest.version} (hooks: ${manifest.hooks.join(", ")})`);
      throw Object.assign(e, { installed: summary(old), manifest });
    }
    const entry = { manifest, source, sha256: await sha256(source), installedAt: Date.now() };
    await tx("readwrite", (s) => s.put(entry, manifest.name));
    return summary(entry);
  },

  async remove(name) {
    await tx("readwrite", (s) => s.delete(name));
  },

  /** The answer (text) of hook `hook` of plugin `name`, called with text arguments. */
  async call(name, hook, args) {
    const p = await tx("readonly", (s) => s.get(name));
    if (!p) throw new Error(`This request uses plugin '${name}', which isn't installed in this browser (Settings → Plugins)`);
    if (!p.manifest.hooks.includes(hook)) throw new Error(`plugin '${name}' has no ${hook} hook`);
    if ((await sha256(p.source)) !== p.sha256) throw new Error(`plugin '${name}' changed since it was installed; check it and install it again`);
    let answer;
    try {
      answer = JSON.parse(await runHook(p.source, hook, args, name));
    } catch (e) {
      throw new Error(/^plugin '/.test(e.message) || /^Stopped/.test(e.message) ? e.message : `plugin '${name}' failed: ${e.message}`, { cause: e });
    }
    if (typeof answer !== "string") throw new Error(`plugin '${name}' answered something other than text`);
    if (new TextEncoder().encode(answer).length > MAX_BYTES) throw new Error(`plugin '${name}' answered more than ${MAX_BYTES} bytes`);
    return answer;
  },
};
