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
  ["oauth2", "OAuth 2.0"],
];

export const emptyAuth = () => ({ type: "none", token: "", username: "", password: "", key: "X-API-Key", value: "", in: "header", grant: "client_credentials", tokenUrl: "", clientId: "", clientSecret: "", scope: "", clientAuth: "" });

// Only the fields the chosen type uses, for saving and exporting: switching
// from Basic to a token shouldn't leave the old password stored.
export function cleanAuth(auth) {
  if (!auth || auth.type === "none") return undefined;
  if (auth.type === "bearer") return { type: "bearer", token: auth.token || "" };
  if (auth.type === "basic") return { type: "basic", username: auth.username || "", password: auth.password || "" };
  if (auth.type === "apikey") return { type: "apikey", key: auth.key || "", value: auth.value || "", in: auth.in === "query" ? "query" : "header" };
  if (auth.type === "oauth2") {
    const out = { type: "oauth2", grant: auth.grant === "password" ? "password" : "client_credentials", tokenUrl: auth.tokenUrl || "", clientId: auth.clientId || "" };
    for (const k of ["clientSecret", "scope", ...(out.grant === "password" ? ["username", "password"] : [])]) if (auth[k]) out[k] = auth[k];
    if (auth.clientAuth === "body") out.clientAuth = "body";
    return out;
  }
  return undefined;
}

const base64 = (text) => btoa(String.fromCharCode(...new TextEncoder().encode(text)));

// OAuth 2.0 (hitop-core oauth2.rs): the token request for the client credentials or password
// grant: { url, headers, body, key } with values filled in by `fill`.
const form = (pairs) => pairs.filter(([, v]) => v).map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("&");
export function oauthTokenRequest(auth, fill = (s) => s) {
  const url = fill(auth.tokenUrl || "").trim();
  if (!url) throw new Error("OAuth 2.0: the token URL is empty");
  const grant = auth.grant === "password" ? "password" : "client_credentials";
  const [clientId, secret, scope] = [fill(auth.clientId || ""), fill(auth.clientSecret || ""), fill(auth.scope || "")];
  const user = grant === "password" ? fill(auth.username || "") : "";
  const pairs = [["grant_type", grant], ...(grant === "password" ? [["username", user], ["password", fill(auth.password || "")]] : []), ["scope", scope]];
  const headers = { "Content-Type": "application/x-www-form-urlencoded", Accept: "application/json" };
  if (auth.clientAuth === "body") pairs.push(["client_id", clientId], ["client_secret", secret]);
  else headers.Authorization = `Basic ${base64(`${encodeURIComponent(clientId)}:${encodeURIComponent(secret)}`)}`;
  return { url, headers, body: form(pairs), key: `${url}\n${clientId}\n${scope}\n${user}` };
}

/** The access token and its lifetime (seconds) from the token endpoint's answer. */
export function oauthParseToken(status, text) {
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {
    // not JSON
  }
  if (status < 200 || status >= 300) throw new Error(`OAuth 2.0: the token endpoint answered ${status}: ${json?.error_description || json?.error || "no reason given"}`);
  if (!json?.access_token) throw new Error("OAuth 2.0: the token endpoint's answer has no access_token");
  return { token: json.access_token, lifetime: Number(json.expires_in) || null };
}

// --- Server-Sent Events -------------------------------------------------------------

/** A request that asks for an event stream (Accept: text/event-stream). */
export const asksForEvents = (headers) => (headers || []).some((h) => h.enabled !== false && String(h.name).toLowerCase() === "accept" && /text\/event-stream/i.test(h.value || ""));

/**
 * Splits streamed text into events: `{ events, rest }`, each event `{ event, data, id }` (data
 * lines joined by newlines; comments and empty blocks left out). `rest`: an unfinished block.
 */
