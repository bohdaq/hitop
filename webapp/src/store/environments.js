// The store's environments (a part of Store, src/store.js).
import collectionService from "../../../shared/services/collectionService.js";
import { exportableEnvironments, exportableGlobals, isPostmanGlobals, mergeGlobals, parseEnvironments, parseGlobals, secretNames, withSecretRefs } from "../features.js";

export const EnvironmentsPart = (Base) =>
  class extends Base {
  get activeEnvironment() {
    const env = this.environments.find((e) => e.id === this.activeEnvironmentId) ?? null;
    // Another org's environment isn't used here.
    return env && this.envInCurrentOrg(env) ? env : null;
  }

  // The globals a collection sees: a folder's own globals.yaml when it has
  // one, else the app's; with what scripts set on top.
  globalsKey(collectionId) {
    return this.folders.get(collectionId)?.globals ? `folder:${collectionId}` : "app";
  }

  globalsFor(collectionId) {
    const own = this.folders.get(collectionId)?.globals;
    const out = { ...(own ?? this.globals).variables };
    for (const [k, v] of Object.entries(this.scriptGlobals[this.globalsKey(collectionId)] ?? {})) {
      if (v === null) delete out[k];
      else out[k] = v;
    }
    return out;
  }

  setScriptGlobals(collectionId, updates) {
    const key = this.globalsKey(collectionId);
    if (updates && Object.keys(updates).length) this.scriptGlobals[key] = { ...this.scriptGlobals[key], ...updates };
  }

  setGlobals(variables, secrets) {
    const names = (secrets ?? this.globals.secrets).filter((n) => n in variables);
    this.globals = { variables, secrets: names };
    this.emit();
  }

  // Postman globals or HITOP's globals export: added to the globals.
  importGlobals(json) {
    const incoming = parseGlobals(json); // throws on bad input
    this.globals = mergeGlobals(this.globals, incoming);
    this.emit();
    const n = Object.keys(incoming.variables).length;
    this.notify(`Imported ${n} global${n === 1 ? "" : "s"}`);
  }

  exportGlobals() {
    return JSON.stringify(exportableGlobals(this.globals), null, 2);
  }

  // The variables a request sees, later ones winning: the globals, its
  // collection's, the active environment's, then what scripts set.
  variables(collectionId, withRefs = true) {
    const out = { ...this.globalsFor(collectionId), ...collectionService.getCollectionVariables(this.collections, collectionId), ...(this.activeEnvironment?.variables ?? {}), ...this.sharedFor(collectionId) };
    for (const [k, v] of Object.entries(this.scriptVariables[collectionId] ?? {})) {
      if (v === null) delete out[k];
      else out[k] = v;
    }
    return withRefs ? withSecretRefs(out, this.refValues) : out;
  }

  /**
   * Where `${name}` gets its value in a request of `collectionId`, in the order `variables()` applies
   * them: { value, scope }, secret values masked; null when nothing defines it.
   */
  variableSource(collectionId, name) {
    if (name.startsWith("secret:")) return { value: "••••", scope: "secret manager reference" };
    const script = this.scriptVariables[collectionId] ?? {};
    if (name in script && script[name] !== null) return { value: String(script[name]), scope: "set by a script" };
    const env = this.activeEnvironment;
    const ownGlobals = this.folders.get(collectionId)?.globals ?? this.globals;
    const secrets = new Set([...(env?.secrets ?? []), ...(ownGlobals?.secrets ?? [])]);
    const envLabel = env ? `environment ${this.environmentLabel(env)}` : "environment";
    const scopes = [
      [this.sharedFor(collectionId), "team environment"],
      [env?.variables ?? {}, envLabel],
      [collectionService.getCollectionVariables(this.collections, collectionId) ?? {}, "collection"],
      [this.globalsFor(collectionId), "globals"],
    ];
    // U19: a secret with no value on this device (absent or empty) says so: `missingSecret`.
    const empty = (v) => v == null || v === "";
    for (const [vars, scope] of scopes) {
      if (!vars || !Object.hasOwn(vars, name)) continue;
      if (secrets.has(name) && empty(vars[name])) return { value: "", scope, missingSecret: true };
      return { value: secrets.has(name) ? "••••" : String(vars[name] ?? ""), scope };
    }
    if (secrets.has(name)) return { value: "", scope: secretNames(env).includes(name) ? envLabel : "globals", missingSecret: true };
    return null;
  }

  setScriptVariables(collectionId, updates) {
    if (updates && Object.keys(updates).length) this.scriptVariables[collectionId] = { ...this.scriptVariables[collectionId], ...updates };
  }

  setActiveEnvironment(id) {
    this.activeEnvironmentId = this.environments.some((e) => e.id === id) ? id : null;
    this.emit();
  }

  addEnvironment(name) {
    if (!name.trim()) return;
    // In the current org only, until it's made available in every org.
    const env = { id: crypto.randomUUID(), name: name.trim(), variables: {}, ...(this.currentOrg ? { org: this.currentOrg } : {}) };
    this.environments = [...this.environments, env];
    if (!this.activeEnvironment) this.activeEnvironmentId = env.id;
    this.view = { kind: "environments", collectionId: null, environmentId: env.id };
    this.emit();
  }

  /** UX2: a copy of an environment ("Staging (copy)"), its variables and secret names with it. */
  duplicateEnvironment(id) {
    const from = this.environments.find((e) => e.id === id);
    if (!from || from.team) return;
    const names = new Set(this.environments.map((e) => e.name));
    let name = `${from.name} (copy)`;
    for (let n = 2; names.has(name); n++) name = `${from.name} (copy ${n})`;
    const env = { ...structuredClone(from), id: crypto.randomUUID(), name };
    this.environments = [...this.environments, env];
    this.view = { kind: "environments", collectionId: null, environmentId: env.id };
    this.emit();
  }

  renameEnvironment(id, name) {
    if (!name.trim()) return;
    this.environments = this.environments.map((e) => (e.id === id ? { ...e, name: name.trim() } : e));
    this.emit();
  }

  // `secrets`, when given, is the new list of secret names; they must be
  // keys of `variables`.
  setEnvironmentVariables(id, variables, secrets) {
    this.environments = this.environments.map((e) => {
      if (e.id !== id) return e;
      const next = { ...e, variables };
      const names = (secrets ?? secretNames(e)).filter((n) => n in variables);
      if (names.length) next.secrets = names;
      else delete next.secrets;
      return next;
    });
    this.emit();
  }

  deleteEnvironment(id) {
    this.environments = this.environments.filter((e) => e.id !== id);
    if (this.activeEnvironmentId === id) this.activeEnvironmentId = null;
    this.emit();
  }

  importEnvironments(json) {
    // Postman globals go to the globals, not to a new environment.
    let parsed = null;
    try {
      parsed = JSON.parse(json);
    } catch {
      // parseEnvironments says what's wrong
    }
    if (isPostmanGlobals(parsed)) return this.importGlobals(json);
    const imported = parseEnvironments(json); // throws on bad input
    this.environments = [...this.environments, ...imported];
    this.emit();
    this.notify(`Imported ${imported.length} environment${imported.length === 1 ? "" : "s"}`);
  }

  exportEnvironments() {
    return JSON.stringify(exportableEnvironments(this.environments), null, 2);
  }
  };
