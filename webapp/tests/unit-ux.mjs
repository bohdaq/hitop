// Unit checks for the 1.2 UX helpers (no browser): node tests/unit-ux.mjs
import assert from "node:assert/strict";
import { addCollections, folderOf, requestTree } from "../src/collection-tree.js";
import { clampSplit, layoutFor, splitAt, splitForKey } from "../src/split.js";
import { fuzzyMatch, quickOpenItems, rankItems } from "../src/quick-open.js";

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
  assert.equal(runCommand({ collection: "Shop API", folderLabel: "shop-api", environment: "Staging" }), "hitop run ./shop-api --env Staging --reporter cli,junit,json --out reports");
  assert.equal(runCommand({ collection: "Shop API", environment: null }), "hitop run 'Shop API' --reporter cli,junit,json --out reports");
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

console.log("unit-ux: all checks passed");
