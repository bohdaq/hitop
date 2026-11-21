// The scripts before and after a request, run in a page (the web build) or in the browser
// extension's sandboxed page (sandbox.html, HITOP Next A8: extension pages may not eval).
// Plain data in, plain data out, in the shape the desktop's Rust scripts answer (tauri.js):
// pre → {url, headers, body, contextUpdates, variables, globals},
// post → {tests, contextUpdates, variables, globals, error?}.
import { makeAssert } from "./features.js";
import { runScript } from "./pm.js";

// What `console.*` printed, for the response pane's Console tab: [{level, text}] (at most 500).
function capture() {
  const logs = [];
  const show = (v) => {
    if (typeof v === "string") return v;
    if (v instanceof Error) return `${v.name}: ${v.message}`;
    try {
      return JSON.stringify(v, null, 2) ?? String(v);
    } catch {
      return String(v);
    }
  };
  const console = {};
  for (const level of ["log", "info", "warn", "error", "debug"]) console[level] = (...args) => logs.length < 500 && logs.push({ level, text: args.map(show).join(" ") });
  return { console, logs };
}

// A thrown error's message with where it happened in the script ("line 3, column 5"). The script
// runs inside `new Function(..., "with(ctx) { <script> }")`: two lines of function header come
// first, and the first line starts after "with(ctx) { ".
export function scriptError(e) {
  const message = e?.message ?? String(e);
  const m = /(?:<anonymous>|> Function):(\d+):(\d+)/.exec(e?.stack ?? "");
  if (!m) return message;
  const line = Number(m[1]) - 2;
  if (line < 1) return message;
  const col = line === 1 ? Number(m[2]) - 12 : Number(m[2]);
  return `${message} (line ${line}, column ${Math.max(1, col)})`;
}

export function runPre(pmShim, { script, url, method, headers, body, variables, context, globals }) {
  const updates = {};
  const ctx = {
    url,
    method,
    headers: headers.map((h) => ({ ...h })),
    body,
    variables: { ...variables },
    context: { ...context },
    setContext: (k, v) => (updates[k] = v),
    getContext: (k) => (updates[k] !== undefined ? updates[k] : context[k]),
    getVariable: (k) => variables[k],
    setHeader: (name, value) => {
      const h = ctx.headers.find((x) => x.name === name);
      if (h) h.value = value;
      else ctx.headers.push({ name, value });
    },
    setUrl: (u) => (ctx.url = u),
    setBody: (b) => (ctx.body = b),
  };
  const vars = {};
  const setGlobals = {};
  const { console, logs } = capture();
  ctx.console = console;
  try {
    runScript(pmShim, script, ctx, {
      vars: { ...variables },
      setVar: (k, v) => (vars[k] = v),
      globals,
      setGlobal: (k, v) => (setGlobals[k] = v),
      assert: { ok() {} },
      request: { get url() { return ctx.url; }, method, get headers() { return ctx.headers.map((h) => [h.name, h.value]); }, get body() { return ctx.body; } },
      setUrl: ctx.setUrl,
      setHeader: ctx.setHeader,
      removeHeader: (n) => (ctx.headers = ctx.headers.filter((h) => String(h.name).toLowerCase() !== String(n).toLowerCase())),
      setBody: ctx.setBody,
      response: null,
    });
  } catch (e) {
    const error = new Error(scriptError(e));
    error.logs = logs;
    throw error;
  }
  return { url: ctx.url, headers: ctx.headers, body: ctx.body, contextUpdates: updates, variables: vars, globals: setGlobals, logs };
}

/** A script that throws still returns what it set before; `error` says why it stopped. */
export function runPost(pmShim, { script, text, headers, status, durationMs, sent, variables, context, globals }) {
  let parsed = text;
  try {
    parsed = JSON.parse(text);
  } catch {
    // not JSON
  }
  const updates = {};
  const { assert, results } = makeAssert({ statusCode: status, text, headers, durationMs });
  const ctx = {
    assert,
    response: parsed,
    responseText: text,
    responseHeaders: headers,
    statusCode: status,
    variables: { ...variables },
    context: { ...context },
    setContext: (k, v) => (updates[k] = v),
    getContext: (k) => (updates[k] !== undefined ? updates[k] : context[k]),
    getVariable: (k) => variables[k],
    getResponseValue: (path) => path.split(".").reduce((v, k) => (v && typeof v === "object" ? v[k] : undefined), parsed),
    getResponseHeader: (name) => headers[name.toLowerCase()],
  };
  const vars = {};
  const setGlobals = {};
  let error = null;
  const { console, logs } = capture();
  ctx.console = console;
  try {
    runScript(pmShim, script, ctx, {
      vars: { ...variables },
      setVar: (k, v) => (vars[k] = v),
      globals,
      setGlobal: (k, v) => (setGlobals[k] = v),
      assert,
      request: { url: sent?.url ?? "", method: sent?.method ?? "", headers: (sent?.headers ?? []).filter((h) => h.name).map((h) => [h.name, h.value]), body: typeof sent?.body === "string" ? sent.body : "" },
      response: { code: status, text, headers, time: durationMs },
    });
  } catch (e) {
    error = scriptError(e);
  }
  return { tests: results, contextUpdates: updates, variables: vars, globals: setGlobals, error, logs };
}
