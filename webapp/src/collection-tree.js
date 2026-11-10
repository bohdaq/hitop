// Pure helpers over collections (no storage, no UI): adding imported
// collections next to the ones there are, and a collection's requests as
// folder groups (the folder path each request keeps in `file`, as Postman
// folders, OpenAPI tags and collection folders give it).

// Imported collections as they join `existing`: an id already in use gets a
// new one (`newId()`), a name already in use gets " (2)", " (3)"... Returns
// { added, renamed: [[from, to]] }.
export function addCollections(existing, incoming, newId) {
  const ids = new Set(existing.map((c) => String(c.id)));
  const names = new Set(existing.map((c) => c.name));
  const renamed = [];
  const added = incoming.map((c) => {
    let id = c.id;
    if (id == null || ids.has(String(id))) id = newId();
    ids.add(String(id));
    const base = String(c.name || "Imported").trim() || "Imported";
    let name = base;
    for (let n = 2; names.has(name); n++) name = `${base} (${n})`;
    names.add(name);
    if (name !== base) renamed.push([base, name]);
    return { ...c, id, name };
  });
  return { added, renamed };
}

// The folder a request is in ("account/admin" for file "account/admin/me"),
// or "" at the top.
export const folderOf = (request) => {
  const file = typeof request?.file === "string" ? request.file.replace(/\\/g, "/").replace(/^\/+|\/+$/g, "") : "";
  const at = file.lastIndexOf("/");
  return at === -1 ? "" : file.slice(0, at);
};

// A collection's requests as a tree, in their order: { requests, folders:
// [{ name, path, requests, folders }] }. A folder sits where its first
// request is. `index` on each entry is the request's place in `requests`.
export function requestTree(requests) {
  const root = { name: "", path: "", requests: [], folders: [], items: [] };
  const byPath = new Map([["", root]]);
  const folder = (path) => {
    if (byPath.has(path)) return byPath.get(path);
    const at = path.lastIndexOf("/");
    const parent = folder(at === -1 ? "" : path.slice(0, at));
    const node = { name: at === -1 ? path : path.slice(at + 1), path, requests: [], folders: [], items: [] };
    byPath.set(path, node);
    parent.folders.push(node);
    parent.items.push({ folder: node });
    return node;
  };
  requests.forEach((request, index) => {
    const node = folder(folderOf(request));
    const entry = { request, index };
    node.requests.push(entry);
    node.items.push(entry);
  });
  return root;
}

// A file name from a request name, as hitop-core's `names::slug` makes it:
// lower case, letters and digits kept, every other run of characters one "-",
// at most 80 characters, "request" when nothing is left.
export function slugName(name) {
  let out = "";
  let dash = false;
  for (const c of String(name ?? "").toLowerCase()) {
    if (/[\p{L}\p{N}]/u.test(c)) {
      if (dash && out) out += "-";
      dash = false;
      out += c;
    } else dash = true;
    if (out.length >= 80) break;
  }
  if (!out) return "request";
  return /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/.test(out) ? `${out}-file` : out;
}

// The `file` for a request named `name` in `folder` ("" = the top) of a
// collection whose requests are `requests`: unique among the files there.
export function requestFile(requests, folder, name) {
  const taken = new Set(requests.map((r) => r.file).filter(Boolean));
  const base = folder ? `${folder}/${slugName(name)}` : slugName(name);
  let file = base;
  for (let n = 2; taken.has(file); n++) file = `${base}-${n}`;
  return file;
}

// `tree` (requestTree) with these folder paths present too: folders made in
// the sidebar hold no request yet, so they exist only on screen until one is
// saved into them. Mutates and returns `tree`.
export function withFolders(tree, paths = []) {
  const byPath = new Map([["", tree]]);
  const walk = (node) => {
    for (const f of node.folders) {
      byPath.set(f.path, f);
      walk(f);
    }
  };
  walk(tree);
  const folder = (path) => {
    if (byPath.has(path)) return byPath.get(path);
    const at = path.lastIndexOf("/");
    const parent = folder(at === -1 ? "" : path.slice(0, at));
    const node = { name: at === -1 ? path : path.slice(at + 1), path, requests: [], folders: [], items: [] };
    byPath.set(path, node);
    parent.folders.push(node);
    parent.items.push({ folder: node });
    return node;
  };
  for (const p of paths) if (p) folder(p);
  return tree;
}

// Every folder path in a tree, parents before children.
export function folderPaths(tree) {
  const out = [];
  const walk = (node) => {
    for (const f of node.folders) {
      out.push(f.path);
      walk(f);
    }
  };
  walk(tree);
  return out;
}

// A request moved to a collection and folder: `collections` with the request
// removed from `from` and put in `to` (in `folder`, "" = the top) before the
// request `beforeId` or at the end. Its name and every field the editor
// doesn't know stay; `file` is the destination's to give (a folder path, or
// none at the top: a folder collection names the file from the request).
// A move within one folder keeps `file`.
export function moveRequestTo(collections, { from, requestId, to, folder = "", beforeId = null }) {
  const source = collections.find((c) => c.id === from);
  const target = collections.find((c) => c.id === to);
  const request = source?.requests?.find((r) => r.id === requestId);
  if (!request || !target) return collections;
  const sameFolder = from === to && folderOf(request) === folder;
  const { file: _file, ...rest } = request;
  const others = (from === to ? source.requests : [...source.requests, ...target.requests]).filter((r) => r.id !== requestId);
  const moved = sameFolder ? request : folder ? { ...rest, file: requestFile(others, folder, request.name) } : rest;
  return collections.map((c) => {
    let requests = c.id === from ? c.requests.filter((r) => r.id !== requestId) : c.requests;
    if (c.id === to) {
      const at = beforeId == null ? -1 : requests.findIndex((r) => r.id === beforeId);
      requests = at === -1 ? [...requests, moved] : [...requests.slice(0, at), moved, ...requests.slice(at)];
    }
    return requests === c.requests ? c : { ...c, requests };
  });
}

// `requests` with the folder `path` (and its subfolders) renamed to `newPath`.
export function renameFolderIn(requests, path, newPath) {
  return requests.map((r) => {
    const folder = folderOf(r);
    if (folder !== path && !folder.startsWith(`${path}/`)) return r;
    const file = String(r.file).replace(/\\/g, "/").replace(/^\/+/, "");
    return { ...r, file: `${newPath}${file.slice(path.length)}` };
  });
}
