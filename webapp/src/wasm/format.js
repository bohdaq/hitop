// hitop-core's collection folder format in the browser (hitop-wasm, built
// by scripts/build-wasm.mjs): YAML is read and written by the same Rust
// code as the CLI, so a folder means the same thing everywhere.
let loading = null;

function api(m) {
  const call = (fn, ...args) => JSON.parse(fn(...args.map((a) => (typeof a === "string" ? a : JSON.stringify(a)))));
  return {
    // { "hitop.yaml": text, "requests/a.yaml": text, ... } → { collection, environments }
    readFiles: (files, name) => call(m.readFiles, files, String(name ?? "")),
    // { collection, environments } → files. Secret values are left out.
    toFiles: (folder) => call(m.toFiles, folder),
    // existing paths + wanted files → { write: { path: text }, remove: [path] }
    planSync: (existing, files) => call(m.planSync, existing, files),
    // JSON through hitop-core's model: what the desktop's Rust storage keeps.
    normalize: (kind, value) => call(m.normalize, kind, value),
  };
}

// The format module, loaded once on first use.
export function loadFormat() {
  loading ??= import("./pkg/hitop_wasm.js").then(async (m) => {
    await m.default();
    return api(m);
  });
  return loading;
}

// For Node (unit tests, the Tauri mock): the module from the .wasm bytes.
export async function formatFromBytes(bytes) {
  const m = await import("./pkg/hitop_wasm.js");
  m.initSync({ module: bytes });
  return api(m);
}
