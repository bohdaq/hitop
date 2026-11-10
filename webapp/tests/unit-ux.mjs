// Unit checks for the 1.2 UX helpers (no browser): node tests/unit-ux.mjs
import assert from "node:assert/strict";
import { addCollections, folderOf, folderPaths, moveRequestTo, renameFolderIn, requestFile, requestTree, slugName, withFolders } from "../src/collection-tree.js";
import { clampSplit, layoutFor, splitAt, splitForKey } from "../src/split.js";
import { fuzzyMatch, quickOpenItems, rankItems } from "../src/quick-open.js";

// Requests and collections UX: file names as hitop-core's names::slug, folders made on screen,
// moves between collections and folders, folder renames.
assert.equal(slugName("List Users (v2)!"), "list-users-v2");
assert.equal(slugName("  "), "request");
assert.equal(slugName("CON"), "con-file");
assert.equal(slugName("Über Straße"), "über-straße");
assert.equal(slugName("x".repeat(100)).length, 80);
const inFolder = [{ id: 1, name: "Me", file: "admin/me" }, { id: 2, name: "List" }];
assert.equal(requestFile(inFolder, "admin", "Me"), "admin/me-2", "a taken file gets a number");
assert.equal(requestFile(inFolder, "", "Me"), "me");
assert.equal(requestFile(inFolder, "admin/deep", "New request"), "admin/deep/new-request");
const tree = withFolders(requestTree(inFolder), ["admin", "admin/keys", "billing"]);
assert.deepEqual(folderPaths(tree), ["admin", "admin/keys", "billing"], "existing folders kept once, new ones added where they come");
assert.equal(tree.folders[0].folders[0].name, "keys");
const cols = [
  { id: 1, name: "A", requests: [{ id: 10, name: "One", file: "x/one", bodyMode: "raw" }, { id: 11, name: "Two" }] },
  { id: 2, name: "B", requests: [{ id: 20, name: "Three", file: "y/three" }] },
];
let moved = moveRequestTo(cols, { from: 1, requestId: 10, to: 2, folder: "", beforeId: 20 });
assert.deepEqual(moved[0].requests.map((r) => r.id), [11]);
assert.deepEqual(moved[1].requests.map((r) => r.id), [10, 20], "before the anchor");
assert.deepEqual(moved[1].requests[0], { id: 10, name: "One", bodyMode: "raw" }, "every field but `file` comes along at the top level");
moved = moveRequestTo(cols, { from: 1, requestId: 11, to: 2, folder: "y" });
assert.deepEqual(moved[1].requests.map((r) => r.file), ["y/three", "y/two"], "a folder gives the file; at the end without an anchor");
moved = moveRequestTo(cols, { from: 1, requestId: 11, to: 1, folder: "x", beforeId: 10 });
assert.deepEqual(moved[0].requests.map((r) => [r.id, r.file]), [[11, "x/two"], [10, "x/one"]], "within a collection: into a folder, before its first request");
assert.equal(moveRequestTo(cols, { from: 1, requestId: 10, to: 1, folder: "x" })[0].requests[1].file, "x/one", "same folder: file kept");
assert.equal(moveRequestTo(cols, { from: 1, requestId: 99, to: 2 }), cols, "unknown request: unchanged");
assert.equal(moveRequestTo(cols, { from: 1, requestId: 10, to: 9 }), cols, "unknown collection: unchanged");
assert.deepEqual(
  renameFolderIn([{ id: 1, file: "admin/me" }, { id: 2, file: "admin/keys/rotate" }, { id: 3, file: "administration/x" }, { id: 4 }], "admin", "staff").map((r) => r.file),
  ["staff/me", "staff/keys/rotate", "administration/x", undefined],
  "the folder and its subfolders move; a folder with the same prefix doesn't",
);

