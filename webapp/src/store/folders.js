// The store's collection folders and zip files (a part of Store, src/store.js).
import { GLOBALS_ENV, account, errorText, withoutSecretValues, workspaceOf } from "./shared.js";
import { buildReport, previousRun, rememberRun, runCommand, runStats } from "../run-report.js";
import { bruFiles, folderToZip, zipToFolder } from "../zip.js";
import { generateUniqueId } from "../../../shared/services/idGenerator.js";
import { loadFormat } from "../wasm/format.js";
import { platform } from "../platform/index.js";
import { secretNames } from "../features.js";

export const FoldersPart = (Base) =>
  class extends Base {
  folderOf(collectionId) {
    return this.folders.get(collectionId) ?? null;
  }

  // An environment's name in lists: a folder's environments say whose they are.
  environmentLabel(env) {
    const owner = this.folderEnvs.get(env.id);
    const col = owner && this.collection(owner.collectionId);
    return col ? `${env.name} (${col.name})` : env.name;
  }

  // What a folder holds now: the collection and its environments, secret
  // values left out (they go to the keychain or this browser instead).
  folderContent(collectionId) {
    const collection = this.collection(collectionId);
    const environments = this.environments
      .filter((e) => this.folderEnvs.get(e.id)?.collectionId === collectionId)
      .map((e) => withoutSecretValues({ ...e, id: this.folderEnvs.get(e.id).id }));
    // A folder's globals.yaml goes back as it came (left out: the file stays as it is).
    const globals = this.folders.get(collectionId)?.globals;
    return globals ? { collection, environments, globals: withoutSecretValues({ ...globals, id: "globals", name: "globals" }) } : { collection, environments };
  }

  folderSecretValues(collectionId) {
    const col = this.collection(collectionId);
    const out = {};
    if (!col) return out;
    for (const e of this.environments) {
      if (this.folderEnvs.get(e.id)?.collectionId !== collectionId) continue;
      for (const n of secretNames(e)) if (e.variables[n]) out[account(workspaceOf(col), e.name, n)] = e.variables[n];
    }
    return out;
  }

  // Puts a folder's collection and environments into the app, replacing the
  // collection with the same id (the folder wins).
  async attachFolder(ref, folder, { remember = true } = {}) {
    const p = platform();
    const col = folder.collection;
    const envs = (folder.environments || []).map((e) => ({ ...e, variables: { ...e.variables }, folderId: e.id, id: `folder:${col.id}:${e.id}` }));
    const accounts = envs.flatMap((e) => secretNames(e).map((n) => account(workspaceOf(col), e.name, n)));
    const values = accounts.length ? await p.folderSecrets.load(accounts) : {};
    for (const e of envs) for (const n of secretNames(e)) e.variables[n] = values[account(workspaceOf(col), e.name, n)] ?? "";
    // The folder's globals.yaml, with secret values from the keychain (desktop).
    let globals = null;
    if (folder.globals) {
      globals = { variables: { ...folder.globals.variables }, secrets: secretNames(folder.globals) };
      const gAccounts = globals.secrets.map((n) => account(workspaceOf(col), GLOBALS_ENV, n));
      const gValues = gAccounts.length ? await p.folderSecrets.load(gAccounts) : {};
      for (const n of globals.secrets) globals.variables[n] = gValues[account(workspaceOf(col), GLOBALS_ENV, n)] ?? "";
    }
    for (const [id, owner] of this.folderEnvs) if (owner.collectionId === col.id) this.folderEnvs.delete(id);
    for (const e of envs) {
      this.folderEnvs.set(e.id, { collectionId: col.id, id: e.folderId });
      delete e.folderId;
    }
    this.environments = [...this.environments.filter((e) => !e.id.startsWith?.(`folder:${col.id}:`)), ...envs];
    const at = this.collections.findIndex((c) => c.id === col.id);
    this.collections = at === -1 ? [...this.collections, col] : this.collections.map((c, i) => (i === at ? col : c));
    const entry = { ref, label: p.folders.label(ref), connected: true, error: null, written: null, secrets: {}, globals };
    this.folders.set(col.id, entry);
    entry.secrets = this.folderSecretValues(col.id);
    entry.written = JSON.stringify(this.folderContent(col.id));
    p.setFolderCollections?.([...this.folders.keys()]);
    if (remember) p.folders.remember(col.id, ref);
    this.emit();
    return col;
  }

  async openFolder() {
    const f = platform().folders;
    if (!f?.pick) return;
    let ref;
    try {
      ref = await f.pick();
    } catch (e) {
      this.notify(`Couldn't open the folder: ${errorText(e)}`);
      return;
    }
    if (!ref) return;
    let folder;
    try {
      folder = await f.read(ref);
    } catch (e) {
      const text = errorText(e);
      this.notify(text.includes("not a collection folder") ? `${f.label(ref)} has no hitop.yaml, so it isn't a collection folder. To start one, open a collection and choose Save to folder.` : `Couldn't open ${f.label(ref)}: ${text}`);
      return;
    }
    const col = await this.attachFolder(ref, folder);
    this.show("collection", col.id);
    this.notify(`Opened ${f.label(ref)}. Changes are saved to the folder as you make them.`);
  }

  // Writes a collection into a new folder and keeps saving there.
  async saveToFolder(collectionId) {
    const f = platform().folders;
    const col = this.collection(collectionId);
    if (!f?.pick || !col) return;
    let ref;
    try {
      ref = await f.pick();
    } catch (e) {
      this.notify(`Couldn't open the folder: ${errorText(e)}`);
      return;
    }
    if (!ref) return;
    try {
      await f.read(ref);
      this.notify(`${f.label(ref)} already holds a collection. Use Open folder to work on it, or pick an empty folder.`);
      return;
    } catch {
      // not a collection folder yet: good
    }
    const folder = { collection: col, environments: [] };
    try {
      await f.write(ref, folder);
    } catch (e) {
      this.notify(`Couldn't save to ${f.label(ref)}: ${errorText(e)}`);
      return;
    }
    await this.attachFolder(ref, folder);
    this.notify(`Saved ${col.name} to ${f.label(ref)}. Changes are saved there as you make them.`);
  }

  // A folder from an earlier visit (web): the browser asks again.
  async reconnectFolder(collectionId) {
    const f = platform().folders;
    const entry = this.folders.get(collectionId);
    if (!f || !entry) return;
    try {
      if (!(await f.reconnect(entry.ref))) return;
      await this.attachFolder(entry.ref, await f.read(entry.ref), { remember: false });
      this.notify(`Reconnected ${entry.label}`);
    } catch (e) {
      // A shared collection's files here are gone: from the server again.
      if (String(entry.ref).startsWith("hitop-sync:") && this.syncClient?.link?.(collectionId)) {
        try {
          await this.syncClient.syncNow(collectionId);
          this.notify(`Reconnected ${entry.label}`);
          return;
        } catch {
          // said below
        }
      }
      this.notify(`Couldn't read ${entry.label}: ${errorText(e)}`);
    }
  }

  // Takes the collection out of the app; the folder's files stay.
  closeFolder(collectionId) {
    const p = platform();
    this.folders.delete(collectionId);
    p.folders?.forget(collectionId);
    p.setFolderCollections?.([...this.folders.keys()]);
    for (const [id, owner] of this.folderEnvs) {
      if (owner.collectionId !== collectionId) continue;
      this.folderEnvs.delete(id);
      this.environments = this.environments.filter((e) => e.id !== id);
      if (this.activeEnvironmentId === id) this.activeEnvironmentId = null;
    }
  }

  closeAllFolders() {
    for (const id of [...this.folders.keys()]) this.closeFolder(id);
  }

  async restoreFolders() {
    const f = platform().folders;
    if (!f) return;
    for (const { collectionId, ref, connected } of await f.restore()) {
      if (!connected) {
        // Web: the browser needs a click before the app may read it again.
        if (this.collection(collectionId)) this.folders.set(collectionId, { ref, label: f.label(ref), connected: false, error: null, written: null, secrets: {} });
        else f.forget(collectionId);
        continue;
      }
      try {
        await this.attachFolder(ref, await f.read(ref), { remember: false });
      } catch (e) {
        // Still a folder's (or a shared, end-to-end encrypted) collection: never taken for one of the
        // person's own, which the library keeps.
        if (this.collection(collectionId)) this.folders.set(collectionId, { ref, label: f.label(ref), connected: false, error: null, written: null, secrets: {} });
        else f.forget(collectionId);
        this.notify(`Couldn't open the folder ${f.label(ref)}: ${errorText(e)}. Open it again with Open folder.`);
      }
    }
    this.emit();
  }

  // Live save: each connected folder gets what changed, one write at a time.
  syncFolders() {
    const p = platform();
    for (const [id, entry] of this.folders) {
      if (!entry.connected) continue;
      if (!this.collection(id)) {
        this.closeFolder(id);
        continue;
      }
      const secrets = this.folderSecretValues(id);
      const changed = Object.fromEntries([
        ...Object.entries(secrets).filter(([k, v]) => entry.secrets[k] !== v),
        ...Object.keys(entry.secrets).filter((k) => !(k in secrets)).map((k) => [k, null]),
      ]);
      if (Object.keys(changed).length) {
        p.folderSecrets.save(changed);
        entry.secrets = secrets;
      }
      const content = this.folderContent(id);
      const json = JSON.stringify(content);
      if (json === entry.written) continue;
      if (entry.writing) {
        entry.again = true;
        continue;
      }
      entry.writing = true;
      entry.saving = p.folders
        .write(entry.ref, content)
        .then(() => {
          entry.written = json;
          if (entry.error) {
            entry.error = null;
            this.dispatchEvent(new Event("change"));
          }
        })
        .catch((e) => {
          const first = !entry.error;
          entry.error = errorText(e);
          entry.written = json; // don't retry the same content on every keystroke
          if (first) this.notify(`Couldn't save to ${entry.label}: ${entry.error}`);
          this.dispatchEvent(new Event("change"));
        })
        .finally(() => {
          entry.writing = false;
          if (entry.again) {
            entry.again = false;
            this.syncFolders();
          }
        });
    }
  }

  // Writes what a folder collection has changed now, and waits for it (the
  // desktop's Git panel does this before it commits, pulls or switches).
  async flushFolder(collectionId) {
    const entry = this.folders.get(collectionId);
    if (!entry?.connected) return;
    this.save();
    for (let i = 0; i < 20 && (entry.writing || entry.again); i++) await entry.saving;
  }

  // Reads a folder collection again after its files changed on disk (a
  // pull, a branch switch, a resolved conflict): the folder wins.
  async reloadFolder(collectionId) {
    const f = platform().folders;
    const entry = this.folders.get(collectionId);
    if (!f || !entry) return null;
    try {
      const folder = await f.read(entry.ref);
      if (folder.collection.id !== collectionId) {
        // hitop.yaml now names another collection: replace the old one.
        this.closeFolder(collectionId);
        this.collections = this.collections.filter((c) => c.id !== collectionId);
      }
      const col = await this.attachFolder(entry.ref, folder);
      if (this.view.collectionId === collectionId) this.view = { ...this.view, collectionId: col.id };
      this.emit();
      return col.id;
    } catch (e) {
      this.notify(`Couldn't read ${entry.label} again: ${errorText(e)}`);
      return null;
    }
  }

  // A collection as a .zip of its folder. A folder collection takes its own
  // environments; any other takes the app's. Secret values are never in it.
  // The collection's documentation as one HTML page (as `hitop docs`): saved or downloaded.
  async exportDocs(collectionId) {
    const col = this.collection(collectionId);
    if (!col) return;
    try {
      const page = (await loadFormat()).collectionDocs(col);
      const name = `${(col.name || "collection").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "collection"}-docs.html`;
      if (await platform().saveFile(name, page, "text/html")) this.notify(`Saved ${name}: open it in a browser or publish it anywhere`);
    } catch (e) {
      this.notify(`Couldn't make the docs page of ${col.name}: ${errorText(e)}`);
    }
  }

  async exportZip(collectionId) {
    const col = this.collection(collectionId);
    if (!col) return;
    const content = this.folders.has(collectionId)
      ? this.folderContent(collectionId)
      : { collection: col, environments: this.environments.filter((e) => !this.folderEnvs.has(e.id)).map(withoutSecretValues) };
    try {
      const { name, bytes } = await folderToZip(content);
      if (await platform().saveFile(name, bytes, "application/zip")) this.notify(`Exported ${name}`);
    } catch (e) {
      this.notify(`Couldn't export ${col.name}: ${errorText(e)}`);
    }
  }

  // Adds the collection in a .zip of a folder, and its environments with
  // names you don't have yet (secrets come without values).
  async importZip(file) {
    let folder;
    try {
      const bru = bruFiles(new Uint8Array(await file.arrayBuffer()));
      if (bru) return { report: await this.importConverted((await loadFormat()).convertImportFiles(bru)) };
      folder = await zipToFolder(new Uint8Array(await file.arrayBuffer()), file.name);
    } catch (e) {
      this.notify(`Couldn't import ${file.name}: ${errorText(e)}`);
      return null;
    }
    const col = this.inOrg(folder.collection);
    if (this.collection(col.id)) col.id = generateUniqueId();
    this.collections = [...this.collections, col];
    const names = new Set(this.environments.map((e) => e.name));
    const added = (folder.environments || [])
      .filter((e) => !names.has(e.name))
      .map((e) => ({ ...e, id: crypto.randomUUID(), variables: { ...e.variables, ...Object.fromEntries(secretNames(e).map((n) => [n, ""])) } }));
    this.environments = [...this.environments, ...added];
    this.view = { kind: "collection", collectionId: col.id };
    this.emit();
    this.notify(`Imported ${col.name}${added.length ? ` and ${added.length} environment${added.length === 1 ? "" : "s"}` : ""}`);
    return col;
  }

  // What the Run report shows for a finished run (run-report.js): report.json, the stats against
  // the previous run of this collection, and the `hitop run` line. Remembers the run's summary.
  runReport(collection, results, startedAt, { folder = null } = {}) {
    if (!results.length) return null;
    const durationMs = Date.now() - startedAt;
    const environment = this.activeEnvironment?.name ?? null;
    let storage;
    try {
      storage = globalThis.localStorage ?? null;
    } catch {
      storage = null;
    }
    // A folder's run (U39) is compared with nothing and not remembered: its p95 isn't the
    // collection's. `hitop run` has no folder argument, so it gets no command line either.
    const previous = storage && !folder ? previousRun(storage, collection.id) : null;
    const stats = runStats(results, durationMs, previous);
    const report = buildReport({ collection: collection.name, environment, results, startedAt, durationMs });
    if (storage && !folder) rememberRun(storage, collection.id, { at: startedAt, p95: stats.p95, passed: report.passed, requests: results.length, wallMs: durationMs });
    const place = this.folderOf(collection.id);
    const folderLabel = place && !String(place.ref ?? "").startsWith("hitop-sync:") ? place.label : null;
    return { report, stats, previous, folder, command: folder ? null : runCommand({ collection: collection.name, folderLabel, environment }) };
  }

  async runAll() {
    if (this.run.running) return;
    const controller = (this.runAbort = new AbortController());
    const started = performance.now();
    let total = 0;
    let ok = 0;
    let ran = 0;
    let planned = 0;
    let assertions = 0;
    let assertionsFailed = 0;
    this.run = { collectionId: null, results: [], running: true, cancelled: false, completed: 0, summary: null };
    this.emit();
    for (const col of this.visibleCollections) {
      if (!col.requests?.length || controller.signal.aborted) continue;
      ran++;
      planned += col.requests.length;
      let results;
      try {
        results = await this.runCollection(col.id);
      } catch (e) {
        this.runAbort = null;
        this.run = { ...this.run, running: false };
        this.emit();
        throw e;
      }
      total += results.length;
      ok += results.filter((r) => r.success).length;
      assertions += results.reduce((n, r) => n + (r.tests?.length ?? 0), 0);
      assertionsFailed += results.reduce((n, r) => n + (r.tests?.filter((t) => !t.pass).length ?? 0), 0);
      this.run = { ...this.run, running: true, completed: this.run.completed + 1 };
    }
    this.runAbort = null;
    this.run = {
      collectionId: null,
      results: [],
      running: false,
      cancelled: controller.signal.aborted,
      completed: 0,
      summary: { totalCollections: ran, plannedRequests: planned, totalRequests: total, successCount: ok, failedCount: total - ok, assertions, assertionsFailed, duration: Math.round(performance.now() - started) },
    };
    this.emit();
  }
  };
