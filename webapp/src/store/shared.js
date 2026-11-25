// What the parts of the store share: the shapes of tabs and requests, and small helpers.
import collectionService from "../../../shared/services/collectionService.js";
import pmShimText from "../../../shared/pm-shim.js?raw";
import tabService from "../../../shared/services/tabService.js";
import { MY_REQUESTS, SAMPLE_BASE_URL, UNTITLED, isMyRequests, requestAutoName, sampleRequests, tabHasContent } from "../new-request.js";
import { SAMPLE_NAME } from "../getting-started.js";
import { cleanPathVariables, cleanSettings } from "../defaults.js";
import { asksForEvents, cleanAuth, emptyAuth, secretNames } from "../features.js";
import { generateUniqueId } from "../../../shared/services/idGenerator.js";
import { isBinaryType, toBase64 } from "../binary.js";
import { loadShim } from "../pm.js";
import { platform } from "../platform/index.js";


// Postman's pm.* API for scripts (the shim the CLI and desktop run too).
// The pm.* shim, loaded on first use. The browser extensions may not evaluate code in their pages
// (HITOP Next A8): there scripts run in the sandboxed page, and the Postman import report goes
// without its unsupported-call notes.
let shimCache = null;
export function pmShim() {
  if (shimCache) return shimCache;
  try {
    shimCache = loadShim(pmShimText);
  } catch {
    shimCache = { create() { throw new Error("Scripts can't run in this page"); }, unsupported: () => [] };
  }
  return shimCache;
}

export const storage = () => platform().storage;
export const account = (workspace, env, name) => `${workspace}/${env}/${name}`;
export const workspaceOf = (collection) => collection.workspace || String(collection.id);
// Secret globals go by this environment name (env var HITOP_SECRET_GLOBAL_<NAME>,
// keychain account <workspace>/global/<name>), as in the CLI.
export const GLOBALS_ENV = "global";
// Ids the Rust model reads (whole numbers); old web data has strings.
export const numericId = (id) => (Number.isSafeInteger(id) && id >= 0 ? id : generateUniqueId());
export const errorText = (e) => (e && typeof e === "object" && "message" in e ? e.message : String(e));
// An environment as written to a file or export: secret values left out.
export const withoutSecretValues = (e) => ({ ...e, variables: Object.fromEntries(Object.entries(e.variables || {}).filter(([k]) => !secretNames(e).includes(k))) });

export const COOKIES_KEY = "hitop_cookies";
export const COOKIE_JAR_KEY = "hitop_cookie_jar";
// The org picked in the top bar (store.currentOrg), when signed in to HITOP Cloud.
export const ORG_KEY = "hitop_org";

export function loadCookies() {
  try {
    return { cookies: JSON.parse(localStorage.getItem(COOKIES_KEY) || "[]"), enabled: localStorage.getItem(COOKIE_JAR_KEY) !== "off" };
  } catch {
    return { cookies: [], enabled: true };
  }
}

// A saved request sent as GraphQL (hitop-core's rule): bodyType graphql on
// a raw body. Its query is `body`, the rest is in `graphql`.
export const isGraphql = (r) => r?.bodyType === "graphql" && (!r.bodyMode || r.bodyMode === "raw");

// A saved request that opens a WebSocket connection (hitop-core's `kind`).
export const isWebsocket = (r) => r?.kind === "websocket";
// A saved request that makes a gRPC call (hitop-core's `kind`).
export const isGrpc = (r) => r?.kind === "grpc";
// What request lists show in the method's place.
// SSE: a GET that asks for an event stream (Accept: text/event-stream; UX2 method menu).
export const methodLabel = (r) => (isWebsocket(r) ? "WS" : isGrpc(r) ? "GRPC" : r.method === "GET" && asksForEvents(r.headers) ? "SSE" : r.method);

