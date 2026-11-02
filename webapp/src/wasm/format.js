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
    // OpenAPI 3 text (YAML or JSON) + { flat, source, inlineAuth } → { collection, report: { mapped, skipped, warnings } }.
    importOpenapi: (text, options) => call(m.importOpenapi, String(text ?? ""), options ?? {}),
    // An imported collection + newer spec text → { collection, diff: { added, changed, removed, unchanged, collection }, report }.
    syncOpenapi: (collection, text, options, prune) => JSON.parse(m.syncOpenapi(JSON.stringify(collection), String(text ?? ""), JSON.stringify(options ?? {}), Boolean(prune))),
    isOpenapi: (text) => m.isOpenapi(String(text ?? "")),
    // A GraphQL request's JSON body (variables filled in), as hitop-core
    // builds it: text, not parsed. Throws when the variables aren't an object.
    graphqlBody: (query, variables, operationName) => m.graphqlBody(String(query ?? ""), String(variables ?? ""), String(operationName ?? "")),
    // What every hitop surface does when sending (hitop-core `sending`,
    // spec/vectors/send.json): the Content-Type a raw body gets when the
    // request sets none (null for an empty body), and the URL with a scheme
    // (throws "… isn't a URL" for anything else).
    rawContentType: (body) => m.rawContentType(typeof body === "string" ? body : "") ?? null,
    withScheme: (url) => m.withScheme(String(url ?? "")),
    // { request, variables, secrets } + "curl" | "fetch" | "python" | "go" | "java" | "rust" → { code, placeholders }.
    snippet: (input, lang) => call(m.snippet, input, lang),
    // [{ id, label }] in menu order.
    snippetLanguages: () => JSON.parse(m.snippetLanguages()),
    // hitop sync (src/sync.js): hitop-core's sync plan and hitop-sync-crypto, as the CLI runs
    // them. Keys and envelopes are base64url strings.
    sync: {
      plan: (local, state, remote, me) => call(m.syncPlan, local, state, remote, String(me)),
      pushed: (state, push) => call(m.syncPushed, state, push),
      resolve: (state, path, keepMine) => JSON.parse(m.syncResolve(JSON.stringify(state), String(path), Boolean(keepMine))),
      isSyncedPath: (path) => m.syncIsSyncedPath(String(path)),
      keygen: () => JSON.parse(m.syncKeygen()),
      publicKey: (secret) => m.syncPublicKey(secret),
      fingerprint: (publicKey) => m.syncFingerprint(publicKey),
      newKey: () => m.syncNewKey(),
      randomId: () => m.syncRandomId(),
      // Authenticated (format 2) wrap made by this device (secret, its key id) for one device.
      sealWrap: (secret, senderKeyId, recipientKeyId, recipientPublicKey, workspace, epoch, head, key) =>
        m.syncSealWrap(secret, String(senderKeyId), String(recipientKeyId), recipientPublicKey, workspace, epoch, head, key),
      // { legacy: true } or { legacy: false, sender, head } (not verified).
      wrapHeader: (wrapped) => JSON.parse(m.syncWrapHeader(wrapped)),
      // hitop-sync-crypto accept_wraps: which keys this device may use. wraps [{ epoch, wrapped }],
      // pinned/listed { keyId: publicKey }, memory = WorkspaceMemory ({} when none) →
      // { ok: true, keys: { epoch: key }, current, notices, newPins, memory, legacy } | { ok: false, error: { kind, message, ... } }.
      acceptWraps: (secret, keyId, workspace, current, wraps, pinned, listed, memory) =>
        JSON.parse(m.syncAcceptWraps(secret, String(keyId), workspace, current, JSON.stringify(wraps), JSON.stringify(pinned ?? {}), JSON.stringify(listed ?? {}), JSON.stringify(memory ?? {}))),
      normalizeFingerprint: (fp) => m.syncNormalizeFingerprint(String(fp ?? "")),
      // A format 1 (unauthenticated, hitop 1.1) wrap: never made by the app; tests play a malicious server with it.
      wrapLegacy: (publicKey, workspace, epoch, key) => m.syncWrapLegacy(publicKey, workspace, epoch, key),
      blobEpoch: (envelope) => m.syncBlobEpoch(envelope),
      seal: (key, workspace, blob, version, epoch, path, text) => m.syncSeal(key, workspace, blob, BigInt(version), epoch, path, text),
      open: (key, workspace, blob, version, envelope) => JSON.parse(m.syncOpen(key, workspace, blob, BigInt(version), envelope)),
      // vectors/v1.json through this build; returns how many cases passed.
      checkVectors: (json) => m.syncCheckVectors(typeof json === "string" ? json : JSON.stringify(json)),
      // vectors/v2.json (authenticated wraps, key commitments).
      checkVectorsV2: (json) => m.syncCheckVectorsV2(typeof json === "string" ? json : JSON.stringify(json)),
    },
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
