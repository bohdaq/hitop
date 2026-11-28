// The store's import, export, history (a part of Store, src/store.js).
import collectionService from "../../../shared/services/collectionService.js";
import { addCollections } from "../collection-tree.js";
import { numericId, pmShim, tabForRequest } from "./shared.js";
import { parseEnvironments } from "../features.js";
import { forWebApp, isPostmanCollection, scriptScans } from "../pm.js";
import { generateUniqueId } from "../../../shared/services/idGenerator.js";
import { loadFormat } from "../wasm/format.js";
import { markImported } from "../getting-started.js";
import { platform } from "../platform/index.js";
import { callHook } from "../plugin-hooks.js";

export const ImportExportPart = (Base) =>
  class extends Base {
  // `only`: one collection's id (UX2: the Export dialog's picker); empty header rows are left out.
  exportJson(format, only = null) {
    const picked = only == null ? this.collections : this.collections.filter((c) => String(c.id) === String(only));
    const clean = picked.map((c) => ({ ...c, requests: (c.requests || []).map((r) => ({ ...r, headers: (r.headers || []).filter((h) => h.name || h.value) })) }));
    return collectionService.exportCollections(clean, format);
  }

  // Returns the import report for a Postman collection (what hitop couldn't
  // carry over, unsupported pm.* calls by script line), else null. Imported
  // collections are added to the ones there are (a name in use gets " (2)"),
  // or with `replace` take their place.
  async importJson(json, { replace = false } = {}) {
    let imported;
    let report = null;
    const converted = (await loadFormat()).convertImport(json);
    if (converted) return this.importConverted(converted, { replace });
    const parsed = JSON.parse(json.replace(/^\uFEFF/, "")); // throws on bad JSON
    if (isPostmanCollection(parsed)) {
      // hitop-core's importer (hitop-wasm), as `hitop import` runs it.
      const out = (await loadFormat()).importPostman(json, scriptScans(pmShim(), parsed));
      imported = [forWebApp(out.collection)];
      report = { name: out.collection.name, issues: out.report.issues || [] };
    } else {
      imported = collectionService.importCollections(json);
    }
    return this.addImported(imported, report, replace);
  }

  // An importer plugin's collections (decision 10), read as a HITOP JSON file and always added
  // (never Replace). hitop-core checked them and dropped `workspace` and `org`; where a collection is
  // kept is decided here as for any import: new ids, no `sharedIn`, the current organization.
  async importWithPlugin(name, text, fileName = "") {
    const collections = await callHook(name, "importer", [String(text ?? ""), String(fileName ?? "")]);
    // hitop-core read them as HITOP JSON already: only that branch of importCollections (its format
    // guessing by keys like `info` or `items` would let an answer pick another reader).
    const own = collections.map(({ id: _id, sharedIn: _shared, org: _org, workspace: _ws, ...c }) => ({ ...c, id: generateUniqueId(), requests: c.requests || [], variables: c.variables || {} }));
    return this.addImported(own, null, false);
  }

  // Insomnia, Bruno and HAR (hitop-core convert.rs): Postman's shapes, through the same importer.
  async importConverted(converted, { replace = false } = {}) {
    const format = await loadFormat();
    const issues = [...converted.issues];
    const imported = [];
    for (const c of converted.collections) {
      const out = format.importPostman(JSON.stringify(c), scriptScans(pmShim(), c));
      imported.push(forWebApp(out.collection));
      issues.push(...(out.report.issues || []));
    }
    const environments = converted.environments.flatMap((e) => parseEnvironments(JSON.stringify(e)));
    if (environments.length) {
      const names = new Set(this.environments.map((e) => e.name));
      for (const e of environments) {
        const base = e.name;
        for (let n = 2; names.has(e.name); n++) e.name = `${base} (${n})`;
        names.add(e.name);
      }
      this.environments = [...this.environments, ...environments];
    }
    const name = imported.map((c) => c.name).join(", ") || environments.map((e) => e.name).join(", ");
    const report = { name, issues };
    if (imported.length) return this.addImported(imported, report, replace);
    this.emit();
    this.notify(`Imported ${environments.length} environment${environments.length === 1 ? "" : "s"}`);
    return report;
  }

  addImported(imported, report, replace) {
    // The desktop keeps ids as numbers (Rust's model); give string ids new ones.
    if (platform().name === "tauri") imported = imported.map((c) => ({ ...c, id: numericId(c.id), requests: (c.requests || []).map((r) => ({ ...r, id: numericId(r.id) })) }));
    if (!replace) {
      const { added, renamed } = addCollections(this.collections, imported, generateUniqueId);
      this.collections = [...this.collections, ...added.map((c) => this.inOrg(c))];
      markImported(globalThis.localStorage, added.map((c) => c.id));
      const first = added.find((c) => c.requests?.length);
      if (first) this.openRequest(first.requests[0], first.id);
      else this.show("collections");
      const names = added.map((c) => c.name).join(", ");
      this.notify(`Added ${added.length} collection${added.length === 1 ? "" : "s"}: ${names}${renamed.length ? ` (renamed: you already have ${renamed.map(([from]) => from).join(", ")})` : ""}`);
      return report;
    }
    this.closeAllFolders();
    this.collections = imported;
    markImported(globalThis.localStorage, imported.map((c) => c.id));
    const first = imported.find((c) => c.requests?.length);
    this.tabs = first ? [tabForRequest(first.requests[0], first.id)] : [];
    this.currentTab = 0;
    this.view = { kind: "request", collectionId: null };
    // Nothing to open: a new request (in My requests, made when the import has none).
    if (!first) this.newTab(this.defaultCollection());
    this.emit();
    this.notify(`Imported ${imported.length} collection${imported.length === 1 ? "" : "s"}`);
    return report;
  }
  };
