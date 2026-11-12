// FV1 (HITOP_NEXT_UI.md): new requests land in a collection, and a first visit starts on the
// sample. What decides that and is pure (no storage, no store): the automatic name a new request
// follows its URL with, what counts as an untouched request or collection, the sample's
// requests. Unit-checked in tests/unit-ux.mjs; store.js does the rest.
import { SAMPLE_NAME, SAMPLE_REQUESTS } from "./getting-started.js";

export const UNTITLED = "Untitled";

// FV1: where requests go when no other collection is meant: "My requests" (made for new people),
// or the "Default" collection older versions made (it keeps its name).
export const MY_REQUESTS = "My requests";
export const isMyRequests = (c) => c?.name === MY_REQUESTS || c?.name === "Default";

// The part of a URL a request's automatic name shows: its path ("/users"), or the host when there
// is no path; query and fragment left out. "${baseUrl}/users" → "/users".
function urlWhere(text) {
  let url = String(text ?? "").trim().replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").replace(/[?#].*$/s, "");
  if (!url) return "";
  // A leading variable (${baseUrl}, {{baseUrl}}) stands for the host.
  const lead = /^(\$\{[^}]*\}|\{\{[^}]*\}\})/.exec(url);
  const host = lead ? lead[1] : url.split("/")[0];
  const path = url.slice(host.length);
  url = path && path !== "/" ? path : host;
  return url.length > 48 ? `${url.slice(0, 47)}…` : url;
}

/**
 * A new request's automatic name, from what it sends (a tab: method, url, kind, grpcMethod):
 * "GET /users", "GET api.example.com" (host only), "WS /socket", "GRPC SayHello"; "Untitled" with
 * no URL. A new request follows its URL this way until it is renamed (tab `autoName`).
 */
export function requestAutoName(tab) {
  if (tab.kind === "grpc") {
    const what = tab.grpcMethod || urlWhere(tab.url);
    return what ? `GRPC ${what}` : UNTITLED;
  }
  const where = urlWhere(tab.url);
  if (!where) return UNTITLED;
  return `${tab.kind === "websocket" ? "WS" : tab.method || "GET"} ${where}`;
}

/** A saved request with nothing in it (no URL, body or scripts): a new one nobody has filled in. */
export function isBlankRequest(r) {
  return !String(r?.url ?? "").trim() && !String(r?.body ?? "").trim() && !String(r?.preRequestScript ?? "").trim() && !String(r?.postRequestScript ?? "").trim() && !r?.kind;
}

/** A tab with something in it (an old draft worth keeping). */
export function tabHasContent(t) {
  return Boolean(t?.url?.trim() || t?.requestBody?.trim() || t?.preRequestScript?.trim() || t?.postRequestScript?.trim());
}

/**
 * The sample collection's requests (store.js sampleCollection adds ids): JSONPlaceholder (public,
 * through the proxy on the web), with a variable, a script that passes a value on, and tests.
 */
export const SAMPLE_BASE_URL = "https://jsonplaceholder.typicode.com";
export function sampleRequests() {
  const req = (name, method, url, extra = {}) => ({ name, url, method, headers: [{ name: "", value: "" }], body: "", preRequestScript: "", postRequestScript: "", ...extra });
  return [
    req(SAMPLE_REQUESTS[0], "GET", "${baseUrl}/posts?_limit=3", {
      postRequestScript: 'pm.test("status is 200", () => pm.response.to.have.status(200));\npm.test("three posts", () => pm.expect(pm.response.json().length).to.eql(3));\n// The next request reads it as ${postId}.\npm.collectionVariables.set("postId", pm.response.json()[0].id);',
    }),
    req(SAMPLE_REQUESTS[1], "GET", "${baseUrl}/posts/${postId}", {
      postRequestScript: 'pm.test("same post", () => pm.expect(pm.response.json().id).to.eql(Number(pm.collectionVariables.get("postId"))));',
    }),
    req(SAMPLE_REQUESTS[2], "POST", "${baseUrl}/posts", {
      headers: [{ name: "Content-Type", value: "application/json" }],
      body: '{\n  "title": "Hello from HITOP",\n  "body": "A sample post",\n  "userId": 1\n}',
      postRequestScript: 'pm.test("created", () => pm.response.to.have.status(201));',
    }),
  ];
}

const shape = (r) => JSON.stringify([r.method, r.url, r.body || "", r.preRequestScript || "", r.postRequestScript || ""]);

/** The sample as added, untouched: only its own requests, as they came. */
export function isPristineSample(c) {
  if (c?.name !== SAMPLE_NAME) return false;
  const as = new Map(sampleRequests().map((r) => [r.name, shape(r)]));
  return (c.requests || []).every((r) => as.get(r.name) === shape(r));
}

/** An empty My requests (or older Default): no request with anything in it, no variables. */
export function isEmptyMyRequests(c) {
  return isMyRequests(c) && (c.requests || []).every(isBlankRequest) && !Object.keys(c.variables || {}).length;
}
