// Unit checks for features.js (no browser): node tests/unit.mjs
import assert from "node:assert/strict";
import { cookieHeader, exportableEnvironments, exportableGlobals, isPostmanGlobals, maskSecrets, mergeGlobals, normalizeGlobals, parseGlobals, mergeCookies, multipartBody, parseEnvironments, parseQuery, secretNames, secretRefs, secretValues, urlEncodedBody, withQuery, withSecretRefs } from "../src/features.js";

const now = Date.parse("2026-10-31T12:00:00Z");
let jar = mergeCookies([], ["sid=abc; Path=/; HttpOnly", "pref=dark; Domain=example.com; Path=/; Max-Age=3600", "evil=1; Domain=other.com", "old=x; Expires=Thu, 01 Jan 2026 00:00:00 GMT"], "https://api.example.com/v1/login", now);
assert.deepEqual(jar.map((c) => c.name).sort(), ["pref", "sid"]);
assert.equal(cookieHeader(jar, "https://api.example.com/v1/me", now), "sid=abc; pref=dark");
assert.equal(mergeCookies([], ["a=1"], "https://x.dev/v1/login", now)[0].path, "/v1", "default path is the request directory");
assert.equal(cookieHeader(jar, "https://www.example.com/", now), "pref=dark", "domain cookie reaches subdomains, host-only doesn't");
assert.equal(cookieHeader(jar, "https://example.org/", now), "");
jar = mergeCookies(jar, ["sid=; Path=/; Max-Age=0"], "https://api.example.com/v1/logout", now);
assert.equal(cookieHeader(jar, "https://api.example.com/", now), "pref=dark", "Max-Age=0 deletes");
assert.equal(cookieHeader(jar, "https://www.example.com/", now + 3601_000), "", "expired cookies aren't sent");

const files = new Map([["f1", new Blob(["hello"], { type: "text/plain" })]]);
files.get("f1").name = "hi.txt";
const { body, contentType } = await multipartBody([{ id: "t", key: "title", value: "A", type: "text" }, { id: "f1", key: "upload", type: "file" }, { id: "x", key: "off", value: "1", type: "text", enabled: false }], files);
const text = new TextDecoder().decode(body);
const boundary = contentType.split("boundary=")[1];
assert.ok(text.startsWith(`--${boundary}\r\nContent-Disposition: form-data; name="title"\r\n\r\nA\r\n`));
assert.ok(text.includes('name="upload"; filename="hi.txt"\r\nContent-Type: text/plain\r\n\r\nhello\r\n'));
assert.ok(!text.includes("off"));
assert.ok(text.endsWith(`--${boundary}--\r\n`));

assert.deepEqual(urlEncodedBody([{ key: "q", value: "a b&c" }, { key: "n", value: "1", enabled: false }]), { body: "q=a%20b%26c", contentType: "application/x-www-form-urlencoded" });
assert.equal(withQuery("https://x/?a=1", parseQuery("https://x/?a=1")), "https://x/?a=1");
// Secrets: values stay in localStorage, never in an export or a masked text.
const legacy = { id: "e1", name: "Prod", variables: { base: "https://x" } };
assert.deepEqual(secretNames(legacy), [], "environments stored before secrets have none");
assert.deepEqual(exportableEnvironments([legacy]), [{ name: "Prod", variables: { base: "https://x" } }], "and export as before");
const prod = { id: "e2", name: "Prod", variables: { base: "https://x", token: "s3cr3t t0ken" }, secrets: ["token"] };
assert.deepEqual(secretValues(prod), ["s3cr3t t0ken"]);
const exported = JSON.stringify(exportableEnvironments([prod]));
assert.ok(!exported.includes("s3cr3t"), exported);
assert.deepEqual(JSON.parse(exported), [{ name: "Prod", variables: { base: "https://x" }, secrets: ["token"] }]);
const back = parseEnvironments(exported)[0];
assert.deepEqual([back.secrets, back.variables], [["token"], { base: "https://x", token: "" }], "import keeps the name, with no value");
const masked = maskSecrets({ url: "https://x/?t=s3cr3t%20t0ken", headers: [{ name: "Authorization", value: "Bearer s3cr3t t0ken" }], statusCode: 200 }, secretValues(prod));
assert.deepEqual(masked, { url: "https://x/?t=••••", headers: [{ name: "Authorization", value: "Bearer ••••" }], statusCode: 200 });
assert.equal(maskSecrets("as is", []), "as is");
console.log("features.js: all unit checks passed");

// --- T5: hitop-core's format in WASM, zip files, the desktop's storage shapes ---
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { formatFromBytes } from "../src/wasm/format.js";
import { historyFromRust, historyToRust } from "../src/platform/history.js";

