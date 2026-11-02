// The browser: requests go straight out (fetch) or through the HITOP proxy,
// data stays in localStorage under the keys the React app used, and
// collection folders open through the File System Access API where the
// browser has it (Chrome, Edge).
import storageService from "../../../frontend/src/services/storageService.jsx";
import { localSaves } from "./local-storage.js";
import { sendRequest, usesProxy } from "../../../frontend/src/services/transportService.jsx";
import { loadEnvironments, loadGlobals, saveEnvironments, saveGlobals } from "../features.js";
import { loadFormat } from "../wasm/format.js";

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
    async init() {},
    storage: {
      ...storageService,
      ...localSaves,
      loadEnvironments,
      saveEnvironments,
      loadGlobals,
      saveGlobals,
    },
    send: sendRequest,
    // The browser's WebSocket: no handshake headers (browsers can't set them).
    websocketHeaders: false,
    websocket: browserWebSocket,
    // gRPC needs HTTP/2 trailers, which a page can't read: desktop and CLI only.
    grpc: null,
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
