// The browser: requests go straight out (fetch) or through the HITOP proxy,
// data stays in localStorage under the keys the React app used, and
// collection folders open through the File System Access API where the
// browser has it (Chrome, Edge).
import storageService from "../../../frontend/src/services/storageService.jsx";
import { localSaves } from "./local-storage.js";
import { sendRequest, usesProxy } from "../../../frontend/src/services/transportService.jsx";
import { loadEnvironments, loadGlobals, saveEnvironments, saveGlobals } from "../features.js";
import { loadFormat } from "../wasm/format.js";
import { openResponseStore } from "./response-store.js";

const FOLDER_SECRETS_KEY = "hitop_folder_secrets";
const DB = "hitop";
const STORE = "folders";

// --- directory handles -------------------------------------------------------

// Every file under `dir` the format reads: hitop.yaml, requests/ and
// environments/, by path. Hidden entries are skipped.
async function listFiles(dir, prefix = "", out = new Map()) {
  for await (const [name, handle] of dir.entries()) {
    if (name.startsWith(".")) continue;
    const path = prefix + name;
    if (handle.kind === "directory") {
      if (prefix || name === "requests" || name === "environments") await listFiles(handle, `${path}/`, out);
    } else if (/\.ya?ml$/.test(name) && (prefix || name === "hitop.yaml")) {
      out.set(path, handle);
    }
  }
  return out;
}

async function readTexts(dir) {
  const files = {};
  for (const [path, handle] of await listFiles(dir)) files[path] = await (await handle.getFile()).text();
  return files;
}

async function walkTo(dir, parts, create) {
  for (const part of parts) dir = await dir.getDirectoryHandle(part, { create });
  return dir;
}

async function removeEmptyDirs(dir) {
  let empty = true;
  for await (const [name, handle] of dir.entries()) {
    if (handle.kind === "directory" && (await removeEmptyDirs(handle))) await dir.removeEntry(name);
    else empty = false;
  }
  return empty;
}

// Writes `folder` into the directory as hitop-core would: changed files
// only, an existing .yml keeps its extension, stale request and
// environment files go.
export async function writeDirectory(dir, folder) {
  const format = await loadFormat();
  const files = format.toFiles(folder);
  const existing = await readTexts(dir);
  const plan = format.planSync(Object.keys(existing), files);
  for (const [path, text] of Object.entries(plan.write)) {
    if (existing[path] === text) continue;
    const parts = path.split("/");
    const parent = await walkTo(dir, parts.slice(0, -1), true);
    const writable = await (await parent.getFileHandle(parts.at(-1), { create: true })).createWritable();
    await writable.write(text);
    await writable.close();
  }
  for (const path of plan.remove) {
    const parts = path.split("/");
    await (await walkTo(dir, parts.slice(0, -1), false)).removeEntry(parts.at(-1));
  }
  try {
    const requests = await dir.getDirectoryHandle("requests");
    for await (const [name, handle] of requests.entries()) if (handle.kind === "directory" && (await removeEmptyDirs(handle))) await requests.removeEntry(name);
  } catch {
    // no requests/ folder
  }
}

export async function readDirectory(dir) {
  const format = await loadFormat();
  return format.readFiles(await readTexts(dir), dir.name);
}

// Handles survive a reload in IndexedDB; permission has to be asked again.
function db() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function idb(mode, fn) {
  const d = await db();
  return new Promise((resolve, reject) => {
    const tx = d.transaction(STORE, mode);
    const req = fn(tx.objectStore(STORE));
    tx.oncomplete = () => resolve(req?.result);
    tx.onerror = () => reject(tx.error);
  });
}