// Import that adds: ids in use get new ones, names in use get " (2)", " (3)".
let next = 100;
const newId = () => next++;
const existing = [
  { id: 1, name: "Default", requests: [] },
  { id: 2, name: "Shop", requests: [] },
  { id: 3, name: "Shop (2)", requests: [] },
];
const { added, renamed } = addCollections(existing, [{ id: 1, name: "Shop", requests: [{ id: 9 }] }, { id: 7, name: "New" }, { name: "  " }, { id: 8, name: "New" }], newId);
assert.deepEqual(
  added.map((c) => [c.id, c.name]),
  [
    [100, "Shop (3)"],
    [7, "New"],
    [101, "Imported"],
    [8, "New (2)"],
  ],
);
assert.deepEqual(added[0].requests, [{ id: 9 }], "requests come along as they are");
assert.deepEqual(renamed, [
  ["Shop", "Shop (3)"],
  ["New", "New (2)"],
]);
assert.deepEqual(addCollections([], [{ id: "a", name: "A" }], newId).added, [{ id: "a", name: "A" }], "nothing to clash with: unchanged");
assert.equal(addCollections([{ id: 5, name: "x" }], [{ id: "5", name: "y" }], newId).added[0].id, 102, "ids compare as text (web string ids, desktop numbers)");

// Folder groups from `file` paths.
assert.equal(folderOf({ file: "account/me" }), "account");
assert.equal(folderOf({ file: "a/b/c" }), "a/b");
assert.equal(folderOf({ file: "me" }), "");
assert.equal(folderOf({}), "");
assert.equal(folderOf({ file: "a\\b\\c" }), "a/b", "Windows separators");
const reqs = [{ name: "top" }, { name: "me", file: "account/me" }, { name: "deep", file: "account/admin/x" }, { name: "list", file: "orders/list" }, { name: "again", file: "account/again" }, { name: "last" }];
const t = requestTree(reqs);
assert.deepEqual(
  t.items.map((i) => (i.folder ? `[${i.folder.path}]` : i.request.name)),
  ["top", "[account]", "[orders]", "last"],
  "a folder sits where its first request is",
);
const account = t.folders[0];
assert.deepEqual(
  account.items.map((i) => (i.folder ? `[${i.folder.name}]` : `${i.request.name}@${i.index}`)),
  ["me@1", "[admin]", "again@4"],
);
assert.equal(account.folders[0].path, "account/admin");
assert.deepEqual(account.folders[0].requests.map((e) => e.index), [2]);

// The split between the editor and the response.
assert.equal(layoutFor(1200), "columns");
assert.equal(layoutFor(880), "columns");
assert.equal(layoutFor(700), "rows");
assert.equal(layoutFor(358), "stack");
assert.equal(clampSplit("columns", 10), 25);
assert.equal(clampSplit("columns", 90), 75);
assert.equal(clampSplit("columns", NaN), 50);
assert.equal(clampSplit("rows", 5000), 900);
assert.equal(splitAt("columns", 600, 100, 1000), 50);
assert.equal(splitAt("rows", 500, 200, 0), 300);
assert.equal(splitForKey("columns", 50, "ArrowRight"), 55);
assert.equal(splitForKey("columns", 50, "ArrowLeft", true), 30);
assert.equal(splitForKey("columns", 50, "ArrowDown"), null, "columns move sideways only");
assert.equal(splitForKey("rows", 280, "ArrowDown"), 320);
assert.equal(splitForKey("rows", 280, "Home"), 160);
assert.equal(splitForKey("columns", 70, "Enter"), 50);
assert.equal(splitForKey("columns", 50, "a"), null);

