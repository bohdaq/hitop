// The desktop app (Tauri): the web app runs in the app's window, and Rust
// does the rest through `invoke` (desktop/src-tauri/src/commands.rs):
// requests (reqwest: no CORS, no proxy), scripts (boa, as in the CLI),
// storage in hitop-core's files (collections.json, history.json,
// contexts.json, environments.json, shared with the CLI and the old React
// desktop), secret values in the OS keychain, and collection folders on
// disk. Open tabs stay in the window's localStorage.
import storageService from "../../../frontend/src/services/storageService.jsx";
import { historyFromRust, historyToRust } from "./history.js";
import { askOnce, settingsPages } from "./tauri-settings.js";

// What @tauri-apps/api's invoke() calls; the e2e tests put a mock here.
const invoke = (cmd, args = {}) => window.__TAURI_INTERNALS__.invoke(cmd, args);

const FOLDERS_KEY = "hitop_folders";
const COOKIES_KEY = "hitop_cookies";
const GLOBAL = "global";

const account = (workspace, env, name) => `${workspace}/${env}/${name}`;

/** Read only Headers lookalike over the response's header pairs. */
class NativeHeaders {
  constructor(pairs) {
    this.pairs = pairs;
    this.map = new Map();
    for (const [name, value] of pairs) {
      const key = name.toLowerCase();
      this.map.set(key, this.map.has(key) ? `${this.map.get(key)}${key === "set-cookie" ? "\n" : ", "}${value}` : value);
    }
  }
  get(name) {
    return this.map.get(name.toLowerCase()) ?? null;
  }
  has(name) {
    return this.map.has(name.toLowerCase());
  }
  forEach(fn) {
    this.map.forEach((v, k) => fn(v, k, this));
  }
}

const toBase64 = (bytes) => {
  let text = "";
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text);
};

const fromBase64 = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));

const headerPairs = (headers) => (Array.isArray(headers) ? headers.map((h) => [h.name, h.value]) : Object.entries(headers || {}));

// The request goes out as the proxy's envelope, so both look the same to
// the app (and to the tests).
async function send(url, options = {}) {
  const binary = options.body instanceof Uint8Array;
  const r = await invoke("send_request", {
    args: {
      method: options.method || "GET",
      url,
      headers: headerPairs(options.headers),
      body: binary ? toBase64(options.body) : (options.body ?? null),
      ...(binary ? { bodyEncoding: "base64" } : {}),
      // hitop-core's cookie jar (shared with `hitop send`) sends and keeps cookies.
      cookies: Boolean(options.cookies),
    },
  });
  const text = r.bodyEncoding === "base64" ? new TextDecoder().decode(fromBase64(r.body)) : r.body;
  return {
    status: r.status,
    statusText: r.statusText,
    ok: r.status >= 200 && r.status < 300,
    url: r.url,
    headers: new NativeHeaders(r.headers),
    timeMs: r.timeMs,
    // The jar after this response (when the request used it).
    cookies: r.cookies,
    text: async () => text,
    json: async () => JSON.parse(text),
  };
}

// --- secrets ------------------------------------------------------------------------

// Writes the values that changed since `known` (account → value) and
// deletes the ones that are gone. Returns the new known map.
async function syncSecrets(known, wanted, onError) {
  for (const [acc, value] of Object.entries(wanted)) {
    if (known[acc] === value) continue;
    try {
      await invoke("secret_set", { account: acc, value });
      known[acc] = value;
    } catch (e) {
      onError(`Couldn't keep a secret in the system keychain: ${e}`);
    }
  }
  for (const acc of Object.keys(known)) {
    if (acc in wanted) continue;
    try {
      await invoke("secret_delete", { account: acc });
    } catch {
      // already gone
    }
    delete known[acc];
  }
  return known;
}

async function loadSecretValues(accounts) {
  const out = {};
  for (const acc of accounts) {
    try {
      const v = await invoke("secret_get", { account: acc });
      if (v != null) out[acc] = v;
    } catch {
      // no keychain: the value stays empty
    }
  }
  return out;
}

// --- the adapter ---------------------------------------------------------------------

