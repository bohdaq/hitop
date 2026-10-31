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
