// Where the app runs: `web` (the browser: fetch or the HITOP proxy, and
// localStorage) or `tauri` (the desktop app: Rust through Tauri's invoke).
// The build picks one: `npm run build:tauri` sets VITE_HITOP_PLATFORM (webapp/.env.tauri, `vite --mode tauri`), and
// Vite drops the other adapter from the bundle.
//
// An adapter has:
//   name                     "web" | "tauri"
//   init()                   async; loads what `storage` then reads synchronously
//   storage                  load*/save* for collections, tabs, history, contexts, environments
//   responses                null, or large response bodies by tab id (IndexedDB, response-store.js):
//                            { keys, get(id), put(id, { body, raw }), delete(id), clear() }, async
//   send(url, options)       a fetch-like response; `headers.pairs` keeps repeated headers
//   websocket(url, { headers, protocols }, { onOpen({ protocol }), onMessage({ text } | { bytes }),
//                            onError(message), onClose({ code, reason }) })  → { send(text), close() }
//   websocketHeaders         true when websocket() sends the handshake headers (not in a browser)
//   grpc                     null in a browser (no gRPC there), or { list(server) → { source, services },
//                            describe(server, symbol) → { description, template }, call(args, { onHeaders(pairs),
//                            onMessage(json), onStatus({ code, name, message, trailers }), onError(message) }) → { cancel() } }
//   git                      null in a browser, or the desktop's Git panel calls (platform/tauri-git.js gitApi)
//   managesCookies(url)      true when the app's cookie jar applies (no browser cookie handling)
//   cookies                  null (the app keeps its jar in localStorage), or { load(), save(cookies) }
//                            for a native jar: send() then gets options.cookies and returns res.cookies
//   cookiesNote              which requests the jar applies to and where it is kept
//   pages                    optional: extra sidebar pages [{ kind, label, icon, render(store, app) }]
//   onStart(app)             optional: runs once the app is on screen
//   scripts                  null (scripts run in the page) or { pre(args), post(args) }, async
//   folders                  null, or { pick, create, read, write, restore, remember, forget, label }
//   saveFile(name, data, type)  data is text or bytes
//   secretsNote              where secret values are kept, for the environments page
export const IS_TAURI = import.meta.env.VITE_HITOP_PLATFORM === "tauri";

let current = null;

export async function initPlatform() {
  const mod = IS_TAURI ? await import("./tauri.js") : await import("./web.js");
  current = mod.createPlatform();
  await current.init();
  return current;
}

export function platform() {
  if (!current) throw new Error("initPlatform() first");
  return current;
}

// For tests: use a given adapter.
export function setPlatform(p) {
  current = p;
}