const here = dirname(fileURLToPath(import.meta.url));
const format = await formatFromBytes(readFileSync(join(here, "../src/wasm/pkg/hitop_wasm_bg.wasm")));
const tree = (dir) => {
  const out = {};
  const walk = (d) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      if (statSync(p).isDirectory()) walk(p);
      else out[relative(dir, p).split("\\").join("/")] = readFileSync(p, "utf8");
    }
  };
  walk(dir);
  return out;
};
// The shared fixtures: hitop-core's golden folder must come out byte for byte.
const golden = join(here, "../../cli/crates/hitop-core/tests/golden/every-field");
const model = {
  collection: JSON.parse(readFileSync(join(golden, "collection.json"), "utf8")),
  environments: JSON.parse(readFileSync(join(golden, "environments.json"), "utf8")),
  globals: JSON.parse(readFileSync(join(golden, "globals.json"), "utf8")),
};
const goldenFiles = tree(join(golden, "folder"));
assert.deepEqual(format.toFiles(model), goldenFiles, "the browser writes the golden folder exactly as the CLI does");
const readBack = format.readFiles(goldenFiles, "folder");
assert.deepEqual(format.toFiles(readBack), goldenFiles, "and reads it back to the same files");
assert.throws(() => format.readFiles({ "hitop.yaml": "format: 2\n" }, "x"), /newer hitop/);

// Old web app data with string ids converts (the T1 open item).
const legacyCols = JSON.parse(JSON.parse(readFileSync(join(here, "legacy-storage.json"), "utf8")).hitop_collections);
const legacyFiles = format.toFiles({ collection: legacyCols[1], environments: [] });
assert.ok(legacyFiles["requests/create-post.yaml"].includes("url: ${base-url}/posts"), Object.keys(legacyFiles).join());

