// Unit checks for the 1.2 UX helpers (no browser): node tests/unit-ux.mjs
import assert from "node:assert/strict";
import { addCollections, folderOf, requestTree } from "../src/collection-tree.js";

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

console.log("unit-ux: all checks passed");