// Quick open: fuzzy matching and ranking.
assert.equal(fuzzyMatch("xyz", "Get user"), null);
assert.deepEqual(fuzzyMatch("gu", "Get user").at, [0, 4], "word starts");
assert.ok(fuzzyMatch("user", "Get user").score > fuzzyMatch("user", "uxsxexr").score, "consecutive beats spread");
assert.ok(fuzzyMatch("lo", "List orders").score > fuzzyMatch("lo", "Hello").score, "word starts beat the middle of a word");
assert.deepEqual(fuzzyMatch("", "x"), { score: 0, at: [] });
assert.deepEqual(fuzzyMatch("cu", "createUser").at, [0, 6], "camelCase counts as a word start");
const methodLabel = (r) => (r.kind === "websocket" ? "WS" : r.method);
const collections = [
  { id: 1, name: "Shop", requests: [{ id: 11, name: "List orders", method: "GET", url: "${base}/orders", file: "orders/list" }, { id: 12, name: "Create order", method: "POST", url: "${base}/orders" }] },
  { id: 2, name: "Auth", requests: [{ id: 21, name: "Login", method: "POST", url: "https://id.example.com/token" }, { id: 22, name: "Live feed", kind: "websocket", method: "GET", url: "wss://x/feed" }] },
];
const qtabs = [
  { id: "t1", title: "Login", loadedRequestId: 21, loadedCollectionId: 2, method: "POST", url: "https://id.example.com/token" },
  { id: "t2", title: "GET api.example.com/health", loadedRequestId: null, method: "GET", url: "api.example.com/health" },
];
const items = quickOpenItems({ collections, tabs: qtabs, methodLabel });
assert.deepEqual(
  items.map((i) => i.key),
  ["tab:t2", "req:1:11", "req:1:12", "req:2:21", "req:2:22"],
  "unsaved tabs, then every saved request once (an open one isn't listed twice)",
);
assert.equal(items.find((i) => i.key === "req:2:21").open, true);
assert.equal(items.find((i) => i.key === "req:1:11").detail, "Shop › orders");
assert.equal(items.find((i) => i.key === "req:2:22").method, "WS");
assert.deepEqual(rankItems(items, "").map((i) => i.key), items.map((i) => i.key), "no query: as listed");
assert.equal(rankItems(items, "lord")[0].title, "List orders");
assert.equal(rankItems(items, "login")[0].key, "req:2:21");
assert.equal(rankItems(items, "health")[0].kind, "tab");
assert.ok(rankItems(items, "token").some((i) => i.title === "Login"), "the URL is searched too");
assert.equal(rankItems(items, "qqq").length, 0);
assert.equal(rankItems(items, "o", 2).length, 2, "limit");
// Collections, environments (by variable name) and history (newest first, once each) after the requests.
const more = quickOpenItems({ collections, tabs: [], methodLabel, environments: [{ id: "e1", name: "staging", variables: { base: "x", token: "" } }], history: [{ method: "GET", url: "https://a.test/x", statusCode: 200 }, { method: "GET", url: "https://b.test/y" }, { method: "GET", url: "https://a.test/x", statusCode: 500 }] });
assert.deepEqual(more.filter((i) => i.kind !== "request").map((i) => i.key), ["col:1", "col:2", "env:e1", "hist:GET https://a.test/x", "hist:GET https://b.test/y"]);
assert.equal(more.find((i) => i.kind === "history").detail, "History, 500", "the newest entry of a URL");
assert.equal(rankItems(more, "token")[0].kind, "environment", "an environment is found by a variable name");
assert.equal(rankItems(more, "auth")[0].key, "col:2");

// Run report (run-report.js): report.json v1, JUnit, p95, the CLI line, the run history.
{
  const { buildReport, junitXml, percentile, previousRun, rememberRun, runCommand, runStats, shellWord, timeline } = await import("../src/run-report.js");
  assert.equal(percentile([10, 20, 30, 40, 1000]), 1000);
  assert.equal(percentile([5, 1, 3]), 5);
  assert.equal(percentile([]), null);
  assert.equal(shellWord("Shop API"), "'Shop API'");
  assert.equal(shellWord("it's"), "'it'\\''s'");
  assert.equal(runCommand({ collection: "Shop API", folderLabel: "shop-api", environment: "Staging" }), "hitop run ./shop-api --env Staging --bail --reporter cli,junit,json --out reports");
  assert.equal(runCommand({ collection: "Shop API", environment: null }), "hitop run 'Shop API' --bail --reporter cli,junit,json --out reports");
  const timings = { dnsMs: 1, connectMs: 2, waitMs: 30, downloadMs: 1, totalMs: 34, reused: false };
  const results = [
    { requestName: "Log in", method: "POST", url: "https://x/login", statusCode: 201, success: true, tests: [{ name: "ok", pass: true }], duration: 40, timings, file: "shop/requests/log-in.yaml" },
    { requestName: "Cancel", method: "DELETE", url: "https://x/o/1", statusCode: 409, success: false, tests: [{ name: "status is 204", pass: false, detail: "got 409" }], duration: 60 },
    { requestName: "Never", method: "GET", url: "https://x/n", statusCode: "Error", success: false, response: "connection refused" },
  ];
  const report = buildReport({ collection: "Shop & <API>", environment: "Staging", results, startedAt: Date.UTC(2026, 10, 9, 10, 0, 0), durationMs: 120 });
  assert.equal(report.reportVersion, 1);
  assert.equal(report.startedAt, "2026-11-09T10:00:00Z");
  assert.deepEqual(report.summary, { requests: { total: 3, passed: 1, failed: 2 }, assertions: { total: 2, passed: 1, failed: 1 } });
  const [a, b, c] = report.collections[0].iterations[0].requests;
  assert.deepEqual(Object.keys(a), ["name", "method", "url", "passed", "status", "responseTimeMs", "durationMs", "error", "failure", "assertions", "file", "timings"], "spec key order, timings last");
  assert.equal(a.responseTimeMs, 34);
  assert.equal(b.failure, "status is 204: got 409");
  assert.equal(b.timings, undefined, "no timings key without timings");
  assert.deepEqual([c.status, c.error, c.durationMs], [null, "connection refused", 0]);
  const xmlText = junitXml(report);
  assert.ok(xmlText.includes('tests="3" failures="1" errors="1"') && xmlText.includes("Shop &amp; &lt;API&gt;") && xmlText.includes('file="shop/requests/log-in.yaml"'), xmlText);
  const rows = timeline(results);
  assert.deepEqual(rows.map((r) => r.start), [0, 40, 100]);
  assert.ok(Math.abs(rows[1].left - 40) < 1e-9);
  const store = new Map();
  const storage = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v) };
  assert.equal(previousRun(storage, 7), null);
  rememberRun(storage, 7, { at: 1, p95: 50 });
  const stats = runStats(results, 120, previousRun(storage, 7));
  assert.deepEqual([stats.requests, stats.checks, stats.p95, stats.p95Delta, stats.wallMs], [{ passed: 1, total: 3 }, { passed: 1, total: 2 }, 60, 10, 120]);
  for (let i = 0; i < 30; i++) rememberRun(storage, 7, { at: i, p95: i });
  assert.equal(JSON.parse(store.get("hitop_run_history"))["7"].length, 20, "20 runs per collection");
  rememberRun({ getItem: () => "{", setItem: () => { throw new Error("full"); } }, 1, {});
}