// Request fields the web app adds, as a tab carries them: a GraphQL request
// shows the body mode "graphql", with its variables and operation name; a
// WebSocket request its subprotocols and saved messages; a gRPC request its
// service, method, message and schema source.
export const EXTRA_FIELDS = (r) => ({
  kind: isWebsocket(r) ? "websocket" : isGrpc(r) ? "grpc" : undefined,
  subprotocols: r.websocket?.subprotocols ?? [],
  wsMessages: r.websocket?.messages ?? [],
  grpcService: r.grpc?.service ?? "",
  grpcMethod: r.grpc?.method ?? "",
  grpcMessage: r.grpc?.message ?? "",
  grpcProtos: r.grpc?.protos ?? [],
  grpcImportPaths: r.grpc?.importPaths ?? [],
  grpcPlaintext: !!r.grpc?.plaintext,
  bodyMode: isGraphql(r) ? "graphql" : r.bodyMode || "raw",
  formFields: r.formFields || [],
  disabledParams: r.disabledParams || [],
  graphqlVariables: r.graphql?.variables ?? "",
  graphqlOperationName: r.graphql?.operationName ?? "",
  settings: r.settings ? { ...r.settings } : undefined,
  pathVariables: Array.isArray(r.pathVariables) ? r.pathVariables.map((p) => ({ ...p })) : [],
});

// The same fields of a tab, as saved with the request: the "graphql" mode is
// a raw body with bodyType graphql. Variables and operation name are kept
// after a switch to another body type (as the form rows are), not sent.
export const SAVED_FIELDS = (t) => {
  const graphql = Object.fromEntries(Object.entries({ variables: t.graphqlVariables, operationName: t.graphqlOperationName }).filter(([, v]) => v));
  return {
    bodyMode: t.bodyMode === "graphql" ? "raw" : t.bodyMode || "raw",
    formFields: t.formFields || [],
    disabledParams: t.disabledParams || [],
    graphql: Object.keys(graphql).length ? graphql : undefined,
    ...(t.bodyMode === "graphql" ? { bodyType: "graphql" } : {}),
    kind: t.kind === "websocket" || t.kind === "grpc" ? t.kind : undefined,
    websocket: websocketFields(t),
    grpc: grpcFields(t),
    settings: cleanSettings(t.settings),
    pathVariables: cleanPathVariables(t),
  };
};

// A tab's gRPC fields as saved (`grpc`), or undefined when it has none.
// Kept when the request switches to HTTP, as the WebSocket ones are.
export function grpcFields(t) {
  const g = {
    service: t.grpcService || undefined,
    method: t.grpcMethod || undefined,
    message: t.grpcMessage || undefined,
    protos: (t.grpcProtos || []).filter(Boolean),
    importPaths: (t.grpcImportPaths || []).filter(Boolean),
    plaintext: t.grpcPlaintext || undefined,
  };
  if (!g.protos.length) delete g.protos;
  if (!g.importPaths.length) delete g.importPaths;
  for (const k of Object.keys(g)) if (g[k] === undefined) delete g[k];
  return Object.keys(g).length ? g : undefined;
}

// The log of a gRPC tab keeps this many entries.
export const GRPC_LOG_MAX = 500;

// A tab's subprotocols and saved messages as saved (`websocket`), or
// undefined when it has neither. Kept when the request switches to HTTP.
export function websocketFields(t) {
  const subprotocols = (t.subprotocols || []).filter(Boolean);
  const messages = t.wsMessages || [];
  if (!subprotocols.length && !messages.length) return undefined;
  return { ...(subprotocols.length ? { subprotocols } : {}), ...(messages.length ? { messages } : {}) };
}

// ws:// and wss:// as given; http(s):// read as ws(s)://, as hitop-core does.
export function wsUrl(url) {
  const m = /^(wss?|https?):\/\//i.exec(url);
  if (!m) throw new Error(`'${url}' is not a WebSocket URL: use ws:// or wss://`);
  const scheme = { ws: "ws", wss: "wss", http: "ws", https: "wss" }[m[1].toLowerCase()];
  return `${scheme}://${url.slice(m[0].length)}`;
}

// The log of a WebSocket tab keeps this many entries.
export const WS_LOG_MAX = 500;

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
export const BODY_METHODS = ["POST", "PUT", "PATCH"];

export const emptyRequest = () =>
  collectionService.createRequest("New request", {
    url: "",
    method: "GET",
    headers: [{ name: "", value: "" }],
    body: "",
    preRequestScript: "",
    postRequestScript: "",
  });