export function parseEvents(text) {
  const blocks = text.split(/\r?\n\r?\n/);
  const rest = blocks.pop();
  const events = [];
  for (const block of blocks) {
    const ev = { event: "message", data: [], id: null };
    for (const line of block.split(/\r?\n/)) {
      if (!line || line.startsWith(":")) continue;
      const at = line.indexOf(":");
      const field = at === -1 ? line : line.slice(0, at);
      const value = at === -1 ? "" : line.slice(at + 1).replace(/^ /, "");
      if (field === "data") ev.data.push(value);
      else if (field === "event") ev.event = value || "message";
      else if (field === "id") ev.id = value;
    }
    if (ev.data.length) events.push({ ...ev, data: ev.data.join("\n") });
  }
  return { events, rest };
}

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

// --- globals ---------------------------------------------------------------------------

// The globals: the lowest variable scope, { variables, secrets } (secret
// values in `variables`, as for environments). Kept apart from the old keys,
// so storage from before globals reads as it was.
const GLOBALS_KEY = "hitop_globals";

export const emptyGlobals = () => ({ variables: {}, secrets: [] });

export function normalizeGlobals(g) {
  const variables = g && typeof g.variables === "object" && g.variables ? Object.fromEntries(Object.entries(g.variables).map(([k, v]) => [k, String(v ?? "")])) : {};
  const secrets = secretNames(g);
  for (const n of secrets) if (!(n in variables)) variables[n] = "";
  return { variables, secrets };
}

export function loadGlobals() {
  try {
    return normalizeGlobals(JSON.parse(localStorage.getItem(GLOBALS_KEY) || "null"));
  } catch {
    return emptyGlobals();
  }
}

export function saveGlobals(globals) {
  try {
    localStorage.setItem(GLOBALS_KEY, JSON.stringify(globals));
  } catch {
    // Storage full or blocked: globals last for this page only.
  }
}

// What `hitop globals export` writes: { variables, secrets } without the
// secret values.
export function exportableGlobals(g) {
  const names = secretNames(g);
  const out = { variables: Object.fromEntries(Object.entries(g.variables || {}).filter(([k]) => !names.includes(k))) };
  if (names.length) out.secrets = names;
  return out;
}

// Globals from a file: a Postman globals (or environment) export, or HITOP's
// globals export. A Postman secret keeps its value (masked in this browser).
export function parseGlobals(json) {
  const g = JSON.parse(json);
  if (!g || typeof g !== "object" || Array.isArray(g)) throw new Error("that isn't a globals file");
  if (Array.isArray(g.values)) {
    const [env] = parseEnvironments(json);
    return { variables: env.variables, secrets: env.secrets || [] };
  }
  if (g.variables && typeof g.variables === "object") return normalizeGlobals(g);
  throw new Error("that isn't a globals file (no values or variables)");
}

// Adds `incoming` globals to `globals`: incoming values win, and a name is
// secret or plain as `incoming` says.
export function mergeGlobals(globals, incoming) {
  const variables = { ...globals.variables, ...incoming.variables };
  const secrets = [...secretNames(globals).filter((n) => !(n in incoming.variables) || secretNames(incoming).includes(n)), ...secretNames(incoming)];
  return { variables, secrets: [...new Set(secrets)].sort() };
}

export const isPostmanGlobals = (v) => v && typeof v === "object" && v._postman_variable_scope === "globals";

// Accepts HITOP's own export ([{name, variables}]) or a Postman environment
// ({name, values: [{key, value, enabled}]}), alone or in an array.
export function parseEnvironments(json) {
  const parsed = JSON.parse(json);
  const list = Array.isArray(parsed) ? parsed : [parsed];
  return list.map((env, i) => {
    if (!env || typeof env !== "object") throw new Error(`item ${i + 1} is not an object`);
    const variables = {};
    const postmanSecrets = [];
    if (Array.isArray(env.values)) {
      for (const v of env.values) {
        if (!v?.key || v.enabled === false) continue;
        variables[v.key] = String(v.value ?? "");
        // Postman's secret type: a secret here (masked, never exported).
        if (v.type === "secret") postmanSecrets.push(v.key);
      }
    } else if (env.variables && typeof env.variables === "object") {
      for (const [k, v] of Object.entries(env.variables)) variables[k] = String(v ?? "");
    } else {
      throw new Error(`item ${i + 1} has no variables`);
    }
    // HITOP's export lists secret names without their values.
    const secrets = secretNames(env);
    for (const n of secrets) variables[n] = "";
    // A Postman secret keeps its value: in the browser it lives in storage, masked.
    for (const n of postmanSecrets) if (!secrets.includes(n)) secrets.push(n);
    const out = { id: crypto.randomUUID(), name: String(env.name || `Environment ${i + 1}`), variables };
    if (secrets.length) out.secrets = secrets;
    return out;
  });
}

