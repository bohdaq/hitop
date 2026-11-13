// DD1: moving requests and folders in the sidebar tree (pure, no storage or UI).
//
// A collection's order is its `requests` order: that is what Run all and `hitop run` follow, so
// what the tree shows is what runs. A folder is the requests whose `file` lies under its path and
// sits where its first request is; moving a folder moves that block. `folders` (a collection key,
// `hitop.yaml` `folders`) lists folder paths that exist even with no request in them, in the order
// they were made; a folder only goes away when it is deleted, never because its last request left.
//
// A place in the tree is { collectionId, folder, before }: the folder ("" = the collection's top)
// and what to go before: { request: id }, { folder: path } or null for the end of `folder`.

import { folderOf, requestFile } from "./collection-tree.js";

const parentOf = (path) => {
  const at = path.lastIndexOf("/");
  return at === -1 ? "" : path.slice(0, at);
};
const baseOf = (path) => path.slice(path.lastIndexOf("/") + 1);
/** Is `folder` the folder `path` or under it? ("" holds everything.) */
export const within = (folder, path) => path === "" || folder === path || folder.startsWith(`${path}/`);

/** A collection's stored folder paths (`folders`), cleaned up. */
export function storedFolders(c) {
  return Array.isArray(c?.folders) ? [...new Set(c.folders.filter((p) => typeof p === "string").map(clean).filter(Boolean))] : [];
}

const clean = (p) =>
  String(p)
    .replace(/\\/g, "/")
    .split("/")
    .map((s) => s.trim())
    .filter(Boolean)
    .join("/");

/** Every folder path of a collection (requests' folders, stored ones, and their parents). */
export function allFolders(c) {
  const out = new Set();
  const add = (p) => {
    for (let q = p; q; q = parentOf(q)) out.add(q);
  };
  for (const r of c?.requests || []) add(folderOf(r));
  for (const p of storedFolders(c)) add(p);
  return out;
}

/** Is there a folder `path` in `c`, ignoring case (a folder collection's disk may not tell them apart)? */
export function hasFolder(c, path) {
  const want = path.toLowerCase();
  return [...allFolders(c)].some((p) => p.toLowerCase() === want);
}

