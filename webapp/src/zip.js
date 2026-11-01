// Collection folders as .zip files: export writes `<name>/hitop.yaml`,
// `requests/` and `environments/` (YAML from hitop-core through
// hitop-wasm); import finds the hitop.yaml closest to the top and reads the
// folder around it, so zips of the folder itself or of its contents both work.
import { strFromU8, strToU8, unzipSync, zipSync } from "fflate";
import { loadFormat } from "./wasm/format.js";

const slug = (name) => name.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-+|-+$/g, "") || "collection";

// `folder` is { collection, environments }; secret values must already be
// gone (the Rust writer drops them too).
export async function folderToZip(folder) {
  const format = await loadFormat();
  const files = format.toFiles(folder);
  const dir = slug(folder.collection.name);
  const entries = {};
  // A fixed date keeps the bytes the same for the same folder.
  for (const [path, text] of Object.entries(files)) entries[`${dir}/${path}`] = [strToU8(text), { mtime: new Date("2026-01-01T00:00:00Z") }];
  return { name: `${dir}.zip`, bytes: zipSync(entries, { level: 6 }) };
}

// The folder's files from zip bytes, and the folder's name.
export function zipToFiles(bytes, zipName = "collection.zip") {
  let entries;
  try {
    entries = unzipSync(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes));
  } catch {
    throw new Error("That file isn't a zip archive HITOP can read.");
  }
  const paths = Object.keys(entries).filter((p) => !p.endsWith("/") && !p.split("/").some((seg) => seg.startsWith(".") || seg === "__MACOSX"));
  const manifests = paths.filter((p) => p === "hitop.yaml" || p.endsWith("/hitop.yaml")).sort((a, b) => a.split("/").length - b.split("/").length);
  if (!manifests.length) throw new Error("There's no hitop.yaml in that zip, so it isn't a collection folder.");
  const prefix = manifests[0].slice(0, -"hitop.yaml".length);
  const files = {};
  for (const p of paths) if (p.startsWith(prefix)) files[p.slice(prefix.length)] = strFromU8(entries[p]);
  const name = prefix ? prefix.slice(0, -1).split("/").pop() : zipName.replace(/\.zip$/i, "");
  return { files, name };
}

export async function zipToFolder(bytes, zipName) {
  const { files, name } = zipToFiles(bytes, zipName);
  const format = await loadFormat();
  return format.readFiles(files, name);
}
