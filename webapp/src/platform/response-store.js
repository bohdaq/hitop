// Large response bodies, kept across reloads in IndexedDB (both adapters:
// the desktop's open tabs live in the window's storage too). localStorage
// holds about 5 MB for everything, so `hitop_tabs` keeps only the first part
// of a large body (with `responseCut` and `responseStored`) and the whole
// body is here, by tab id: { body, raw }. Its own database, so the web
// adapter's "hitop" one (folder handles) keeps its version.
//
// openResponseStore() → null when IndexedDB is missing or refuses to open
// (some private windows, blocked site data): the store then keeps the 1.1
// behaviour (a body over 256 KB saved as its first 64 KB).
const DB = "hitop-responses";
const STORE = "bodies";
// A browser that never answers the open request doesn't hold up the app.
const OPEN_TIMEOUT_MS = 3000;

function open() {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("IndexedDB didn't open")), OPEN_TIMEOUT_MS);
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => (clearTimeout(timer), resolve(req.result));
    req.onerror = () => (clearTimeout(timer), reject(req.error));
    req.onblocked = () => (clearTimeout(timer), reject(new Error("IndexedDB is blocked")));
  });
}

export async function openResponseStore() {
  let db;
  try {
    if (typeof indexedDB === "undefined" || !indexedDB) return null;
    db = await open();
  } catch {
    return null;
  }
  const run = (mode, fn) =>
    new Promise((resolve, reject) => {
      let tx;
      try {
        tx = db.transaction(STORE, mode);
      } catch (e) {
        return reject(e);
      }
      const req = fn(tx.objectStore(STORE));
      tx.oncomplete = () => resolve(req?.result);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error ?? new Error("IndexedDB write aborted"));
    });
  let keys;
  try {
    keys = await run("readonly", (s) => s.getAllKeys());
  } catch {
    return null;
  }
  return {
    // The keys there were when the app started (tab ids).
    keys,
    get: (id) => run("readonly", (s) => s.get(id)),
    put: (id, value) => run("readwrite", (s) => s.put(value, id)),
    delete: (id) => run("readwrite", (s) => s.delete(id)),
    clear: () => run("readwrite", (s) => s.clear()),
  };
}

// What the site's storage holds, for Settings: { usage, quota } in bytes from
// the browser (null where it can't say), and localStorage's share by key
// (characters, which most browsers count twice: UTF-16).
export async function storageUsage() {
  let estimate = null;
  try {
    estimate = (await navigator.storage?.estimate?.()) ?? null;
  } catch {
    // not available here
  }
  const keys = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      keys[k] = (k.length + (localStorage.getItem(k) || "").length) * 2;
    }
  } catch {
    // storage blocked
  }
  return { usage: estimate?.usage ?? null, quota: estimate?.quota ?? null, local: keys };
}
