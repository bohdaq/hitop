// What a request gets from its collection and folders, its path variables and its own sending
// options: pure helpers the store sends with (and node unit tests check, tests/unit-ux3.mjs).
// The same rules as hitop-core (model.rs `Collection::effective`, `Request::settings`).
import { folderOf as folderOfRequest } from "./collection-tree.js";

// A request's path variables as saved (`pathVariables`): rows with a name, for `:name` in its URL.
export function cleanPathVariables(t) {
  const rows = (t.pathVariables || []).filter((p) => p?.key);
  return rows.length ? rows.map((p) => ({ key: p.key, value: p.value ?? "" })) : undefined;
}

/** The `:name` path segments of a URL, in order (`/users/:id/posts/:postId` → id, postId). */
export function pathVariableNames(url) {
  const path = String(url || "").replace(/^[a-z][a-z0-9+.-]*:\/\/[^/]*/i, "").split(/[?#]/)[0];
  const out = [];
  for (const m of path.matchAll(/\/:([A-Za-z_][\w-]*)/g)) if (!out.includes(m[1])) out.push(m[1]);
  return out;
}

/** `:name` segments replaced by their values (encoded); names without a value stay as they are. */
export function applyPathVariables(url, rows, fill = (x) => x) {
  const values = new Map((rows || []).filter((r) => r?.key && r.value !== undefined && r.value !== "").map((r) => [r.key, fill(String(r.value))]));
  if (!values.size) return url;
  const at = String(url).search(/[?#]/);
  const [head, tail] = at === -1 ? [url, ""] : [url.slice(0, at), url.slice(at)];
  return head.replace(/\/:([A-Za-z_][\w-]*)(?=\/|$)/g, (whole, name) => (values.has(name) ? `/${encodeURIComponent(values.get(name))}` : whole)) + tail;
}

/**
 * A request as it is sent with its collection's and folders' defaults (hitop-core
 * `Collection::effective`, the same rules as the CLI): with no auth of its own ("Inherit") it gets
 * the nearest folder's, else the collection's; inherited headers go first, its own replace them by
 * name. `{type: "none"}` is "No auth" and wins.
 */
export function inheritDefaults(source, collection) {
  if (!collection) return source;
  const layers = [{ auth: collection.auth, headers: collection.headers }];
  const folder = folderOfRequest(source);
  const defaults = collection.folderDefaults || {};
  if (folder) {
    const parts = folder.split("/");
    for (let i = 1; i <= parts.length; i++) if (defaults[parts.slice(0, i).join("/")]) layers.push(defaults[parts.slice(0, i).join("/")]);
  }
  const ownAuth = source.auth && source.auth.type !== "inherit" ? source.auth : null;
  const auth = ownAuth ?? [...layers].reverse().find((l) => l.auth && l.auth.type !== "inherit")?.auth ?? null;
  const own = (source.headers || []).filter((h) => h.enabled !== false && h.name);
  const ownNames = new Set(own.map((h) => h.name.toLowerCase()));
  const inherited = [];
  for (const l of layers) {
    for (const h of l.headers || []) {
      if (h.enabled === false || !h.name) continue;
      const i = inherited.findIndex((x) => x.name.toLowerCase() === h.name.toLowerCase());
      if (i !== -1) inherited.splice(i, 1);
      inherited.push({ name: h.name, value: h.value ?? "" });
    }
  }
  return { ...source, auth: auth ?? { type: "none" }, headers: [...inherited.filter((h) => !ownNames.has(h.name.toLowerCase())), ...(source.headers || [])] };
}

/** Where a request's inherited auth comes from: { auth, from } ("folder orders", "the collection") or null. */
export function inheritedAuth(source, collection) {
  if (!collection) return null;
  const folder = folderOfRequest(source);
  const defaults = collection.folderDefaults || {};
  if (folder) {
    const parts = folder.split("/");
    for (let i = parts.length; i >= 1; i--) {
      const path = parts.slice(0, i).join("/");
      const a = defaults[path]?.auth;
      if (a && a.type !== "inherit") return { auth: a, from: `folder ${path}` };
    }
  }
  return collection.auth && collection.auth.type !== "inherit" ? { auth: collection.auth, from: "the collection" } : null;
}

// The time limit for one request in ms: its own (Settings tab), else the app's (Settings page).
export function requestTimeout(source, settings) {
  const own = Number(source?.settings?.timeoutMs);
  return own > 0 ? own : Number(settings?.timeoutMs) || 0;
}

// A request's own sending options as saved (`settings`): only what differs from the defaults.
export function cleanSettings(settings) {
  if (!settings || typeof settings !== "object") return undefined;
  const out = {};
  if (settings.followRedirects === false) out.followRedirects = false;
  if (Number(settings.timeoutMs) > 0) out.timeoutMs = Math.round(Number(settings.timeoutMs));
  return Object.keys(out).length ? out : undefined;
}

