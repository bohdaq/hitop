// Scripts in the web app and the Firefox extension. A script may come from someone else (a shared
// or imported collection), so it never runs in the page, which holds the session and the device key;
// Firefox has no sandboxed extension pages either (sandbox-scripts.js is Chrome's). Scripts run in boa compiled to WebAssembly
// (hitop-scripts-wasm) in a worker (src/scripts-worker.js) instead, in the desktop's
// run_pre_script / run_post_script shapes, mapped here the way tauri.js maps them. A script that
// runs longer than LIMIT_MS is stopped (the worker is replaced) and reported as an error.
import wasmUrl from "hitop-scripts-wasm/wasm?url";

export const LIMIT_MS = 30_000;

let worker = null;
let next = 1;
const pending = new Map();

function spawn() {
  worker = new Worker(new URL("../scripts-worker.js", import.meta.url), { type: "module" });
  worker.addEventListener("message", (e) => {
    const m = e.data;
    const call = pending.get(m?.id);
    if (!call) return;
    pending.delete(m.id);
    clearTimeout(call.timer);
    if (m.error) call.reject(new Error(m.error));
    else call.resolve(m.result);
  });
  worker.addEventListener("error", (e) => {
    for (const [id, call] of pending) {
      clearTimeout(call.timer);
      call.reject(new Error(e.message || "The script runner didn't start"));
      pending.delete(id);
    }
    worker = null;
  });
}

function call(kind, args) {
  if (!worker) spawn();
  const id = next++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      // A script that never ends: stop it with the worker (and anything queued behind it).
      worker?.terminate();
      worker = null;
      for (const [other, c] of pending) {
        clearTimeout(c.timer);
        c.reject(new Error(other === id ? `The script ran for more than ${LIMIT_MS / 1000} s and was stopped` : "Stopped with a script that ran too long"));
      }
      pending.clear();
    }, LIMIT_MS);
    pending.set(id, { resolve, reject, timer });
    worker.postMessage({ id, kind, args, wasm: new URL(wasmUrl, location.href).href });
  });
}

const pairs = (headers) => (headers ?? []).filter((h) => h.name).map((h) => [h.name, h.value]);

export const wasmScripts = {
  async pre({ script, url, method, headers, body, variables, context, globals }) {
    const r = await call("pre", { script, url, method, headers: pairs(headers), body: body || "", variables, context, globals: globals || {} });
    return { url: r.url, headers: r.headers.map(([name, value]) => ({ name, value })), body: r.body, contextUpdates: r.context_updates || {}, variables: r.variables || {}, globals: r.globals || {}, logs: r.logs || [] };
  },
  async post({ script, text, headers, status, durationMs, sent, variables, context, globals }) {
    // pm.request: the request as sent (a multipart body with files is bytes: left out).
    const request = sent && { url: sent.url ?? "", method: sent.method ?? "", headers: pairs(sent.headers), body: typeof sent.body === "string" ? sent.body : "" };
    const r = await call("post", { script, response_body: text, response_headers: headers, status_code: status, duration_ms: Math.round(durationMs || 0), variables, context, request, globals: globals || {} });
    return { tests: r.tests || [], contextUpdates: r.context_updates || {}, variables: r.variables || {}, globals: r.globals || {}, logs: r.logs || [], error: r.error ?? null };
  },
};
