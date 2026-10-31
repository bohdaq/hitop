// Request helpers the web app adds on top of the shared services: query
// parameters, auth presets, environments and assertions. Plain functions, so
// the CLI and desktop can port them as they are.

// --- query parameters ----------------------------------------------------------

// Splits a URL's query into rows without decoding or validating anything, so
// `${variables}` and half-typed URLs survive a round trip unchanged.
export function parseQuery(url) {
  const q = url.indexOf("?");
  if (q < 0) return [];
  let query = url.slice(q + 1);
  const hash = query.indexOf("#");
  if (hash >= 0) query = query.slice(0, hash);
  if (!query) return [];
  return query.split("&").map((part) => {
    const eq = part.indexOf("=");
    return eq < 0 ? { key: part, value: "" } : { key: part.slice(0, eq), value: part.slice(eq + 1) };
  });
}

// The URL with its query replaced by `rows` (the fragment is kept). Spaces
// and characters that would end a key or value are encoded; the rest is left
// as typed, ${...} included.
export function withQuery(url, rows) {
  const hashAt = url.indexOf("#");
  const hash = hashAt >= 0 ? url.slice(hashAt) : "";
  const noHash = hashAt >= 0 ? url.slice(0, hashAt) : url;
  const q = noHash.indexOf("?");
  const base = q >= 0 ? noHash.slice(0, q) : noHash;
  const enc = (s) => s.replace(/[\s&#=]/g, (c) => encodeURIComponent(c));
  const query = rows
    .filter((r) => r.key || r.value)
    .map((r) => (r.value === "" ? enc(r.key) : `${enc(r.key)}=${r.value.replace(/[\s&#]/g, (c) => encodeURIComponent(c))}`))
    .join("&");
  return `${base}${query ? `?${query}` : ""}${hash}`;
}

// --- auth -------------------------------------------------------------------------

export const AUTH_TYPES = [
  ["none", "No auth"],
  ["bearer", "Bearer token"],
  ["basic", "Basic auth"],
  ["apikey", "API key"],
];

export const emptyAuth = () => ({ type: "none", token: "", username: "", password: "", key: "X-API-Key", value: "", in: "header" });

// Only the fields the chosen type uses, for saving and exporting: switching
// from Basic to a token shouldn't leave the old password stored.
export function cleanAuth(auth) {
  if (!auth || auth.type === "none") return undefined;
  if (auth.type === "bearer") return { type: "bearer", token: auth.token || "" };
  if (auth.type === "basic") return { type: "basic", username: auth.username || "", password: auth.password || "" };
  if (auth.type === "apikey") return { type: "apikey", key: auth.key || "", value: auth.value || "", in: auth.in === "query" ? "query" : "header" };
  return undefined;
}

const base64 = (text) => btoa(String.fromCharCode(...new TextEncoder().encode(text)));

// Adds the auth header (or query parameter) to an already interpolated
// request. A header the user set by hand with the same name wins.
export function applyAuth(auth, request, interpolate = (s) => s) {
  if (!auth || auth.type === "none") return request;
  const has = (name) => request.headers.some((h) => h.name.toLowerCase() === name.toLowerCase() && h.value);
  const add = (name, value) => (has(name) ? request.headers : [...request.headers, { name, value }]);
  if (auth.type === "bearer" && auth.token) {
    return { ...request, headers: add("Authorization", `Bearer ${interpolate(auth.token)}`) };
  }
  if (auth.type === "basic" && (auth.username || auth.password)) {
    return { ...request, headers: add("Authorization", `Basic ${base64(`${interpolate(auth.username)}:${interpolate(auth.password)}`)}`) };
  }
  if (auth.type === "apikey" && auth.key && auth.value) {
    const key = interpolate(auth.key);
    const value = interpolate(auth.value);
    if (auth.in === "query") {
      const rows = parseQuery(request.url).filter((r) => r.key !== key);
      return { ...request, url: withQuery(request.url, [...rows, { key, value: encodeURIComponent(value) }]) };
    }
    return { ...request, headers: add(key, value) };
  }
  return request;
}

// --- environments -----------------------------------------------------------------

const ENV_KEY = "hitop_environments";
const ACTIVE_KEY = "hitop_active_environment";

export function loadEnvironments() {
  try {
    const list = JSON.parse(localStorage.getItem(ENV_KEY) || "[]");
    const active = localStorage.getItem(ACTIVE_KEY);
    return { environments: Array.isArray(list) ? list : [], activeEnvironmentId: active || null };
  } catch {
    return { environments: [], activeEnvironmentId: null };
  }
}

export function saveEnvironments(environments, activeId) {
  try {
    localStorage.setItem(ENV_KEY, JSON.stringify(environments));
    if (activeId) localStorage.setItem(ACTIVE_KEY, activeId);
    else localStorage.removeItem(ACTIVE_KEY);
  } catch {
    // Storage full or blocked: environments last for this page only.
  }
}

// Accepts HITOP's own export ([{name, variables}]) or a Postman environment
// ({name, values: [{key, value, enabled}]}), alone or in an array.
export function parseEnvironments(json) {
  const parsed = JSON.parse(json);
  const list = Array.isArray(parsed) ? parsed : [parsed];
  return list.map((env, i) => {
    if (!env || typeof env !== "object") throw new Error(`item ${i + 1} is not an object`);
    const variables = {};
    if (Array.isArray(env.values)) {
      for (const v of env.values) if (v?.key && v.enabled !== false) variables[v.key] = String(v.value ?? "");
    } else if (env.variables && typeof env.variables === "object") {
      for (const [k, v] of Object.entries(env.variables)) variables[k] = String(v ?? "");
    } else {
      throw new Error(`item ${i + 1} has no variables`);
    }
    return { id: crypto.randomUUID(), name: String(env.name || `Environment ${i + 1}`), variables };
  });
}

// --- assertions -------------------------------------------------------------------

// The `assert` object post-request scripts get. Each call records a result
// instead of throwing, so one failed check doesn't hide the others.
export function makeAssert({ statusCode, text, headers, durationMs }) {
  const results = [];
  const record = (name, pass, detail) => results.push({ name, pass: Boolean(pass), detail: pass ? "" : detail });
  const assert = {
    status(expected) {
      record(`status is ${expected}`, statusCode === Number(expected), `got ${statusCode}`);
    },
    bodyContains(fragment) {
      record(`body contains "${fragment}"`, String(text).includes(String(fragment)), "not found in the response body");
    },
    header(name, expected) {
      const actual = headers[String(name).toLowerCase()];
      if (expected === undefined) record(`has header ${name}`, actual !== undefined, "header missing");
      else record(`header ${name} is "${expected}"`, actual !== undefined && actual.includes(String(expected)), actual === undefined ? "header missing" : `got "${actual}"`);
    },
    responseTime(maxMs) {
      record(`responds within ${maxMs} ms`, durationMs != null && durationMs <= Number(maxMs), `took ${durationMs} ms`);
    },
    equal(actual, expected, name = `${JSON.stringify(actual)} equals ${JSON.stringify(expected)}`) {
      record(name, JSON.stringify(actual) === JSON.stringify(expected), `got ${JSON.stringify(actual)}`);
    },
    ok(condition, name = "condition is true") {
      record(name, condition, "was false");
    },
  };
  return { assert, results };
}

// --- cookie jar -----------------------------------------------------------------

// One Set-Cookie line as a jar entry, or null when it can't be stored for
// this URL (wrong domain, unparsable). `deleted` marks an expiry in the past.
export function parseSetCookie(line, requestUrl, now = Date.now()) {
  const host = new URL(requestUrl).hostname.toLowerCase();
  const [pair, ...attrs] = line.split(";").map((s) => s.trim());
  const eq = pair.indexOf("=");
  if (eq <= 0) return null;
  const cookie = { name: pair.slice(0, eq).trim(), value: pair.slice(eq + 1).trim(), domain: host, hostOnly: true, path: "/", expires: null, secure: false };
  const reqPath = new URL(requestUrl).pathname;
  cookie.path = reqPath.lastIndexOf("/") > 0 ? reqPath.slice(0, reqPath.lastIndexOf("/")) : "/";
  let maxAge = null;
  for (const attr of attrs) {
    const at = attr.indexOf("=");
    const key = (at < 0 ? attr : attr.slice(0, at)).trim().toLowerCase();
    const val = at < 0 ? "" : attr.slice(at + 1).trim();
    if (key === "domain" && val) {
      const d = val.replace(/^\./, "").toLowerCase();
      if (host !== d && !host.endsWith(`.${d}`)) return null; // another site's cookie
      cookie.domain = d;
      cookie.hostOnly = false;
    } else if (key === "path" && val.startsWith("/")) cookie.path = val;
    else if (key === "expires") {
      const t = Date.parse(val);
      if (!Number.isNaN(t)) cookie.expires = t;
    } else if (key === "max-age" && /^-?\d+$/.test(val)) maxAge = Number(val);
    else if (key === "secure") cookie.secure = true;
  }
  if (maxAge !== null) cookie.expires = now + maxAge * 1000;
  cookie.deleted = cookie.expires !== null && cookie.expires <= now;
  return cookie;
}

const sameCookie = (a, b) => a.name === b.name && a.domain === b.domain && a.path === b.path;

// The jar after `lines` (Set-Cookie values from one response) are applied.
export function mergeCookies(jar, lines, requestUrl, now = Date.now()) {
  let next = jar.filter((c) => c.expires === null || c.expires > now);
  for (const line of lines) {
    const cookie = parseSetCookie(line, requestUrl, now);
    if (!cookie) continue;
    next = next.filter((c) => !sameCookie(c, cookie));
    if (!cookie.deleted) {
      delete cookie.deleted;
      next.push(cookie);
    }
  }
  return next;
}

// The Cookie header value the jar has for this URL ("" when none).
export function cookieHeader(jar, requestUrl, now = Date.now()) {
  const url = new URL(requestUrl);
  const host = url.hostname.toLowerCase();
  const path = url.pathname || "/";
  return jar
    .filter((c) => c.expires === null || c.expires > now)
    .filter((c) => (c.hostOnly ? host === c.domain : host === c.domain || host.endsWith(`.${c.domain}`)))
    .filter((c) => path === c.path || path.startsWith(c.path.endsWith("/") ? c.path : `${c.path}/`) || c.path === "/")
    .filter((c) => !c.secure || url.protocol === "https:")
    .sort((a, b) => b.path.length - a.path.length)
    .map((c) => `${c.name}=${c.value}`)
    .join("; ");
}

// --- form bodies --------------------------------------------------------------------

// fields: [{ key, value, type: "text" | "file", enabled }]; files: Map of
// field id -> File (files are never stored, only kept while the page is open).
export async function multipartBody(fields, files) {
  const boundary = `----hitop${crypto.randomUUID().replace(/-/g, "")}`;
  const enc = new TextEncoder();
  const parts = [];
  const quote = (s) => String(s).replace(/"/g, "%22").replace(/\r|\n/g, " ");
  for (const f of fields) {
    if (f.enabled === false || !f.key) continue;
    if (f.type === "file") {
      const file = files.get(f.id);
      const name = file?.name ?? f.fileName ?? "";
      parts.push(enc.encode(`--${boundary}\r\nContent-Disposition: form-data; name="${quote(f.key)}"; filename="${quote(name)}"\r\nContent-Type: ${file?.type || "application/octet-stream"}\r\n\r\n`));
      parts.push(file ? new Uint8Array(await file.arrayBuffer()) : new Uint8Array());
      parts.push(enc.encode("\r\n"));
    } else {
      parts.push(enc.encode(`--${boundary}\r\nContent-Disposition: form-data; name="${quote(f.key)}"\r\n\r\n${f.value ?? ""}\r\n`));
    }
  }
  parts.push(enc.encode(`--${boundary}--\r\n`));
  const body = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    body.set(p, at);
    at += p.length;
  }
  return { body, contentType: `multipart/form-data; boundary=${boundary}` };
}

export function urlEncodedBody(fields) {
  const body = fields
    .filter((f) => f.enabled !== false && f.key)
    .map((f) => `${encodeURIComponent(f.key)}=${encodeURIComponent(f.value ?? "")}`)
    .join("&");
  return { body, contentType: "application/x-www-form-urlencoded" };
}