export function createPlatform() {
  const cache = { collections: null, history: [], contexts: {}, environments: { environments: [], activeEnvironmentId: null }, cookies: [] };
  const last = {};
  let knownSecrets = {};
  let report = (message) => console.error(message);
  // Collections that live in a folder are kept out of collections.json.
  let folderIds = new Set();

  // Writes when the JSON changed; storage errors are reported once each.
  const write = (kind, cmd, args) => {
    const json = JSON.stringify(args);
    if (last[kind] === json) return;
    last[kind] = json;
    invoke(cmd, args).catch((e) => {
      last[kind] = null;
      report(`Couldn't save ${kind}: ${e}`);
    });
  };

  const telemetry = {
    status: () => invoke("telemetry_status"),
    set: (enabled) => invoke("telemetry_set", { enabled }),
  };
  const platform = {
    name: "tauri",
    onError(fn) {
      report = fn;
    },
    setFolderCollections(ids) {
      folderIds = new Set(ids);
    },
    async init() {
      const [collections, history, contexts, envs, cookies] = await Promise.all([
        invoke("load_collections").catch(() => null),
        invoke("load_history").catch(() => []),
        invoke("load_contexts").catch(() => ({})),
        invoke("load_environments").catch(() => ({ active: null, environments: [] })),
        invoke("cookies_load").catch(() => []),
      ]);
      cache.cookies = Array.isArray(cookies) ? cookies : [];
      // Before the Rust jar, the desktop kept cookies in the window's
      // localStorage (as the web app does): move them over once.
      if (!cache.cookies.length) {
        try {
          const old = JSON.parse(localStorage.getItem(COOKIES_KEY) || "[]");
          if (Array.isArray(old) && old.length) {
            await invoke("cookies_save", { cookies: old });
            cache.cookies = old;
          }
          localStorage.removeItem(COOKIES_KEY);
        } catch {
          // nothing stored, or not readable: start empty
        }
      }
      // The usage ping's setting (absent in Mac App Store builds: no page then).
      const settings = await settingsPages(telemetry);
      platform.pages = settings.pages;
      platform.onStart = () => askOnce(telemetry, settings.status);
      cache.collections = collections?.length ? collections : null;
      cache.history = historyFromRust(history);
      cache.contexts = contexts && typeof contexts === "object" ? contexts : {};
      const environments = envs?.environments ?? [];
      const accounts = environments.flatMap((e) => (e.secrets || []).map((n) => account(GLOBAL, e.name, n)));
      knownSecrets = await loadSecretValues(accounts);
      cache.environments = {
        environments: environments.map((e) => ({
          ...e,
          variables: { ...e.variables, ...Object.fromEntries((e.secrets || []).map((n) => [n, knownSecrets[account(GLOBAL, e.name, n)] ?? e.variables?.[n] ?? ""])) },
        })),
        activeEnvironmentId: envs?.active ?? null,
      };
    },
    storage: {
      loadCollections: () => cache.collections,
      saveCollections: (collections) => write("collections", "save_collections", { collections: collections.filter((c) => !folderIds.has(c.id)) }),
      loadTabs: storageService.loadTabs,
      saveTabs: storageService.saveTabs,
      loadCurrentTab: storageService.loadCurrentTab,
      saveCurrentTab: storageService.saveCurrentTab,
      loadHistory: () => cache.history,
      saveHistory: (history) => write("history", "save_history", { history: historyToRust(history) }),
      loadContexts: () => cache.contexts,
      saveContexts: (contexts) => write("contexts", "save_contexts", { contexts }),
      loadEnvironments: () => cache.environments,
      // Secret values go to the keychain (account global/<env>/<name>), the
      // rest to environments.json, as `hitop secret set` does.
      saveEnvironments(environments, activeId) {
        const wanted = {};
        const stored = environments.map((e) => {
          const names = e.secrets || [];
          for (const n of names) if (e.variables?.[n]) wanted[account(GLOBAL, e.name, n)] = e.variables[n];
          return { ...e, variables: Object.fromEntries(Object.entries(e.variables || {}).filter(([k]) => !names.includes(k))) };
        });
        write("environments", "save_environments", { environments: { active: activeId ?? null, environments: stored } });
        const json = JSON.stringify(wanted);
        if (last.secrets !== json) {
          last.secrets = json;
          syncSecrets(knownSecrets, wanted, report);
        }
      },
    },
    send,
    // The Rust jar handles cookies (send passes options.cookies on), so the
    // page adds no Cookie header itself.
    managesCookies: () => false,
    cookies: {
      load: () => cache.cookies,
      save: (cookies) => write("cookies", "cookies_save", { cookies }),
    },
    cookiesNote: "Applies to every request. The desktop app keeps cookies in hitop's settings folder, in the same jar as the CLI's hitop send (see hitop cookies ls).",
    usesProxy: () => false,
    scripts: {
      async pre({ script, url, method, headers, body, variables, context }) {
        const r = await invoke("run_pre_script", { args: { script, url, method, headers: headers.filter((h) => h.name).map((h) => [h.name, h.value]), body: body || "", variables, context } });
        return { url: r.url, headers: r.headers.map(([name, value]) => ({ name, value })), body: r.body, contextUpdates: r.context_updates || {}, variables: r.variables || {} };
      },
      async post({ script, text, headers, status, durationMs, sent, variables, context }) {
        // pm.request: the request as sent (a multipart body with files is bytes: left out).
        const request = sent && { url: sent.url ?? "", method: sent.method ?? "", headers: (sent.headers ?? []).filter((h) => h.name).map((h) => [h.name, h.value]), body: typeof sent.body === "string" ? sent.body : "" };
        const r = await invoke("run_post_script", { args: { script, response_body: text, response_headers: headers, status_code: status, duration_ms: Math.round(durationMs || 0), variables, context, request } });
        return { tests: r.tests || [], contextUpdates: r.context_updates || {}, variables: r.variables || {} };
      },
    },
    folders: {
      async pick() {
        return (await invoke("plugin:dialog|open", { options: { directory: true, multiple: false, title: "Choose a collection folder" } })) ?? null;
      },
      read: (path) => invoke("read_folder", { path }),
      write: (path, folder) => invoke("write_folder", { path, collection: folder.collection, environments: folder.environments }),
      label: (path) => String(path).split(/[\\/]/).filter(Boolean).pop() ?? String(path),
      async restore() {
        try {
          return (JSON.parse(localStorage.getItem(FOLDERS_KEY) || "[]") || []).map((f) => ({ collectionId: f.collectionId, ref: f.path, connected: true }));
        } catch {
          return [];
        }
      },
      reconnect: async () => true,
      remember(collectionId, path) {
        const list = (() => {
          try {
            return JSON.parse(localStorage.getItem(FOLDERS_KEY) || "[]");
          } catch {
            return [];
          }
        })().filter((f) => f.collectionId !== collectionId && f.path !== path);
        localStorage.setItem(FOLDERS_KEY, JSON.stringify([...list, { collectionId, path }]));
      },
      forget(collectionId) {
        try {
          localStorage.setItem(FOLDERS_KEY, JSON.stringify(JSON.parse(localStorage.getItem(FOLDERS_KEY) || "[]").filter((f) => f.collectionId !== collectionId)));
        } catch {
          // nothing stored
        }
      },
    },
    // Folder environments' secret values: in the keychain under the
    // folder's workspace id, where the CLI looks for them.
    folderSecrets: {
      load: loadSecretValues,
      save(values) {
        for (const [acc, value] of Object.entries(values)) {
          (value === null ? invoke("secret_delete", { account: acc }) : invoke("secret_set", { account: acc, value })).catch((e) => report(`Couldn't keep a secret in the system keychain: ${e}`));
        }
      },
    },
    async saveFile(name, data, type = "application/json") {
      const extension = name.split(".").pop();
      const path = await invoke("plugin:dialog|save", { options: { defaultPath: name, filters: [{ name: type.includes("zip") ? "Zip archive" : "JSON", extensions: [extension] }] } });
      if (!path) return false;
      const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
      await invoke("write_file", { path, data: toBase64(bytes) });
      return true;
    },
    secretsNote: "Secret values are kept in your system keychain. They are masked here and in history, and never written to exports or collection files: those keep only their names.",
  };
  return platform;
}
