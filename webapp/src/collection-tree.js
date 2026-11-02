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
