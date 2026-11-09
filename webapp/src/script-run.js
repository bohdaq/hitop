// The scripts before and after a request, run in a page (the web build) or in the browser
// extension's sandboxed page (extension/sandbox, HITOP Next A8: extension pages may not eval).
// Plain data in, plain data out, in the shape the desktop's Rust scripts answer (tauri.js):
// pre → {url, headers, body, contextUpdates, variables, globals},
// post → {tests, contextUpdates, variables, globals, error?}.
import { makeAssert } from "./features.js";
import { runScript } from "./pm.js";

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
  return { url: ctx.url, headers: ctx.headers, body: ctx.body, contextUpdates: updates, variables: vars, globals: setGlobals };
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
    error = e?.message ?? String(e);
  }
  return { tests: results, contextUpdates: updates, variables: vars, globals: setGlobals, error };
}