function folders() {
  if (typeof window.showDirectoryPicker !== "function") return null;
  return {
    async pick() {
      try {
        return await window.showDirectoryPicker({ id: "hitop-collection", mode: "readwrite" });
      } catch (e) {
        if (e.name === "AbortError") return null;
        throw e;
      }
    },
    read: readDirectory,
    write: writeDirectory,
    label: (dir) => dir.name,
    // [{ collectionId, ref }]; the app reads them again on "Reconnect".
    async restore() {
      try {
        const keys = await idb("readonly", (s) => s.getAllKeys());
        const values = await idb("readonly", (s) => s.getAll());
        return keys.map((k, i) => ({ collectionId: k, ref: values[i], connected: false }));
      } catch {
        return [];
      }
    },
    // Permission for a handle from an earlier visit: needs a click.
    async reconnect(dir) {
      return (await dir.requestPermission({ mode: "readwrite" })) === "granted";
    },
    remember: (collectionId, dir) => idb("readwrite", (s) => s.put(dir, collectionId)).catch(() => {}),
    forget: (collectionId) => idb("readwrite", (s) => s.delete(collectionId)).catch(() => {}),
  };
}

// --- the adapter ---------------------------------------------------------------

// Send with options.signal: a direct fetch is aborted by it; a request
// through the proxy can't be (sendRequest's own fetch, shared with the
// React app), so its answer is dropped instead.
function send(url, options = {}) {
  const { signal } = options;
  const sent = sendRequest(url, options);
  if (!signal) return sent;
  if (signal.aborted) return Promise.reject(signal.reason);
  return new Promise((resolve, reject) => {
    const stop = () => reject(signal.reason);
    signal.addEventListener("abort", stop, { once: true });
    sent.then(
      (res) => (signal.removeEventListener("abort", stop), resolve(res)),
      (e) => (signal.removeEventListener("abort", stop), reject(e)),
    );
  });
}

// Server-Sent Events and other streamed bodies: public hosts through the proxy's stream endpoint
// (hitop-server POST /api/proxy/stream), the rest straight from the browser. `onText` gets each
// piece as it arrives; the answer (after the end, or Stop: `stopped`) looks like send()'s.
const STREAM_URL = import.meta.env.VITE_HITOP_PROXY ? `${import.meta.env.VITE_HITOP_PROXY}/stream` : "";
async function stream(url, options = {}, onText = () => {}) {
  const started = performance.now();
  const { signal } = options;
  const viaProxy = Boolean(STREAM_URL) && usesProxy(url);
  const headers = options.headers || {};
  const res = viaProxy
    ? await fetch(STREAM_URL, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ method: options.method || "GET", url, headers: Object.entries(headers), body: options.body ?? null }), signal })
    : await fetch(url, { method: options.method || "GET", headers, body: options.body, signal });
  if (viaProxy && !res.headers.get("x-hitop-status")) {
    const why = await res.json().catch(() => ({}));
    throw new Error(why.error || `the proxy answered ${res.status}`);
  }
  const status = viaProxy ? Number(res.headers.get("x-hitop-status")) : res.status;
  const pairs = viaProxy ? JSON.parse(res.headers.get("x-hitop-headers") || "[]") : [...res.headers];
  let text = "";
  let stopped = false;
  const decoder = new TextDecoder();
  const reader = res.body.getReader();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      const piece = decoder.decode(value, { stream: true });
      text += piece;
      onText(piece);
    }
  } catch (e) {
    if (!signal?.aborted) throw e;
    stopped = true;
  }
  const lower = new Map(pairs.map(([k, v]) => [k.toLowerCase(), v]));
  return { status, headers: { get: (k) => lower.get(k.toLowerCase()) ?? null, forEach: (fn) => pairs.forEach(([k, v]) => fn(v, k)), pairs }, text: async () => text, timeMs: Math.round(performance.now() - started), stopped, url };
}