// The agent's patches (agent.js).
{
  const { validatePatch, requestText, tabUpdates, PATCH_SCHEMA } = await import("../src/agent.js");
  const current = { method: "GET", url: "https://x/o", headers: [{ name: "Authorization", value: "Bearer real" }, { name: "X-Old", value: "1", enabled: false }], body: "", postRequestScript: "" };
  const nulls = { method: null, url: null, headers: null, body: null, preRequestScript: null, postRequestScript: null };
  assert.deepEqual(PATCH_SCHEMA.properties.changes.required, Object.keys(nulls));
  assert.equal(validatePatch(null).ok, false);
  assert.equal(validatePatch({ summary: "x" }).ok, false, "changes required");
  assert.match(validatePatch({ summary: "x", changes: { ...nulls, auth: "basic" } }).error, /auth/);
  assert.match(validatePatch({ summary: "x", changes: { ...nulls, method: "FETCH" } }).error, /FETCH/);
  assert.match(validatePatch({ summary: "x", changes: { ...nulls, body: 3 } }).error, /text/);
  const ok = validatePatch({ summary: " Use POST ", changes: { ...nulls, method: "post", url: "https://x/o", body: "{}", postRequestScript: "// ••••", headers: [{ name: "Authorization", value: "Bearer ••••" }, { name: "Content-Type", value: "application/json" }] } }, current);
  assert.ok(ok.ok);
  assert.deepEqual(ok.patch.changes, { method: "POST", headers: [{ name: "Authorization", value: "Bearer real" }, { name: "Content-Type", value: "application/json" }], body: "{}" }, "unchanged url dropped, masked header restored");
  assert.deepEqual(ok.dropped, ["postRequestScript"], "a field holding •••• is never written");
  assert.equal(ok.patch.summary, "Use POST");
  const unknownMasked = validatePatch({ summary: "x", changes: { ...nulls, headers: [{ name: "X-Key", value: "••••" }] } }, current);
  assert.deepEqual([unknownMasked.patch.changes.headers, unknownMasked.dropped], [undefined, ["headers"]], "a masked header nobody had: the headers are left alone");
  const up = tabUpdates({ body: "{}", headers: [{ name: "A", value: "1" }] }, current);
  assert.deepEqual(up, { requestBody: "{}", headers: [{ name: "A", value: "1" }, { name: "X-Old", value: "1", enabled: false }, { name: "", value: "" }] }, "disabled headers stay");
  assert.equal(requestText({ ...current, postRequestScript: "pm.test('a', () => {});" }), "GET https://x/o\nAuthorization: Bearer real\n\n# Script after\npm.test('a', () => {});");
}

