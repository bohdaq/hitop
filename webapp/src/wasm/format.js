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
    // Postman collection JSON + { script: [[line, call, text]] } → { collection, report }.
    importPostman: (json, scans) => call(m.importPostman, json, scans),
    // A GraphQL request's JSON body (variables filled in), as hitop-core
    // builds it: text, not parsed. Throws when the variables aren't an object.
    graphqlBody: (query, variables, operationName) => m.graphqlBody(String(query ?? ""), String(variables ?? ""), String(operationName ?? "")),
    // { request, variables, secrets } + "curl" | "fetch" | "python" | "go" | "java" | "rust" → { code, placeholders }.
    snippet: (input, lang) => call(m.snippet, input, lang),
    // [{ id, label }] in menu order.
    snippetLanguages: () => JSON.parse(m.snippetLanguages()),
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
