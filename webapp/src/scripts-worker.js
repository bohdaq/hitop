// The Firefox extension's script runner (a module Worker; HITOP Next). Firefox has no sandboxed
// extension pages and its extension CSP forbids eval, so scripts run in hitop-scripts-wasm:
// boa, hitop-core's JavaScript interpreter, compiled to WebAssembly ('wasm-unsafe-eval' is
// allowed). The same engine and pm.* shim as the desktop app and the CLI. A script never runs as
// browser JavaScript: it sees only the request and variables it's given, no DOM, extension APIs
// or storage. Being a worker, the page can stop a script that never ends (wasm-scripts.js).
import init, { runPostScript, runPreScript } from "hitop-scripts-wasm";

let ready = null;

self.addEventListener("message", async (e) => {
  const { id, kind, args, wasm } = e.data ?? {};
  if (typeof id !== "number") return;
  let reply;
  try {
    ready ??= init({ module_or_path: wasm });
    await ready;
    const run = kind === "pre" ? runPreScript : kind === "post" ? runPostScript : null;
    if (!run) throw new Error(`unknown call ${kind}`);
    reply = { id, result: JSON.parse(run(JSON.stringify(args))) };
  } catch (err) {
    reply = { id, error: err?.message ?? String(err) };
  }
  self.postMessage(reply);
});