export function tabForRequest(request, collectionId) {
  return {
    ...tabService.createNewTab(),
    title: request.name,
    url: request.url,
    method: request.method,
    headers: request.headers?.length ? request.headers : [{ name: "", value: "" }],
    requestBody: request.body || "",
    preRequestScript: request.preRequestScript || "",
    postRequestScript: request.postRequestScript || "",
    auth: request.auth ? { ...emptyAuth(), ...request.auth } : emptyAuth(),
    ...EXTRA_FIELDS(request),
    loadedRequestId: request.id,
    loadedCollectionId: collectionId,
  };
}

// A new request as + (and Mod+N, the palette, the sidebar) adds it to a collection.
export const untitledRequest = (extra = {}) => ({ ...emptyRequest(), name: UNTITLED, ...extra });

// Before FV1 a tab could be a draft outside every collection. One restored with something in it
// is filed into My requests (made when there is none); an empty one is dropped. Returns the
// collections and tabs (null tabs: nothing left).
export function fileDrafts(collections, tabs) {
  let cols = collections;
  let home = null;
  const out = [];
  for (const t of tabs) {
    // A saved request's tab stays, also when its collection isn't here yet (a desktop folder
    // collection is read after the store starts).
    if (t.loadedRequestId != null) {
      out.push(t);
      continue;
    }
    if (!tabHasContent(t)) continue;
    home ??= cols.find(isMyRequests);
    if (!home) {
      home = collectionService.createCollection(MY_REQUESTS);
      cols = [...cols, home];
    }
    // A title the old draft made from its URL ("GET host/path") goes on following the URL.
    const auto = !t.title || /^(new request|untitled)$/i.test(t.title.trim()) || t.title.startsWith(`${t.kind === "websocket" ? "WS" : t.kind === "grpc" ? "GRPC" : t.method} `);
    const name = auto ? requestAutoName(t) : t.title;
    const request = clean({ ...collectionService.createRequest(name, { url: t.url, method: t.method, headers: t.headers, body: t.requestBody, preRequestScript: t.preRequestScript, postRequestScript: t.postRequestScript, auth: cleanAuth(t.auth) }), ...SAVED_FIELDS(t) });
    if (!request.auth) delete request.auth;
    const at = cols.indexOf(home);
    home = { ...home, requests: [...(home.requests || []), request] };
    cols = cols.map((c, i) => (i === at ? home : c));
    const { folder: _folder, ...rest } = t;
    out.push({ ...rest, title: name, loadedRequestId: request.id, loadedCollectionId: home.id, ...(auto ? { autoName: true } : {}) });
  }
  return { collections: cols, tabs: out.length ? out : null };
}

export function initialState() {
  const collections = storage().loadCollections()?.map(({ sharedIn: _, ...c }) => c);
  const tabs = storage().loadTabs();
  const current = storage().loadCurrentTab();
  // FV1 first visit (nothing stored at all): the sample with "List posts" open, and an empty
  // My requests for one's own.
  if (!collections?.length && !tabs?.length) {
    const sample = sampleCollection();
    const mine = collectionService.createCollection(MY_REQUESTS);
    return { collections: [sample, mine], tabs: [tabForRequest(sample.requests[0], sample.id)], currentTab: 0, firstVisit: true };
  }
  let cols = collections?.length ? collections : [];
  const open = tabs?.length ? tabs : null;
  if (open) {
    const filed = fileDrafts(cols, open);
    cols = filed.collections;
    // A request in flight when the page closed never finishes.
    if (filed.tabs) {
      const same = filed.tabs.length === open.length;
      return {
        collections: cols,
        tabs: filed.tabs.map((t) => ({ ...t, loading: false })),
        currentTab: same && current !== null && current >= 0 && current < filed.tabs.length ? current : 0,
      };
    }
  }
  // No tab to restore: the first collection's first request, else a new one in My requests.
  const first = cols.find((c) => c.requests?.length);
  if (first) return { collections: cols, tabs: [tabForRequest(first.requests[0], first.id)], currentTab: 0 };
  let home = cols.find(isMyRequests);
  const request = untitledRequest();
  if (home) cols = cols.map((c) => (c === home ? (home = { ...c, requests: [...(c.requests || []), request] }) : c));
  else cols = [...cols, (home = { ...collectionService.createCollection(MY_REQUESTS), requests: [request] })];
  return { collections: cols, tabs: [{ ...tabForRequest(request, home.id), autoName: true }], currentTab: 0 };
}