// The Organization area (org.js).
{
  const { limitInfo, meters, planCards, sectionsFor } = await import("../src/org.js");
  assert.deepEqual(sectionsFor({ role: "member" }).map((s) => s[0]), ["general", "members", "billing", "tokens", "runs"]);
  assert.ok(sectionsFor({ role: "owner", personal: false }).some((s) => s[0] === "security"));
  assert.ok(!sectionsFor({ role: "owner", personal: true }).some((s) => s[0] === "security"), "no SSO for a personal org");
  const m = meters({ usage: { seats: 6, guestViewers: 0, workspaces: 2, storageBytes: 1.2 * 1024 ** 3, devices: 3, runners: 4 }, limits: { seats: 6, guests: 0, workspaces: null, storageBytes: 16 * 1024 ** 3, devices: 10, runners: 5 } });
  assert.deepEqual(m.map((x) => [x.id, x.level]), [["seats", "full"], ["workspaces", "ok"], ["storage", "ok"], ["devices", "ok"], ["runners", "warn"]], "no guest meter on a plan without guests");
  assert.equal(m.find((x) => x.id === "storage").text, "1.2 GB of 16 GB");
  assert.equal(m.find((x) => x.id === "workspaces").text, "2 · no limit");
  const cards = planCards({ plan: "free", personal: false }, { offers: [{ plan: "team", period: "annual" }, { plan: "team", period: "monthly" }, { plan: "business", period: "annual" }] });
  assert.deepEqual(cards.map((c) => [c.plan, c.buy, c.period, c.current]), [["free", false, null, true], ["team", true, "monthly", false], ["business", true, "annual", false]]);
  assert.deepEqual(planCards({ plan: "free", personal: true }, { offers: [{ plan: "pro", period: "monthly" }] }).map((c) => c.plan), ["free", "pro"]);
  const { parseOrgLink } = await import("../src/org.js");
  assert.deepEqual(parseOrgLink("#org=o%201&section=billing"), { org: "o 1", section: "billing" });
  assert.deepEqual(parseOrgLink("#section=tokens"), { org: null, section: "tokens" });
  assert.equal(parseOrgLink("#git=ok"), null);
  assert.equal(parseOrgLink("#section=evil"), null);
  // The portal's side (cloud/portal/src/app-redirect.js) produces exactly what the app reads.
  const { appLink, appSection } = await import("../../cloud/portal/src/app-redirect.js");
  for (const [path, want] of [["/orgs/acme/plan", { org: "acme", section: "billing" }], ["/orgs/acme/members/", { org: "acme", section: "members" }], ["/orgs/acme", { org: "acme", section: "general" }], ["/orgs/acme/settings/danger", { org: "acme", section: "general" }], ["/tokens", { org: null, section: "tokens" }], ["/orgs", { org: null, section: "general" }]]) {
    assert.deepEqual(appSection(path), want, path);
    assert.deepEqual(parseOrgLink(new URL(appLink(path)).hash), want, path);
  }
  for (const kept of ["/", "/sign-in", "/account", "/workspaces", "/workspaces/w/runs", "/orgs/acme/settings/sso", "/orgs/acme/settings/scim", "/orgs/acme/settings/admin", "/orgs/acme/settings/audit"]) assert.equal(appLink(kept), null, kept);
  assert.equal(limitInfo({ status: 500 }), null);
  const li = limitInfo({ status: 402, message: "Free allows 3 workspaces", body: { error: "limit", limit: "workspaces", plan: "free", used: 3, max: 3, org: "o1" } });
  assert.deepEqual([li.headline, li.org, li.message], ["Free has used its 3 shared collections", "o1", "Free allows 3 workspaces"]);
}