// WebSocket requests with handshake headers to public hosts go through hitop-server's bridge
// (GET /api/ws, wsbridge.rs): the page sends {url, headers, protocols} as the first frame, the
// server connects with them and relays frames. Localhost stays a direct browser connection.
const BRIDGE_URL = import.meta.env.VITE_HITOP_PROXY ? `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}${import.meta.env.VITE_HITOP_PROXY.replace(/\/proxy$/, "/ws")}` : "";
const httpForm = (url) => String(url).replace(/^ws(s?):\/\//i, "http$1://");
const sendsHeaders = (url) => Boolean(BRIDGE_URL) && usesProxy(httpForm(url));

function bridgedWebSocket(url, { headers = [], protocols = [] } = {}, h) {
  let ws;
  let opened = false;
  let closed = false;
  try {
    ws = new WebSocket(BRIDGE_URL);
  } catch (e) {
    queueMicrotask(() => {
      h.onError(e.message);
      h.onClose({ code: null, reason: "" });
    });
    return { send() {}, close() {} };
  }
  ws.binaryType = "arraybuffer";
  ws.onopen = () => ws.send(JSON.stringify({ url, headers: headers.map((x) => [x.name, x.value]), protocols }));
  ws.onmessage = (e) => {
    if (!opened) {
      let m = null;
      try {
        m = JSON.parse(e.data);
      } catch {
        // not the bridge's answer
      }
      if (m?.type === "open") {
        opened = true;
        h.onOpen({ protocol: m.protocol || null });
      } else if (m?.type === "error") h.onError(m.message);
      return;
    }
    if (typeof e.data === "string") h.onMessage({ text: e.data });
    else h.onMessage({ bytes: new Uint8Array(e.data) });
  };
  ws.onclose = (e) => {
    if (closed) return;
    closed = true;
    if (!opened && !e.reason) h.onError("the HITOP WebSocket bridge couldn't be reached");
    h.onClose({ code: opened ? e.code : null, reason: opened ? e.reason : "" });
  };
  return {
    send: (text) => opened && ws.send(text),
    close: () => ws.close(1000),
  };
}

// gRPC through hitop-server's bridge (POST /api/grpc/{list,describe,call}, grpcbridge.rs): public
// servers with server reflection only; the call streams one JSON event per line.
const GRPC_BASE = import.meta.env.VITE_HITOP_PROXY ? import.meta.env.VITE_HITOP_PROXY.replace(/\/proxy$/, "/grpc") : "";
async function grpcPost(path, body, signal) {
  const r = await fetch(`${GRPC_BASE}/${path}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body), signal });
  if (!r.ok) {
    const e = await r.json().catch(() => ({}));
    throw new Error(e.error || `the gRPC bridge answered ${r.status}`);
  }
  return r;
}
const bridgeGrpc = GRPC_BASE
  ? {
      reflectionOnly: true,
      list: async (server) => (await grpcPost("list", server)).json(),
      describe: async (server, symbol) => (await grpcPost("describe", { ...server, symbol })).json(),
      call(args, h) {
        const ctl = new AbortController();
        let cancelled = false;
        (async () => {
          try {
            const r = await grpcPost("call", args, ctl.signal);
            const reader = r.body.getReader();
            const dec = new TextDecoder();
            let buf = "";
            for (;;) {
              const { done, value } = await reader.read();
              if (done) break;
              buf += dec.decode(value, { stream: true });
              let at;
              while ((at = buf.indexOf("\n")) >= 0) {
                const line = buf.slice(0, at);
                buf = buf.slice(at + 1);
                if (!line.trim() || cancelled) continue;
                const e = JSON.parse(line);
                if (e.type === "headers") h.onHeaders(e.metadata || []);
                else if (e.type === "message") h.onMessage(e.data);
                else if (e.type === "status") h.onStatus(e);
              }
            }
          } catch (e) {
            if (!cancelled) h.onError(e.message || String(e));
          }
        })();
        return {
          cancel: () => {
            cancelled = true;
            ctl.abort();
          },
        };
      },
    }
  : null;

function webSocketFor(url, options, h) {
  return options?.headers?.length && sendsHeaders(url) ? bridgedWebSocket(url, options, h) : browserWebSocket(url, options, h);
}

// A WebSocket request in the browser: the URL and subprotocols only, since
// a page can't set handshake headers. Errors carry no detail in browsers,
// so the message says what usually causes them.
function browserWebSocket(url, { protocols = [] } = {}, h) {
  let ws;
  let opened = false;
  try {
    ws = new WebSocket(url, protocols.length ? protocols : undefined);
  } catch (e) {
    queueMicrotask(() => {
      h.onError(e.message);
      h.onClose({ code: null, reason: "" });
    });
    return { send() {}, close() {} };
  }
  ws.binaryType = "arraybuffer";
  ws.onopen = () => {
    opened = true;
    h.onOpen({ protocol: ws.protocol || null });
  };
  ws.onmessage = (e) => (typeof e.data === "string" ? h.onMessage({ text: e.data }) : h.onMessage({ bytes: new Uint8Array(e.data) }));
  ws.onclose = (e) => {
    if (!opened) {
      const mixed = location.protocol === "https:" && /^ws:/i.test(url) && !/^ws:\/\/(localhost|127\.0\.0\.1|\[::1\])[:/]/i.test(url);
      h.onError(mixed ? "this page is https, so the browser allows only wss:// (or ws://localhost)" : "the server refused or isn't reachable (the browser gives no details)");
    }
    h.onClose({ code: e.code, reason: e.reason });
  };
  return {
    send: (text) => ws.send(text),
    close: () => ws.close(1000),
  };
}

export function createPlatform() {
  return {
    name: "web",
    // Large response bodies across reloads (IndexedDB), or null without it.
    responses: null,
    async init() {
      this.responses = await openResponseStore();
    },
    storage: {
      ...storageService,
      ...localSaves,
      loadEnvironments,
      saveEnvironments,
      loadGlobals,
      saveGlobals,
    },
    send,
    // The browser's WebSocket: no handshake headers (browsers can't set them).
    websocketHeaders: false,
    websocket: webSocketFor,
    websocketSendsHeaders: sendsHeaders,
    // gRPC needs HTTP/2 trailers, which a page can't read: desktop and CLI only.
    grpc: bridgeGrpc,
    // Git is the desktop app's (libgit2 in Rust).
    git: null,
    // ${secret:...} references (Vault, AWS, Azure, 1Password): desktop and CLI only.
    resolveSecretRefs: null,
    // The browser handles cookies of direct requests itself (and hides them);
    // the proxy's answers carry Set-Cookie for the app's jar.
    managesCookies: usesProxy,
    cookies: null,
    cookiesNote: "Applies to requests that go through the HITOP proxy. For localhost your browser keeps cookies itself. Cookies stay in this browser.",
    usesProxy,
    stream,
    scripts: null,
    folders: folders(),
    // Folder environments' secret values: in this browser, by keychain account.
    folderSecrets: {
      load(accounts) {
        let all = {};
        try {
          all = JSON.parse(localStorage.getItem(FOLDER_SECRETS_KEY) || "{}");
        } catch {
          // unreadable: start empty
        }
        return Object.fromEntries(accounts.filter((a) => a in all).map((a) => [a, all[a]]));
      },
      save(values) {
        try {
          const all = { ...JSON.parse(localStorage.getItem(FOLDER_SECRETS_KEY) || "{}"), ...values };
          for (const [k, v] of Object.entries(values)) if (v === null) delete all[k];
          localStorage.setItem(FOLDER_SECRETS_KEY, JSON.stringify(all));
        } catch {
          // storage full or blocked
        }
      },
    },
    async saveFile(name, data, type = "application/json") {
      const url = URL.createObjectURL(new Blob([data], { type }));
      const a = Object.assign(document.createElement("a"), { href: url, download: name });
      a.click();
      URL.revokeObjectURL(url);
      return true;
    },
    secretsNote: "Secret values stay in this browser. They are masked here and in history, and never written to exports: an export keeps only their names.",
  };
}