// --- secrets ------------------------------------------------------------------------

// An environment's `secrets` lists the names of its secret variables. In the
// browser their values stay in `variables` (localStorage); they are masked
// in the UI and in history and never written to an export. (The CLI and
// desktop keep them in the OS keychain instead.)
export const MASK = "••••";

export function secretNames(env) {
  return Array.isArray(env?.secrets) ? [...new Set(env.secrets.filter((n) => typeof n === "string" && n))] : [];
}

export function secretValues(env) {
  return secretNames(env)
    .map((n) => env.variables?.[n])
    .filter((v) => typeof v === "string" && v !== "");
}

// --- secret manager references -------------------------------------------------
// `${secret:vault:kv/app#token}`, `${secret:aws:name#key}`, `${secret:azure:vault/name}`,
// `${secret:op:vault/item/field}` (spec section 5b). Their inner text is used as a
// variable name, as hitop-core does, so a resolved one fills in like a variable.

const SECRET_REF = /\$\{(secret:[^}]+)\}/g;
// Keys whose strings are never interpolated: scripts and saved responses.
const NOT_FILLED = new Set(["preRequestScript", "postRequestScript", "examples", "scripts"]);

// The inner texts of the references in a value (strings, arrays, objects).
export function secretRefs(value, out = new Set()) {
  if (typeof value === "string") for (const m of value.matchAll(SECRET_REF)) out.add(m[1]);
  else if (Array.isArray(value)) value.forEach((v) => secretRefs(v, out));
  else if (value && typeof value === "object") for (const [k, v] of Object.entries(value)) if (!NOT_FILLED.has(k)) secretRefs(v, out);
  return out;
}

// Variables with resolved references added (by inner text) and filled in
// inside other variables' values (`token: ${secret:vault:kv/app#token}`).
export function withSecretRefs(variables, refValues) {
  if (!refValues || !Object.keys(refValues).length) return variables;
  const out = { ...variables, ...refValues };
  for (const [k, v] of Object.entries(out)) {
    if (typeof v === "string" && v.includes("${secret:")) out[k] = v.replace(SECRET_REF, (whole, r) => (r in refValues ? refValues[r] : whole));
  }
  return out;
}

// Replaces every secret value (and its URL-encoded form) with MASK, in a
// string or, recursively, in an object's strings.
export function maskSecrets(value, secrets) {
  if (!secrets?.length) return value;
  const forms = [...new Set(secrets.flatMap((v) => [v, encodeURIComponent(v)]))].sort((a, b) => b.length - a.length);
  const mask = (v) => {
    if (typeof v === "string") return forms.reduce((text, f) => text.split(f).join(MASK), v);
    if (Array.isArray(v)) return v.map(mask);
    if (v && typeof v === "object") return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, mask(x)]));
    return v;
  };
  return mask(value);
}

// Environments as exported: secret names kept, their values left out.
export function exportableEnvironments(environments) {
  return environments.map((env) => {
    const secrets = secretNames(env);
    const variables = Object.fromEntries(Object.entries(env.variables ?? {}).filter(([k]) => !secrets.includes(k)));
    return secrets.length ? { name: env.name, variables, secrets } : { name: env.name, variables };
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
    // `detail` explains a failure (pm.test passes chai's message).
    ok(condition, name = "condition is true", detail) {
      record(name, condition, detail || "was false");
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
