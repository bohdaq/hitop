// The plugin runner of the web app and the browser extensions (a module Worker, its own wasm instance:
// docs/PLUGINS_THREAT_MODEL.md). A plugin hook runs in boa (hitop-scripts-wasm) in a fresh engine with
// only its arguments, never as browser JavaScript, so it reaches nothing of this worker either (fetch,
// IndexedDB, postMessage). The engine limits set here are this instance's: people's scripts run in the
// scripts worker's own instance (scripts-worker.js). The page stops a call after 5 s (wasm-plugins.js).
import init, { runPluginHook, setPluginLimits } from "hitop-scripts-wasm";

let ready = null;

self.addEventListener("message", async (e) => {
  const { id, kind, wasm, source, hook, args } = e.data ?? {};
  if (typeof id !== "number") return;
  try {
    ready ??= init({ module_or_path: wasm }).then(() => setPluginLimits());
    await ready;
    if (kind === "ready") return self.postMessage({ id, result: true });
    if (kind !== "hook") throw new Error(`unknown call ${kind}`);
    self.postMessage({ id, result: runPluginHook(source, hook, JSON.stringify(args)) });
  } catch (err) {
    // A trap (out of memory, a stack overflow, a Rust panic) leaves the instance unusable: the page
    // replaces the worker.
    self.postMessage({ id, error: err?.message ?? String(err), trap: err instanceof WebAssembly.RuntimeError || err instanceof RangeError });
  }
});