// The scripts as the extension's sandbox runs them (script-run.js; the web build runs the same).
{
  const fs = await import("node:fs");
  const { loadShim } = await import("../src/pm.js");
  const { runPre, runPost } = await import("../src/script-run.js");
  const shim = loadShim(fs.readFileSync(new URL("../../shared/pm-shim.js", import.meta.url), "utf8"));
  const pre = runPre(shim, { script: 'setHeader("X-A", "1"); setContext("k", "v"); pm.environment.set("e", "2");', url: "https://x/", method: "GET", headers: [], body: "", variables: {}, context: {}, globals: {} });
  assert.deepEqual([pre.headers, pre.contextUpdates, pre.variables], [[{ name: "X-A", value: "1" }], { k: "v" }, { e: "2" }]);
  const post = runPost(shim, { script: 'pm.test("ok", () => pm.response.to.have.status(200));\nthrow new Error("boom")', text: "{}", headers: {}, status: 200, durationMs: 1, variables: {}, context: {}, globals: {} });
  assert.deepEqual([post.tests.length, post.tests[0].pass, post.error], [1, true, "boom"], "a throwing script keeps what it set and says why");
  assert.ok(JSON.parse(JSON.stringify(post)), "plain data: crosses postMessage");
}

// The request page's facts (request-facts.js): HTTP version labels, body kind, JSON check with line/col.
{
  const { shortVersion, protocolLabel, bodyKind, checkJson } = await import("../src/request-facts.js");
  assert.deepEqual(["HTTP/2", "HTTP/3", "HTTP/1.1", undefined].map(shortVersion), ["h2", "h3", "HTTP/1.1", ""]);
  assert.equal(protocolLabel("HTTP/2", "https://api.example.com/x"), "HTTPS · h2");
  assert.equal(protocolLabel("HTTP/1.1", "http://localhost:3000/"), "HTTP · HTTP/1.1");
  assert.equal(protocolLabel(null, "https://x/"), "", "no version (a direct browser fetch): nothing");
  assert.equal(bodyKind("<a/>", "application/json"), "JSON", "the Content-Type wins");
  assert.equal(bodyKind("{}", "text/plain"), "Text");
  assert.equal(bodyKind("", "application/soap+xml"), "XML");
  assert.deepEqual([bodyKind(' {"a":1}'), bodyKind("<a/>"), bodyKind("hello")], ["JSON", "XML", "Text"]);
  assert.deepEqual(checkJson('{"a": [1, 2.5e3, -0, true, false, null, "x\\u00e9\\n"]}'), { ok: true });
  assert.deepEqual(checkJson('{"id": ${orderId}, "name": "${name}", "n": [${a}]}'), { ok: true }, "${variables} stand for values");
  assert.deepEqual(checkJson('{\n  "a": 1,\n  "b": 2,\n}'), { ok: false, line: 4, col: 1 }, "trailing comma: the brace");
  assert.deepEqual(checkJson('{\n  "a": 1\n  "b": 2\n}'), { ok: false, line: 3, col: 3 }, "missing comma: the next key");
  assert.deepEqual(checkJson("{'a': 1}"), { ok: false, line: 1, col: 2 });
  assert.deepEqual(checkJson('{"a": 01}'), { ok: false, line: 1, col: 8 });
  assert.deepEqual(checkJson('{"a": "x'), { ok: false, line: 1, col: 9 }, "unterminated string: the end");
  assert.deepEqual(checkJson('{"a": tru}'), { ok: false, line: 1, col: 7 });
  assert.deepEqual(checkJson("[1] 2"), { ok: false, line: 1, col: 5 }, "text after the value");
  assert.deepEqual(checkJson(""), { ok: false, line: 1, col: 1 });
  for (const s of ['{"a":[1,{"b":null}]}', "[]", '"x"', "  3  ", '{"\\u12G4":1}', "[1,]", '{"a"}']) {
    let native = true;
    try {
      JSON.parse(s);
    } catch {
      native = false;
    }
    assert.equal(checkJson(s).ok, native, `agrees with JSON.parse on ${s}`);
  }
}

