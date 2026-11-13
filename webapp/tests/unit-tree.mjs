// DD1 tree moves (no browser): node tests/unit-tree.mjs
import assert from "node:assert/strict";
import { requestTree } from "../src/collection-tree.js";
import { addFolder, allFolders, deleteFolder, endOf, freeFolderPath, moveFolder, moveRequest, renameStored, storedFolders, treeOf } from "../src/tree-moves.js";

let n = 100;
const newId = () => ++n;
const req = (id, file) => (file ? { id, name: `r${id}`, file } : { id, name: `r${id}` });
const col = (id, requests, folders) => ({ id, name: `c${id}`, requests, ...(folders ? { folders } : {}) });
const files = (c) => c.requests.map((r) => `${r.id}:${r.file ?? ""}`).join(" ");
const shape = (c) => {
  const out = [];
  const walk = (node, depth) => {
    for (const it of node.items) {
      if (it.folder) {
        out.push(`${" ".repeat(depth)}[${it.folder.name}]`);
        walk(it.folder, depth + 1);
      } else out.push(`${" ".repeat(depth)}${it.request.id}`);
    }
  };
  walk(treeOf(c, requestTree), 0);
  return out.join("|");
};

// --- empty folders persist and are placed ---
{
  const c = col(1, [req(1), req(2, "a/x")], ["asd"]);
  assert.equal(shape(c), "1|[a]| 2|[asd]", "an empty stored folder sits at the end of its parent");
  const c2 = col(1, [req(1), req(2, "a/x")], ["e", "a"]);
  assert.equal(shape(c2), "1|[e]|[a]| 2", "…or right before the next stored sibling that has requests");
  assert.equal(shape(col(1, [], ["p/q"])), "[p]| [q]", "a nested empty folder brings its parent");
  assert.deepEqual([...allFolders(c)].sort(), ["a", "asd"]);
  assert.equal(freeFolderPath(c, "", "ASD"), "ASD 2", "names clash ignoring case");
  const { collection, path } = addFolder(col(1, []), "", "new");
  assert.equal(path, "new");
  assert.deepEqual(collection.folders, ["new"]);
}

// --- requests: into a folder (also an empty one), between rows, to another collection, copy ---
{
  const c = col(1, [req(1), req(2, "a/x"), req(3, "a/y"), req(4)], ["asd"]);
  let r = moveRequest([c], { from: 1, requestId: 1, to: 1, folder: "asd" });
  assert.equal(files(r.collections[0]), "2:a/x 3:a/y 4: 1:asd/r1", "into an empty folder: at its place (the end of the parent)");
  r = moveRequest([c], { from: 1, requestId: 4, to: 1, folder: "a" });
  assert.equal(files(r.collections[0]), "1: 2:a/x 3:a/y 4:a/r4", "dropped on a folder: its end, right after its last request");
  r = moveRequest([c], { from: 1, requestId: 4, to: 1, folder: "a", before: { request: 3 } });
  assert.equal(files(r.collections[0]), "1: 2:a/x 4:a/r4 3:a/y", "between two rows");
  r = moveRequest([c], { from: 1, requestId: 2, to: 1, folder: "" });
  assert.equal(files(r.collections[0]), "1: 3:a/y 4: 2:", "out to the top, its file dropped");
  r = moveRequest([c], { from: 1, requestId: 2, to: 1, folder: "a", before: { request: 2 } });
  assert.equal(r.error, null, "onto itself: nothing to do");
  // The folder it leaves stays, even empty.
  const one = col(1, [req(1, "solo/x")]);
  r = moveRequest([one], { from: 1, requestId: 1, to: 1, folder: "" });
  assert.deepEqual(r.collections[0].folders, ["solo"]);
  assert.equal(shape(r.collections[0]), "1|[solo]");
  // Another collection, and a copy.
  const other = col(2, [req(9, "a/x")]);
  r = moveRequest([c, other], { from: 1, requestId: 2, to: 2, folder: "a" });
  assert.equal(files(r.collections[1]), "9:a/x 2:a/r2", "named from the request, free in the target");
  assert.ok(!r.collections[0].requests.some((x) => x.id === 2));
  r = moveRequest([c, other], { from: 1, requestId: 2, to: 2, folder: "", copy: true, newId });
  assert.equal(r.collections[0].requests.length, 4, "a copy leaves the original");
  assert.equal(r.request.id, 101);
  assert.equal(r.request.name, "r2");
}

