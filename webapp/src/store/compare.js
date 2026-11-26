// The store's response diff, examples and code snippets (a part of Store, src/store.js).
import { BODY_METHODS, SAVED_FIELDS, savedShape, tabForRequest } from "./shared.js";
import { cleanAuth, maskSecrets, missingSecrets, missingSecretsText, secretNames, secretRefs, secretValues, usedVariables } from "../features.js";
import { exampleResponse, snapshotResponse } from "../diff.js";
import { isEmptyMyRequests, isPristineSample } from "../new-request.js";
import { loadFormat } from "../wasm/format.js";
import { platform } from "../platform/index.js";
import { callArguments, fromSecret, pluginCallTexts } from "../plugin-calls.js";
import { pluginUnavailable } from "../../../shared/services/variableInterpolation.js";

export const ComparePart = (Base) =>
  class extends Base {
  // Secret names and values a request in this collection sees: the active
  // environment's and those of the globals it uses.
  secretNamesFor(collectionId) {
    const own = this.folders.get(collectionId)?.globals ?? this.globals;
    return [...new Set([...secretNames(this.activeEnvironment), ...secretNames(own)])];
  }

  /**
   * U19: secrets `source` (a tab or saved request) uses with no value on this device, as
   * [{name, where}], and the note naming where to fill them in. `variables`: what the request sees
   * (default: now; prepare passes the values after a Postman pre-script ran).
   */
  missingSecretsFor(source, collectionId, variables = this.variables(collectionId)) {
    const own = this.folders.get(collectionId)?.globals ?? this.globals;
    const used = usedVariables({ ...source, body: source.body ?? source.requestBody });
    const missing = missingSecrets(used, variables, secretNames(this.activeEnvironment), secretNames(own));
    return { missing, text: missingSecretsText(missing, this.activeEnvironment ? this.environmentLabel(this.activeEnvironment) : "") };
  }

  secretValuesFor(collectionId) {
    const own = this.folders.get(collectionId)?.globals ?? this.globals;
    return [...secretValues(this.activeEnvironment), ...secretValues(own), ...Object.values(this.sharedFor(collectionId)), ...[...this.refSeen]];
  }

  // HITOP Cloud C4: secret values shared end to end in a synced collection's workspace (`hitop
  // sync secrets share`), by environment name. Memory only, never saved; they fill a secret of the
  // active environment only where it has no value of its own here.
  setSharedValues(collectionId, byEnvironment) {
    this.sharedValues = { ...(this.sharedValues ?? {}), [collectionId]: byEnvironment ?? {} };
    this.emit();
  }

  sharedFor(collectionId) {
    const env = this.activeEnvironment;
    if (!env) return {};
    const shared = this.sharedValues?.[collectionId]?.[env.name] ?? {};
    return Object.fromEntries(secretNames(env).filter((n) => typeof shared[n] === "string" && !(typeof env.variables?.[n] === "string" && env.variables[n] !== "")).map((n) => [n, shared[n]]));
  }

  // Resolves the ${secret:...} references a request (not its scripts) and the
  // variables it sees use, through the desktop (hitop-core's secret managers).
  // The browser can't reach them: a request that uses one fails with a note.
  async resolveRefs(source, collectionId) {
    await this.resolveSecretManagerRefs(source, collectionId);
    await this.resolvePlugins(source, collectionId);
  }

  async resolveSecretManagerRefs(source, collectionId) {
    const wanted = [...new Set([...secretRefs(source), ...secretRefs(Object.values(this.variables(collectionId)))])].filter((r) => !(r in this.refValues));
    if (!wanted.length) return;
    const p = platform();
    const list = wanted.map((r) => `\${${r}}`).join(", ");
    if (!p.resolveSecretRefs) throw new Error(`Secret manager references resolve in the hitop desktop app and the CLI, not in the browser: ${list}`);
    const values = await p.resolveSecretRefs(wanted);
    this.refValues = { ...this.refValues, ...values };
    for (const v of Object.values(values ?? {})) if (typeof v === "string" && v) this.refSeen.add(v);
  }

  // Plugin calls (spec 5c) in what a request sends and in the variables it sees: answered by the
  // installed plugins (platform().plugins: the plugins worker on the web; none yet on the desktop)
  // before the request is built, once per call and argument values, and kept for this request only
  // (`pluginValues` by collection, replaced on each request). An answer computed from a secret is
  // masked like one (threat T11). A call nothing answered is refused by the interpolators.
  async resolvePlugins(source, collectionId) {
    this.pluginValues ??= {};
    delete this.pluginValues[collectionId];
    const variables = this.variables(collectionId);
    const texts = [...pluginCallTexts(source), ...pluginCallTexts(Object.values(variables))].filter((t) => !(t in variables));
    if (!texts.length) return;
    const format = await loadFormat();
    const calls = [...new Set(texts)].map((text) => ({ text, call: format.pluginCall(text) })).filter((c) => c.call);
    if (!calls.length) return;
    const plugins = platform().plugins;
    if (!plugins) throw new Error(pluginUnavailable(calls[0].call.plugin));
    const secrets = this.secretValuesFor(collectionId);
    const answers = {};
    const once = new Map();
    for (const { text, call } of calls) {
      const args = callArguments(call, variables);
      const key = JSON.stringify([call.plugin, args]);
      if (!once.has(key)) once.set(key, await plugins.call(call.plugin, "value", args));
      const answer = once.get(key);
      if (answer && fromSecret(args, secrets)) this.refSeen.add(answer);
      answers[text] = answer;
    }
    this.pluginValues[collectionId] = answers;
  }

  // True when the tab has edits its saved request doesn't (computed, never
  // stored). Cached per tab and request object: both are replaced on change.
  isDirty(tab = this.tab) {
    const request = this.loadedRequest(tab);
    if (!request) return false;
    this.dirtyCache ??= new WeakMap();
    const hit = this.dirtyCache.get(tab);
    if (hit?.request === request) return hit.dirty;
    const dirty = savedShape(tab) !== savedShape(tabForRequest(request, tab.loadedCollectionId));
    this.dirtyCache.set(tab, { request, dirty });
    return dirty;
  }

  isDirtyRequest(requestId, collectionId) {
    const tab = this.tabs.find((t) => t.loadedRequestId === requestId && t.loadedCollectionId === collectionId);
    return tab ? this.isDirty(tab) : false;
  }

  // An unsaved draft with something in it (closing it loses that).
  hasDraftContent(tab) {
    return !tab.loadedRequestId && Boolean(tab.url?.trim() || tab.requestBody?.trim() || tab.preRequestScript?.trim() || tab.postRequestScript?.trim());
  }

  // What importing in place of the collections would delete: [{ name, requests }],
  // empty when there is nothing to lose (what the first visit made: the untouched sample, an
  // empty My requests or the older first-run Default with its blank request).
  collectionsAtStake() {
    const filled = (r) => Boolean(r.url?.trim() || r.body?.trim() || r.preRequestScript?.trim() || r.postRequestScript?.trim() || r.kind);
    const kept = this.collections.filter((c) => !isPristineSample(c) && !isEmptyMyRequests(c));
    if (!kept.length) return [];
    const stake = this.collections.map((c) => ({ name: c.name, requests: (c.requests || []).length, filled: (c.requests || []).some(filled) || Object.keys(c.variables || {}).length > 0 }));
    return this.collections.length > 1 || stake.some((c) => c.filled) ? stake : [];
  }

  loadedRequest(tab = this.tab) {
    return (tab?.loadedRequestId && this.collection(tab.loadedCollectionId)?.requests.find((r) => r.id === tab.loadedRequestId)) || null;
  }

  // Saves the current tab's response as an example of its saved request
  // (`examples` in the folder format), secret values masked.
  saveExample(name) {
    const tab = this.tab;
    const request = this.loadedRequest(tab);
    if (!request || tab.statusCode == null || tab.responseType === "error") return false;
    const example = maskSecrets(
      { name: name.trim() || `${tab.statusCode} response`, status: tab.statusCode, headers: { ...(tab.responseHeaders || {}) }, body: tab.response ?? "" },
      this.secretValuesFor(tab.loadedCollectionId),
    );
    this.collections = this.collections.map((c) =>
      c.id === tab.loadedCollectionId ? { ...c, requests: c.requests.map((r) => (r.id === request.id ? { ...r, examples: [...(r.examples || []), example] } : r)) } : c,
    );
    this.emit();
    this.notify(`Saved example "${example.name}"`);
    return true;
  }

  // What the compare dialog can pick from: the current response, the loaded
  // request's examples, then history items that kept their response (items
  // stored before responses were kept have none and are left out).
  // `only` {method, url}: history of that request alone (UX2: the response pane's Diff).
  compareSources(only = null) {
    const out = [];
    const tab = this.tab;
    if (tab?.statusCode != null && tab.responseType !== "error") {
      // Masked like the stored ones, so a secret never shows up as a difference.
      const response = maskSecrets(snapshotResponse({ status: tab.statusCode, headers: tab.responseHeaders, body: tab.response, durationMs: tab.durationMs }), this.secretValuesFor(tab.loadedCollectionId));
      out.push({ id: "current", label: "Current response", response, request: `${tab.method} ${tab.url}` });
    }
    (this.loadedRequest(tab)?.examples || []).forEach((e, i) => out.push({ id: `example:${i}`, label: `Example: ${e?.name ?? i + 1}`, response: exampleResponse(e) }));
    for (const h of [...this.history].reverse()) {
      if (!h?.response) continue;
      if (only && (h.isCollectionRun || h.method !== only.method || h.url !== only.url)) continue;
      const when = new Date(h.timestamp).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
      const what = h.isCollectionRun ? `${h.collectionName} → ${h.requestName}` : `${h.method} ${h.url}`;
      out.push({ id: `history:${h.id}`, label: `${h.response.status ?? "?"} ${what} · ${when}`, response: h.response, request: h.isCollectionRun ? null : `${h.method} ${h.url}` });
    }
    return out;
  }

  // The current tab as hitop-wasm's snippet input. Secrets stay {{NAME}}
  // placeholders unless `showSecrets`.
  // The Content-Type dispatch adds to this tab's raw body (none set, not
  // GraphQL or a form), for the Headers note and the Code dialog.
  autoContentType(tab = this.tab) {
    if (!tab || tab.kind || !BODY_METHODS.includes(tab.method) || (tab.bodyMode && tab.bodyMode !== "raw")) return null;
    if ((tab.headers || []).some((h) => h.name?.toLowerCase() === "content-type" && h.value)) return null;
    return this.format?.rawContentType(tab.requestBody) ?? null;
  }

  snippetInput(showSecrets = false, tab = this.tab) {
    const request = {
      id: 0,
      name: tab.title || "",
      url: tab.url || "",
      method: tab.method || "GET",
      // As sent: headers with no value are left out.
      headers: (tab.headers || []).filter((h) => h.name && h.value).map((h) => ({ name: h.name, value: h.value, ...(h.enabled === false ? { enabled: false } : {}) })),
      body: BODY_METHODS.includes(tab.method) ? tab.requestBody || "" : "",
      ...SAVED_FIELDS(tab),
    };
    // hitop-core's generator adds the Content-Type and scheme hitop sends with.
    const auth = cleanAuth(tab.auth);
    if (auth) request.auth = auth;
    // The web app sends a body only with these methods.
    if (!BODY_METHODS.includes(tab.method)) Object.assign(request, { bodyMode: "raw", bodyType: undefined });
    // ${secret:...} references stay placeholders (as `hitop snippet` keeps them
    // without --show-secrets): the page never puts their values in code.
    const variables = this.variables(tab.loadedCollectionId, false);
    const refs = [...secretRefs([request, Object.values(variables)])];
    for (const [k, v] of Object.entries(variables)) if (typeof v === "string" && v.includes("${secret:")) variables[k] = v.replace(/\$\{(secret:[^}]+)\}/g, "{{$1}}");
    return { request, variables, secrets: [...(showSecrets ? [] : this.secretNamesFor(tab.loadedCollectionId)), ...refs] };
  }

  // { code, placeholders } from hitop-core's generator.
  async snippet(lang, showSecrets = false, tab = this.tab) {
    return (await loadFormat()).snippet(this.snippetInput(showSecrets, tab), lang);
  }

  /** A saved request as code without opening it (UX2: the sidebar menu's Copy as cURL). */
  snippetFor(request, collectionId, lang = "curl") {
    return this.snippet(lang, false, tabForRequest(request, collectionId));
  }
  };