// U6: the newcomer's three steps.
{
  const { gettingStartedSteps, showGettingStarted, hideGettingStarted, SAMPLE_NAME } = await import("../src/getting-started.js");
  const mem = () => {
    const m = new Map();
    return { getItem: (k) => (m.has(k) ? m.get(k) : null), setItem: (k, v) => m.set(k, String(v)) };
  };
  const done = (x) => x.map((s) => s.done);
  assert.deepEqual(done(gettingStartedSteps({})), [false, false, false]);
  assert.deepEqual(done(gettingStartedSteps({ history: [{}], collections: [{ name: SAMPLE_NAME, requests: [{ url: "x" }] }] })), [true, false, false], "the sample doesn't count as saving one's own");
  assert.deepEqual(done(gettingStartedSteps({ collections: [{ name: "Mine", requests: [{ url: "" }, { url: "https://x" }] }] })), [false, true, false]);
  assert.deepEqual(done(gettingStartedSteps({ runs: { 1: [{}] } })), [false, false, true]);
  assert.deepEqual(done(gettingStartedSteps({ runs: { 1: [] }, ranNow: false })), [false, false, false]);
  assert.equal(done(gettingStartedSteps({ ranNow: true }))[2], true);
  const open = gettingStartedSteps({});
  const a = mem();
  assert.equal(showGettingStarted(a, { fresh: true, steps: open }), true, "a newcomer sees it");
  assert.equal(showGettingStarted(a, { fresh: false, steps: open }), true, "...and keeps seeing it once they have data");
  assert.equal(showGettingStarted(a, { fresh: true, steps: gettingStartedSteps({ history: [{}], collections: [{ name: "M", requests: [{ url: "x" }] }], ranNow: true }) }), false, "all done: gone");
  hideGettingStarted(a);
  assert.equal(showGettingStarted(a, { fresh: true, steps: open }), false, "hidden stays hidden");
  const b = mem();
  assert.equal(showGettingStarted(b, { fresh: false, steps: open }), false, "someone who already had requests never sees it");
  assert.equal(showGettingStarted(b, { fresh: true, steps: open }), false, "...even after emptying the app");
  assert.equal(showGettingStarted(null, { fresh: true, steps: open }), true, "no storage: from this page's state");
}

// U39: a run's failure explains itself.
{
  const { explainFailure } = await import("../src/run-report.js");
  const v = explainFailure({ statusCode: "Error", response: "Variable 'token' not found in variables object", url: "https://x/${token}" });
  assert.deepEqual([v.kind, v.cause], ["not-sent", "${token} has no value: no active environment, collection variable or global defines it, so nothing was sent."]);
  assert.equal(explainFailure({ statusCode: "Not sent", response: "Secret TOKEN has no value on this device." }).kind, "not-sent");
  assert.equal(explainFailure({ statusCode: "Script error", response: "boom" }).cause, "The script before the request stopped with an error, so nothing was sent.");
  const net = explainFailure({ statusCode: "Error", response: "Failed to fetch", url: "http://127.0.0.1:9/x" });
  assert.deepEqual([net.kind, net.cause], ["no-answer", "No answer from 127.0.0.1:9: is the server running, and is the address right?"]);
  assert.equal(explainFailure({ statusCode: "Error", response: "something odd" }).cause, null, "nothing known: no guess");
  const st = explainFailure({ statusCode: 404, success: false, tests: [], excerpt: ' {"error":"no such order"} ', url: "https://x/o/9" });
  assert.deepEqual([st.kind, st.status, st.excerpt], ["status", 404, '{"error":"no such order"}']);
  assert.match(st.cause, /^Nothing answers at this path/);
  assert.match(explainFailure({ statusCode: 503, tests: [] }).cause, /^The server failed/);
  assert.equal(explainFailure({ statusCode: 418, tests: [] }).cause, null);
  const ck = explainFailure({ statusCode: 200, tests: [{ name: "a", pass: true }, { name: "total > 0", pass: false, detail: "got 0" }] });
  assert.deepEqual([ck.kind, ck.checks.map((c) => c.name), ck.cause], ["checks", ["total > 0"], null]);
}