// --- folders: reorder, nest, out, to another collection, copy, refusals ---
{
  const c = col(1, [req(1), req(2, "a/x"), req(3, "a/b/y"), req(4, "z/w"), req(5)], ["e"]);
  let r = moveFolder([c], { from: 1, path: "z", to: 1, parent: "", before: { folder: "a" } });
  assert.equal(files(r.collections[0]), "1: 4:z/w 2:a/x 3:a/b/y 5:", "a folder before another: its block moves");
  r = moveFolder([c], { from: 1, path: "z", to: 1, parent: "a" });
  assert.equal(r.path, "a/z");
  assert.equal(files(r.collections[0]), "1: 2:a/x 3:a/b/y 4:a/z/w 5:", "nested: at the end of the folder");
  r = moveFolder([c], { from: 1, path: "a/b", to: 1, parent: "" });
  assert.equal(r.path, "b");
  assert.equal(files(r.collections[0]), "1: 2:a/x 4:z/w 5: 3:b/y", "out to the top");
  assert.equal(moveFolder([c], { from: 1, path: "a", to: 1, parent: "a/b" }).error, "A folder can't go inside itself.");
  assert.equal(moveFolder([c], { from: 1, path: "a", to: 1, parent: "a" }).error, "A folder can't go inside itself.");
  // A clash at the destination: "name 2".
  const d = col(1, [req(1, "a/x"), req(2, "q/a/y")]);
  r = moveFolder([d], { from: 1, path: "a", to: 1, parent: "q" });
  assert.equal(r.path, "q/a 2");
  // Empty folders move too, and keep their stored place.
  r = moveFolder([c], { from: 1, path: "e", to: 1, parent: "", before: { folder: "a" } });
  assert.deepEqual(storedFolders(r.collections[0]), ["e", "a"]);
  assert.equal(shape(r.collections[0]), "1|[e]|[a]| 2| [b]|  3|[z]| 4|5");
  // To another collection (stored subfolders follow), and a copy.
  const withSub = col(1, [req(1, "a/x")], ["a/empty"]);
  const other = col(2, [req(9)]);
  r = moveFolder([withSub, other], { from: 1, path: "a", to: 2, parent: "" });
  assert.equal(files(r.collections[1]), "9: 1:a/x");
  assert.deepEqual(storedFolders(r.collections[1]), ["a", "a/empty"]);
  assert.deepEqual(storedFolders(r.collections[0]), []);
  assert.equal(r.collections[0].requests.length, 0);
  r = moveFolder([withSub, other], { from: 1, path: "a", to: 1, parent: "", copy: true, newId });
  assert.equal(r.path, "a 2");
  assert.equal(files(r.collections[0]), `1:a/x ${n}:a 2/x`);
}

// --- delete, rename, endOf ---
{
  const c = col(1, [req(1), req(2, "a/x"), req(3, "a/b/y")], ["a/b/c", "keep"]);
  const { collection, removed } = deleteFolder(c, "a");
  assert.deepEqual(removed, [2, 3]);
  assert.deepEqual(collection.folders, ["keep"]);
  assert.deepEqual(renameStored(c, "a", "q").folders, ["q/b/c", "keep"]);
  assert.equal(endOf(c.requests, "a/b"), 3);
  assert.equal(endOf(c.requests, "nope"), 3, "an empty folder's end is its parent's end");
  assert.equal(endOf([req(1, "a/x"), req(2)], "a"), 1);
}
console.log("unit-tree: ok");

// --- after a row, and Alt+Up/Down neighbours ---
{
  const { neighbourPlace, nodeAt } = await import("../src/tree-moves.js");
  const c = col(1, [req(1), req(2, "a/x"), req(3, "a/y"), req(4)]);
  let r = moveRequest([c], { from: 1, requestId: 1, to: 1, folder: "a", after: { request: 2 } });
  assert.equal(files(r.collections[0]), "2:a/x 1:a/r1 3:a/y 4:");
  r = moveRequest([c], { from: 1, requestId: 4, to: 1, folder: "", after: { folder: "a" } });
  assert.equal(files(r.collections[0]), "1: 2:a/x 3:a/y 4:", "after a folder's block (already there)");
  r = moveRequest([c], { from: 1, requestId: 1, to: 1, folder: "", after: { folder: "a" } });
  assert.equal(files(r.collections[0]), "2:a/x 3:a/y 1: 4:");
  const tree = treeOf(c, requestTree);
  assert.equal(nodeAt(tree, "a").requests.length, 2);
  assert.deepEqual(neighbourPlace(tree, "", { request: 4 }, -1), { folder: "", before: { folder: "a" } });
  assert.deepEqual(neighbourPlace(tree, "", { folder: "a" }, -1), { folder: "", before: { request: 1 } });
  assert.deepEqual(neighbourPlace(tree, "a", { request: 2 }, 1), { folder: "a", after: { request: 3 } });
  assert.equal(neighbourPlace(tree, "", { request: 1 }, -1), null, "the top edge");
  // A folder moved down past a request: its block goes after it.
  r = moveFolder([c], { from: 1, path: "a", to: 1, parent: "", ...neighbourPlace(tree, "", { folder: "a" }, 1) });
  assert.equal(files(r.collections[0]), "1: 4: 2:a/x 3:a/y");
  console.log("unit-tree: after + neighbours ok");
}