// The sample collection (store.addSampleCollection, and the first visit), with ids.
export function sampleCollection() {
  return {
    ...collectionService.createCollection(SAMPLE_NAME),
    variables: { baseUrl: SAMPLE_BASE_URL },
    requests: sampleRequests().map(({ name, ...r }) => collectionService.createRequest(name, r)),
  };
}

// Turn a fetch-like response into what the response viewer shows.
export async function readResponse(res) {
  const contentType = res.headers.get("content-type") || "";
  // UX2 (6): images, PDFs and other binary bodies keep their bytes (base64 on the tab).
  if (isBinaryType(contentType) && typeof res.arrayBuffer === "function") {
    const bytes = new Uint8Array(await res.arrayBuffer());
    return { data: toBase64(bytes), type: "binary", size: bytes.length };
  }
  const text = await res.text();
  if (contentType.includes("json")) {
    try {
      return { data: JSON.stringify(JSON.parse(text), null, 2), type: "json", raw: text };
    } catch (e) {
      // Shown as text, with what the parser said (not silently).
      return { data: text, type: "text", jsonError: text.trim() ? e.message : null };
    }
  }
  if (contentType.includes("html")) return { data: text, type: "html" };
  if (contentType.includes("xml")) return { data: text, type: "xml" };
  return { data: text, type: "text" };
}

// A tab as saved without IndexedDB (the 1.1 behaviour): a response body over
// KEEP_BODY is cut to its first KEEP_HEAD characters (`responseCut`), so one
// large response can't fill the browser's storage (about 5 MB for
// everything) and stop every later save. The open tab keeps the whole body
// until the page is reloaded.
export const KEEP_BODY = 256 * 1024;
export const KEEP_HEAD = 64 * 1024;
export function storedTab(t) {
  if (typeof t.response !== "string" || t.response.length <= KEEP_BODY) return t;
  return { ...t, response: t.response.slice(0, KEEP_HEAD), responseCut: true, responseStored: undefined };
}

// With IndexedDB (platform().responses): a body over INLINE_BODY goes there
// whole, and the saved tab keeps its first STORED_HEAD characters with
// `responseCut` and `responseStored` (so the React app and 1.1 still read a
// body, with the note). After a reload the whole body comes back.
export const INLINE_BODY = 64 * 1024;
export const STORED_HEAD = 16 * 1024;
export function storedTabInDb(t) {
  if (typeof t.response !== "string" || t.response.length <= INLINE_BODY || t.id == null) return t;
  return { ...t, response: t.response.slice(0, STORED_HEAD), responseCut: true, responseStored: true };
}

// A tab's response fields when it has none (Settings: clear stored responses).
export const NO_RESPONSE = { consoleLogs: null, redirects: null, response: null, responseSize: null, responseHeaders: null, statusCode: null, responseType: "", tests: null, durationMs: null, timings: null, httpVersion: null, responseScheme: null, truncated: false, responseCut: false, responseStored: undefined, jsonError: null };

// Drops keys set to undefined (a cleared bodyType), so they aren't saved.
export const clean = (r) => Object.fromEntries(Object.entries(r).filter(([, v]) => v !== undefined));

export const headersObject = (headers) => {
  const out = {};
  for (const h of headers || []) if (h.name && h.value) out[h.name] = h.value;
  return out;
};

// A tab's title after an edit: a new request's (and an old draft's) follows its URL.
export function autoTitle(tab) {
  return tab.autoName || !tab.loadedRequestId ? requestAutoName(tab) : tab.title;
}

// What decides whether a tab differs from its saved request: the fields
// saveRequest writes, normalized (empty header rows and unused auth fields
// don't count).
export function savedShape(tab) {
  return JSON.stringify(
    clean({
      url: tab.url || "",
      method: tab.method || "GET",
      headers: (tab.headers || []).filter((h) => h.name || h.value).map((h) => [h.name || "", h.value || "", h.enabled !== false]),
      body: tab.requestBody || "",
      preRequestScript: tab.preRequestScript || "",
      postRequestScript: tab.postRequestScript || "",
      auth: cleanAuth(tab.auth),
      ...SAVED_FIELDS(tab),
    }),
  );
}
