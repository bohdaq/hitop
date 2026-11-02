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

console.log("unit-ux: all checks passed");