// U49 usage warnings before a limit, U50 who can lift one.
{
  const { meters, usageWarnings, shareRoom, limitFixers } = await import("../src/org.js");
  const free = (workspaces, seats = 1) => ({ plan: "free", planInForce: "free", usage: { seats, workspaces, storageBytes: 1024, devices: 1, runners: 0 }, limits: { seats: 3, guests: 0, workspaces: 3, storageBytes: 100 * 1024 ** 2, devices: 3, runners: 5 } });
  const ws = (u) => meters(u).find((m) => m.id === "workspaces");
  assert.deepEqual([ws(free(1)).level, ws(free(2)).level, ws(free(3)).level], ["ok", "warn", "full"], "2 of 3 warns: one left, though under 80%");
  assert.equal(ws(free(2)).note, "Nearly full: 1 left");
  assert.equal(meters({ usage: { runners: 0 }, limits: { runners: 1 } })[0].level, "ok", "no one-left warning for a limit of 1 with none used");
  assert.deepEqual(usageWarnings(free(1)), [], "plenty of room: nothing to warn about");
  assert.deepEqual(usageWarnings(free(3, 2)).map((m) => [m.id, m.level]), [["workspaces", "full"], ["seats", "warn"]], "fullest first");
  assert.deepEqual(usageWarnings({ usage: { workspaces: 9 }, limits: null }), [], "a server without limits claims none");
  assert.deepEqual(usageWarnings(null), []);
  const acme = { id: "o1", name: "Acme", role: "member", personal: false };
  assert.equal(shareRoom(free(1), acme), null);
  assert.deepEqual(shareRoom(free(2), acme), { level: "warn", text: "2 of 3 shared collections of Acme (Free) are used: sharing this one takes the last one." });
  assert.equal(shareRoom(free(3), { personal: true }).level, "full");
  assert.match(shareRoom(free(3), { personal: true }).text, /^All 3 shared collections of your personal account \(Free\) are used/);
  assert.equal(shareRoom({ usage: { workspaces: 5 }, limits: { workspaces: null } }, acme), null, "no limit, no line");
  const members = { members: [{ email: "me@x.test", role: "member" }, { email: "zed@x.test", role: "admin" }, { email: "ada@x.test", role: "owner" }, { email: "bo@x.test", role: "member" }] };
  const f = limitFixers({ org: acme, members, me: "me@x.test" });
  assert.equal(f.canFix, false);
  assert.deepEqual(f.people.map((p) => p.email), ["ada@x.test", "zed@x.test"], "owners first, then admins; members left out");
  assert.equal(f.text, "Only owners and admins change the plan. Ask one of ada@x.test (owner), zed@x.test (admin) to make room or upgrade Acme.");
  assert.equal(limitFixers({ org: acme, members: null }).text, "Only owners and admins change the plan. Ask an owner of Acme to make room or upgrade.", "members unreadable: the org by name");
  assert.equal(limitFixers({ org: null }).text, "Only owners and admins change the plan. Ask an owner of the organization to make room or upgrade.", "an org that isn't mine: never its id");
  assert.equal(limitFixers({ org: { ...acme, role: "admin" } }).canFix, true);
  assert.equal(limitFixers({ org: { personal: true, role: "owner" } }).text, "It's your personal account's plan: you can change it on Plan & billing.");
}

// U19: secrets a request uses with no value on this device.
{
  const { usedVariables, missingSecrets, missingSecretsText } = await import("../src/features.js");
  const used = usedVariables({
    url: "${baseUrl}/orders/${id}?k=${secret:vault:kv/a#b}",
    headers: [{ name: "Authorization", value: "Bearer ${TOKEN}" }, { name: "X-Off", value: "${OFF}", enabled: false }],
    body: '{"k": "${apiKey}"}',
    auth: { type: "basic", username: "${user}", password: "${PASS}" },
    preRequestScript: "pm.environment.get('${inScript}')",
    formFields: [{ key: "f", value: "${field}" }, { key: "up", type: "file", value: "${file}" }],
  });
  assert.deepEqual(used, ["baseUrl", "id", "TOKEN", "apiKey", "user", "PASS", "field"], "enabled headers, body, auth, form; no scripts, refs, disabled headers or files");
  const missing = missingSecrets(used, { baseUrl: "x", TOKEN: "", PASS: "set", apiKey: "k" }, ["TOKEN", "apiKey", "PASS"], ["user"]);
  assert.deepEqual(missing, [{ name: "TOKEN", where: "environment" }, { name: "user", where: "globals" }], "empty and absent count, filled ones don't");
  assert.deepEqual(missingSecrets(["TOKEN"], { TOKEN: "from a script" }, ["TOKEN"]), [], "a value from anywhere fills it");
  assert.deepEqual(missingSecrets(["plain"], {}, ["TOKEN"]), [], "only secrets are reported");
  assert.equal(missingSecretsText([{ name: "TOKEN", where: "environment" }], "Staging"), "Secret TOKEN has no value on this device. Fill it in under Environments → Staging → Secrets: secret values stay on each device and never sync.");
  assert.match(missingSecretsText(missing, "Staging"), /^Secrets TOKEN and user have no value .* Environments → Staging → Secrets and Environments → Globals → Secret globals:/);
  assert.equal(missingSecretsText([], "x"), "");
}

console.log("unit-ux: all checks passed");