// Zip: export, import, and a zip of the folder's contents only.
const { zipToFiles } = await import("../src/zip.js");
const { zipSync, strToU8 } = await import("fflate");
{
  const { files, name } = zipToFiles(zipSync({ "hitop.yaml": strToU8("format: 1\n"), "requests/a.yaml": strToU8("url: https://a\n"), "__MACOSX/._a.yaml": strToU8("x") }), "My API.zip");
  assert.equal(name, "My API");
  assert.deepEqual(Object.keys(files).sort(), ["hitop.yaml", "requests/a.yaml"]);
  const nested = zipToFiles(zipSync({ "outer/my-api/hitop.yaml": strToU8("format: 1\n"), "outer/my-api/requests/b.yml": strToU8("url: x\n"), "outer/other.txt": strToU8("x") }));
  assert.equal(nested.name, "my-api");
  assert.deepEqual(Object.keys(nested.files).sort(), ["hitop.yaml", "requests/b.yml"]);
  assert.throws(() => zipToFiles(zipSync({ "a.txt": strToU8("x") })), /no hitop.yaml/);
  assert.throws(() => zipToFiles(new Uint8Array([1, 2, 3])), /isn't a zip/);
}

// The desktop's history.json: Rust's HistoryItem ⇄ the web app's items.
const webItems = JSON.parse(JSON.parse(readFileSync(join(here, "legacy-storage.json"), "utf8")).hitop_history);
const rust = historyToRust([...webItems, { id: 5, timestamp: "2026-11-01T00:00:00.000Z", url: "u", method: "POST", statusCode: 201, success: true, collectionName: "C", requestName: "R", isCollectionRun: true, headers: [], body: "b" }]);
assert.deepEqual(format.normalize("history", rust), rust, "Rust reads and keeps every field");
const backItems = historyFromRust(rust);
assert.deepEqual(backItems[0], { ...webItems[0] }, "a web history item comes back as it was");
assert.equal(backItems[1].collectionName, "C");
const cli = historyFromRust(JSON.parse(readFileSync(join(here, "desktop-storage/history.json"), "utf8")));
assert.equal(cli[0].statusCode, 200);
assert.equal(cli[0].timestamp, new Date(1793470570537).toISOString(), "CLI history (ms timestamps) shows");

// The old desktop's files read through Rust's model unchanged.
for (const [kind, file] of [["collections", "collections.json"], ["history", "history.json"], ["environments", "environments.json"]]) {
  const json = JSON.parse(readFileSync(join(here, "desktop-storage", file), "utf8"));
  assert.deepEqual(format.normalize(kind, json), json, `${file} reads and writes unchanged`);
}
// Globals: Postman's file and HITOP's export read in, merged (incoming wins,
// secret or plain as it says), exported without secret values.
const pg = JSON.stringify({ name: "G", _postman_variable_scope: "globals", values: [{ key: "a", value: "1" }, { key: "s", value: "v", type: "secret" }, { key: "off", value: "x", enabled: false }] });
assert.ok(isPostmanGlobals(JSON.parse(pg)) && !isPostmanGlobals({ name: "E", values: [] }));
assert.deepEqual(parseGlobals(pg), { variables: { a: "1", s: "v" }, secrets: ["s"] });
assert.deepEqual(parseGlobals(JSON.stringify({ variables: { b: 2 }, secrets: ["t"] })), { variables: { b: "2", t: "" }, secrets: ["t"] });
const merged = mergeGlobals({ variables: { a: "old", s: "plain", t: "kept" }, secrets: ["t"] }, parseGlobals(pg));
assert.deepEqual(merged, { variables: { a: "1", s: "v", t: "kept" }, secrets: ["s", "t"] });
assert.deepEqual(exportableGlobals(merged), { variables: { a: "1" }, secrets: ["s", "t"] });
assert.deepEqual(normalizeGlobals(null), { variables: {}, secrets: [] }, "storage from before globals reads as empty");
assert.throws(() => parseGlobals("[]"));
assert.deepEqual(format.normalize("globals", { variables: { x: "1" } }), { id: "globals", name: "globals", variables: { x: "1" } });

// GraphQL: the body hitop-core builds, and the schema from an introspection answer.
assert.equal(format.graphqlBody("query Q($id: ID) { u(id: $id) { n } }", '{"id": "7", "a": 1}', "Q"), '{"query":"query Q($id: ID) { u(id: $id) { n } }","variables":{"id": "7", "a": 1},"operationName":"Q"}');
assert.equal(format.graphqlBody("{ me }", "", ""), '{"query":"{ me }"}');
assert.throws(() => format.graphqlBody("{ me }", "{id: 1}", ""), /not valid JSON/);
const gqlRequest = { id: 1, name: "Me", url: "u", method: "POST", headers: [], body: "{ me }", preRequestScript: "", postRequestScript: "", bodyType: "graphql", graphql: { variables: '{"a": 1}', operationName: "Me" } };
assert.deepEqual(format.normalize("collections", [{ id: 1, name: "c", requests: [gqlRequest] }])[0].requests[0], gqlRequest, "Rust keeps a GraphQL request as saved");
// WebSocket: Rust keeps kind and websocket (subprotocols, named messages) as the web app saves them.
const wsRequest = { id: 2, name: "Feed", url: "wss://x/feed", method: "GET", headers: [], body: "", preRequestScript: "", postRequestScript: "", kind: "websocket", websocket: { subprotocols: ["v2"], messages: [{ name: "hi", content: "hello ${who}" }] } };
assert.deepEqual(format.normalize("collections", [{ id: 1, name: "c", requests: [wsRequest] }])[0].requests[0], wsRequest, "Rust keeps a WebSocket request as saved");
// gRPC: Rust keeps kind and grpc (service, method, message, protos, importPaths, plaintext) as the web app saves them.
const grpcRequest = { id: 3, name: "Hello", url: "${grpcHost}", method: "GET", headers: [{ name: "x-tenant", value: "t" }], body: "", preRequestScript: "", postRequestScript: "", kind: "grpc", grpc: { service: "a.Greeter", method: "SayHello", message: '{"name": "${who}"}', protos: ["protos/a.proto"], importPaths: ["protos/include"], plaintext: true } };
assert.deepEqual(format.normalize("collections", [{ id: 1, name: "c", requests: [grpcRequest] }])[0].requests[0], grpcRequest, "Rust keeps a gRPC request as saved");
const { schemaFromResponse, INTROSPECTION_QUERY } = await import("../src/graphql-schema.js");
const { buildSchema, graphqlSync } = await import("graphql");
const sdl = buildSchema("type Query { user(id: ID!): User } type User { name: String }");
const introspection = JSON.stringify(graphqlSync({ schema: sdl, source: INTROSPECTION_QUERY }));
assert.ok(schemaFromResponse(introspection).getType("User").getFields().name);
assert.throws(() => schemaFromResponse('{"errors": [{"message": "introspection is off"}]}'), /introspection is off/);

// Response diff (src/diff.js): JSON by structure, key order ignored; text by line.
{
  const { compareResponses, diffJson, diffLines, diffHeaders, exampleResponse, sideBySide, snapshotResponse, SNAPSHOT_LIMIT } = await import("../src/diff.js");
  assert.deepEqual(diffJson({ a: 1, b: { c: [1, 2] }, "x-y": true }, { b: { c: [1, 3, 4] }, a: 1, z: null }), [
    { path: "$.b.c[1]", kind: "changed", before: 2, after: 3 },
    { path: "$.b.c[2]", kind: "added", after: 4 },
    { path: '$["x-y"]', kind: "removed", before: true },
    { path: "$.z", kind: "added", after: null },
  ]);
  assert.deepEqual(diffJson({ a: 1, b: 2 }, { b: 2, a: 1 }), [], "key order doesn't matter");
  assert.deepEqual(diffJson({ a: "1" }, { a: 1 }), [{ path: "$.a", kind: "changed", before: "1", after: 1 }], "types count");
  const same = compareResponses({ status: 200, headers: { "Content-Type": "application/json" }, body: '{"a":1,"b":2}' }, { status: 200, headers: { "content-type": "application/json" }, body: '{\n  "b": 2,\n  "a": 1\n}' });
  assert.ok(same.same && same.body.kind === "json", "formatting and key order alone are no difference");
  const c = compareResponses({ status: 200, headers: { etag: "1", date: "x" }, body: "one\ntwo\nthree" }, { status: 404, headers: { etag: "2", "x-new": "y", date: "x" }, body: "one\n2\nthree\nfour" });
  assert.deepEqual([c.same, c.status, c.body.kind], [false, { before: 200, after: 404, same: false }, "text"]);
  assert.deepEqual(c.headers.filter((h) => h.kind !== "same").map((h) => `${h.name}:${h.kind}`), ["etag:changed", "x-new:added"]);
  assert.deepEqual(c.body.lines.map((l) => l.op[0] + l.text), ["sone", "rtwo", "a2", "sthree", "afour"]);
  assert.deepEqual(sideBySide(c.body.lines).map((r) => r.op), ["same", "changed", "same", "added"]);
  assert.deepEqual(diffLines("", "a").map((l) => l.op), ["added"]);
  assert.deepEqual(diffLines("a\nb\nc", "a\nx\nc", 0).map((l) => l.op), ["same", "removed", "added", "same"], "over the limit: no LCS, still correct");
  assert.deepEqual(diffHeaders({ A: "1" }, { a: "1" }), [{ name: "a", kind: "same", before: "1", after: "1" }]);
  const big = snapshotResponse({ status: 200, headers: { a: "1" }, body: "x".repeat(SNAPSHOT_LIMIT + 5), durationMs: 3 });
  assert.deepEqual([big.body.length, big.truncated, big.durationMs], [SNAPSHOT_LIMIT, true, 3]);
  assert.deepEqual(exampleResponse({ name: "created", status: 201, body: '{"id": 42}' }), { status: 201, headers: {}, body: '{"id": 42}' }, "an example as the folder format writes it");
  assert.deepEqual(exampleResponse({ status: "200", headers: [{ name: "X-A", value: 1 }], body: { id: 1 } }).headers, { "x-a": "1" });
}

// Code snippets through hitop-wasm (the CLI's generator): secrets as {{NAME}}.
{
  const out = format.snippet({ request: { id: 0, name: "r", url: "${base}/u", method: "GET", headers: [{ name: "Authorization", value: "Bearer ${token}" }], body: "" }, variables: { base: "https://api.test", token: "s3cr3t" }, secrets: ["token"] }, "python");
  assert.ok(out.code.includes('"Bearer {{token}}"') && !out.code.includes("s3cr3t") && out.code.includes("https://api.test/u"), out.code);
  assert.deepEqual(out.placeholders, ["token"]);
  assert.deepEqual(format.snippetLanguages().map((l) => l.id), ["curl", "fetch", "python", "go", "java", "rust"]);
}

// ${secret:...} references: found outside scripts, filled in variables and their values.
{
  const request = { url: "${base}/x?t=${secret:vault:kv/app#token}", headers: [{ name: "A", value: "${secret:op:v/i/f}" }], preRequestScript: "// ${secret:aws:no}", examples: [{ body: "${secret:aws:no}" }] };
  assert.deepEqual([...secretRefs(request)], ["secret:vault:kv/app#token", "secret:op:v/i/f"]);
  assert.deepEqual([...secretRefs(["${secret:azure:v/n} ${secret:unterminated"])], ["secret:azure:v/n"]);
  const vars = withSecretRefs({ base: "b", key: "k-${secret:azure:v/n}", other: "${secret:aws:missing}" }, { "secret:azure:v/n": "AZ" });
  assert.deepEqual(vars, { base: "b", key: "k-AZ", other: "${secret:aws:missing}", "secret:azure:v/n": "AZ" });
  const same = { a: "1" };
  assert.equal(withSecretRefs(same, {}), same);
}

console.log("unit checks passed");