/** A free folder path under `parent` for `name` in `c`: "name", else "name 2", "name 3"... */
export function freeFolderPath(c, parent, name, except = null) {
  const base = clean(name).replace(/\//g, "-") || "Folder";
  const at = (n) => (parent ? `${parent}/` : "") + (n === 1 ? base : `${base} ${n}`);
  for (let n = 1; ; n++) {
    const p = at(n);
    if (p === except || !hasFolder(c, p)) return p;
  }
}

/** Where in `requests` something placed at the end of folder `path` goes (an index to insert at). */
export function endOf(requests, path) {
  for (let p = path; ; p = parentOf(p)) {
    let last = -1;
    requests.forEach((r, i) => {
      if (within(folderOf(r), p)) last = i;
    });
    if (last !== -1) return last + 1;
    if (p === "") return requests.length;
  }
}

/** The index in `requests` for a place within `folder`: `after` ({ request } | { folder }), else `before`. */
export function indexFor(requests, folder, before, after = null) {
  if (after?.request != null) {
    const at = requests.findIndex((r) => String(r.id) === String(after.request));
    if (at !== -1) return at + 1;
  }
  if (after?.folder) {
    let last = -1;
    requests.forEach((r, i) => {
      if (within(folderOf(r), after.folder)) last = i;
    });
    if (last !== -1) return last + 1;
  }
  if (before?.request != null) {
    const at = requests.findIndex((r) => String(r.id) === String(before.request));
    if (at !== -1) return at;
  }
  if (before?.folder) {
    const at = requests.findIndex((r) => within(folderOf(r), before.folder));
    if (at !== -1) return at;
  }
  return endOf(requests, folder);
}

// `folders` of a collection with `paths` added (before `beforePath` when it is there) and
// `removed` (paths) taken out.
function withStored(c, { add = [], remove = (_p) => false, beforePath = null, afterPath = null } = {}) {
  const adding = add.filter(Boolean);
  // Paths being (re)placed leave their old place first.
  const list = storedFolders(c).filter((p) => !remove(p) && !(beforePath && adding.includes(p)));
  const fresh = adding.filter((p) => !list.includes(p));
  if (afterPath && !adding.includes(afterPath)) {
    if (!list.includes(afterPath)) list.push(afterPath);
    const at = list.indexOf(afterPath) + 1;
    return [...list.slice(0, at), ...fresh, ...list.slice(at)];
  }
  if (!beforePath || adding.includes(beforePath)) return [...list, ...fresh];
  // The folder to go before joins the list, so the order is kept.
  if (!list.includes(beforePath)) list.push(beforePath);
  const at = list.indexOf(beforePath);
  return [...list.slice(0, at), ...fresh, ...list.slice(at)];
}

const setFolders = (c, folders) => {
  const { folders: _old, ...rest } = c;
  return folders.length ? { ...rest, folders } : rest;
};

/**
 * A request moved (or copied) to a place. `copy` gives the new request `newId()`. Returns
 * { collections, request } (the request as it is now), or { error } when nothing can change.
 * The folder it leaves stays (stored), even when it is now empty.
 */
export function moveRequest(collections, { from, requestId, to, folder = "", before = null, after = null, copy = false, newId = null }) {
  const source = collections.find((c) => c.id === from);
  const target = collections.find((c) => c.id === to);
  const request = source?.requests?.find((r) => String(r.id) === String(requestId));
  if (!request || !target) return { error: "That request or place is gone." };
  if (!copy && [before, after].some((x) => x?.request != null && String(x.request) === String(request.id))) return { error: null };
  const left = folderOf(request);
  const same = from === to;
  const sameFolder = same && left === folder && !copy;
  const pool = (same ? source.requests : [...source.requests, ...target.requests]).filter((r) => copy || r.id !== request.id);
  const { file: _file, id: _id, ...rest } = request;
  const base = copy ? { ...rest, id: newId() } : { ...rest, id: request.id };
  const moved = sameFolder ? request : folder ? { ...base, file: requestFile(pool, folder, request.name) } : base;
  const next = collections.map((c) => {
    let requests = c.requests || [];
    let out = c;
    if (c.id === from && !copy) {
      requests = requests.filter((r) => r.id !== request.id);
      // The folder it leaves keeps existing.
      out = left ? setFolders({ ...out, requests }, withStored(out, { add: [left] })) : { ...out, requests };
    }
    if (c.id === to) {
      const at = indexFor(requests, folder, before, after);
      requests = [...requests.slice(0, at), moved, ...requests.slice(at)];
      out = { ...out, requests };
    }
    return out;
  });
  return { collections: next, request: moved };
}

/**
 * The folder `path` of `from` moved (or copied) into `parent` of `to`, before `before`. Its name
 * stays unless the place already has one like it ("name 2"). Returns { collections, path } (its
 * new path) or { error }.
 */
export function moveFolder(collections, { from, path, to, parent = "", before = null, after = null, copy = false, newId = null }) {
  const source = collections.find((c) => c.id === from);
  const target = collections.find((c) => c.id === to);
  if (!source || !target || !path) return { error: "That folder or place is gone." };
  if (!allFolders(source).has(path)) return { error: "That folder is gone." };
  const same = from === to;
  if (same && within(parent, path)) return { error: "A folder can't go inside itself." };
  if (same && [before, after].some((x) => x?.folder && within(x.folder, path))) return { error: null };
  const keep = same && !copy && parentOf(path) === parent;
  const newPath = keep ? path : freeFolderPath(target, parent, baseOf(path), same && !copy ? path : null);
  const re = (p) => `${newPath}${p.slice(path.length)}`;
  const block = (source.requests || []).filter((r) => within(folderOf(r), path));
  const ids = new Set(block.map((r) => r.id));
  const moved = block.map((r) => {
    const file = String(r.file).replace(/\\/g, "/").replace(/^\/+/, "");
    const out = { ...r, file: `${newPath}${file.slice(path.length)}` };
    return copy ? { ...out, id: newId() } : out;
  });
  const subtree = [...allFolders(source)].filter((p) => within(p, path)).sort();
  const next = collections.map((c) => {
    let out = c;
    let requests = c.requests || [];
    if (c.id === from && !copy) {
      requests = requests.filter((r) => !ids.has(r.id));
      out = setFolders({ ...out, requests }, withStored(out, { remove: (p) => within(p, path) }));
    }
    if (c.id === to) {
      const at = indexFor(requests, parent, before, after);
      requests = [...requests.slice(0, at), ...moved, ...requests.slice(at)];
      out = setFolders({ ...out, requests }, withStored(out, { add: subtree.map(re), beforePath: before?.folder ?? null, afterPath: after?.folder ?? null }));
    }
    return out;
  });
  return { collections: next, path: newPath };
}

/** A new (empty) folder `name` under `parent` of `c`: { collection, path }. */
export function addFolder(c, parent, name) {
  const path = freeFolderPath(c, parent, name);
  return { collection: setFolders(c, withStored(c, { add: [path] })), path };
}

/** The folder `path` deleted with its requests and subfolders: { collection, removed: [request ids] }. */
export function deleteFolder(c, path) {
  const removed = (c.requests || []).filter((r) => within(folderOf(r), path)).map((r) => r.id);
  const requests = (c.requests || []).filter((r) => !within(folderOf(r), path));
  return { collection: setFolders({ ...c, requests }, withStored(c, { remove: (p) => within(p, path) })), removed };
}

/** Stored folder paths after the folder `path` was renamed to `newPath`. */
export function renameStored(c, path, newPath) {
  const list = storedFolders(c).map((p) => (within(p, path) ? `${newPath}${p.slice(path.length)}` : p));
  return setFolders(c, [...new Set(list)]);
}

/**
 * A collection's tree (collection-tree.js `requestTree`) with its stored folders in it. A folder
 * with no request in its subtree sits right before the next stored folder of the same parent that
 * has requests (the stored order), else at the end of its parent.
 */
export function treeOf(c, requestTree) {
  const tree = requestTree(c?.requests || []);
  const stored = storedFolders(c);
  const byPath = new Map([["", tree]]);
  const walk = (node) => {
    for (const f of node.folders) {
      byPath.set(f.path, f);
      walk(f);
    }
  };
  walk(tree);
  const make = (path) => ({ name: baseOf(path), path, requests: [], folders: [], items: [] });
  const missing = [];
  for (const p of stored) for (let q = p; q; q = parentOf(q)) if (!byPath.has(q) && !missing.includes(q)) missing.push(q);
  // Parents first, so a nested empty folder finds its parent node.
  missing.sort((a, b) => a.split("/").length - b.split("/").length);
  for (const path of missing) {
    const node = make(path);
    const parent = byPath.get(parentOf(path));
    byPath.set(path, node);
    // The next stored sibling (after this one in `folders`) that is already placed.
    const siblings = stored.filter((p) => parentOf(p) === parentOf(path));
    const after = siblings.slice(siblings.indexOf(path) + 1).map((p) => byPath.get(p)).find((n) => n && n !== node && parent.folders.includes(n));
    const at = after ? parent.items.findIndex((x) => x.folder === after) : -1;
    if (at === -1) parent.items.push({ folder: node });
    else parent.items.splice(at, 0, { folder: node });
    parent.folders.push(node);
  }
  return tree;
}

/** The node of `tree` for folder `path` ("" = the top), or null. */
export function nodeAt(tree, path) {
  if (!path) return tree;
  for (const f of tree.folders) {
    if (f.path === path) return f;
    if (path.startsWith(`${f.path}/`)) return nodeAt(f, path);
  }
  return null;
}

/**
 * Alt+Up/Down: where a request ({ request: id }) or folder ({ folder: path }) goes to swap with its
 * neighbour in the same folder: { folder, before } or { folder, after }, or null at the edge.
 */
export function neighbourPlace(tree, folder, node, dir) {
  const parent = nodeAt(tree, folder);
  if (!parent) return null;
  const at = parent.items.findIndex((x) => (node.request != null ? String(x.request?.id) === String(node.request) : x.folder?.path === node.folder));
  const other = parent.items[at + dir];
  if (at === -1 || !other) return null;
  const ref = other.folder ? { folder: other.folder.path } : { request: other.request.id };
  return dir < 0 ? { folder, before: ref } : { folder, after: ref };
}
