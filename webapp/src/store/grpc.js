// The store's gRPC (a part of Store, src/store.js).
import historyService from "../../../shared/services/historyService.js";
import { GRPC_LOG_MAX, errorText, isGrpc, isWebsocket } from "./shared.js";
import { applyAuth, maskSecrets } from "../features.js";
import { interpolateVariables } from "../../../shared/services/variableInterpolation.js";
import { platform } from "../platform/index.js";
import { snapshotResponse } from "../diff.js";

export const GrpcPart = (Base) =>
  class extends Base {
  // Desktop only: the platform's `grpc` (Rust through Tauri) is null in a
  // browser, which can't make gRPC calls.

  grpcState(tabId = this.tab?.id) {
    return this.grpcTabs.get(tabId) ?? { status: "idle", log: [], call: null, schema: null, schemaLoading: false, schemaError: null };
  }

  grpcSet(tabId, changes) {
    this.grpcTabs = new Map(this.grpcTabs).set(tabId, { ...this.grpcState(tabId), ...changes });
    this.emit();
  }

  grpcLog(tabId, entry) {
    const g = this.grpcState(tabId);
    this.grpcSet(tabId, { log: [...g.log, { time: new Date().toISOString(), ...entry }].slice(-GRPC_LOG_MAX) });
  }

  grpcClearLog(tabId = this.tab?.id) {
    this.grpcSet(tabId, { log: [] });
  }

  // The server and schema source of a tab, variables filled in: the
  // address, plaintext, metadata (enabled headers and header auth), the
  // .proto files and import paths, and the collection's folder for
  // relative paths (desktop folders are paths).
  grpcServer(tab = this.tab) {
    const variables = this.variables(tab.loadedCollectionId);
    const fill = (text) => (text ? interpolateVariables(text, variables) : text);
    const authed = applyAuth(tab.auth, { url: "", headers: (tab.headers || []).filter((h) => h.name && h.enabled !== false).map((h) => ({ name: fill(h.name), value: fill(h.value || "") })), body: "" }, fill);
    const ref = tab.loadedCollectionId != null ? this.folderOf(tab.loadedCollectionId)?.ref : null;
    return {
      address: fill(tab.url.trim()),
      plaintext: !!tab.grpcPlaintext,
      metadata: authed.headers.map((h) => [h.name, h.value]),
      protos: (tab.grpcProtos || []).map(fill).filter(Boolean),
      importPaths: (tab.grpcImportPaths || []).map(fill).filter(Boolean),
      baseDir: typeof ref === "string" ? ref : null,
    };
  }

  // Reads the schema (reflection or the .proto files) for the service and
  // method pickers.
  async grpcLoadSchema() {
    const tab = this.tab;
    const id = tab.id;
    const g = platform().grpc;
    if (!g) return this.notify("gRPC is available in the desktop app and CLI.");
    if (!tab.url.trim()) return this.notify("Enter the server address first (host:port).");
    let server;
    try {
      server = this.grpcServer(tab);
    } catch (e) {
      return this.notify(`Schema not loaded: ${errorText(e)}`);
    }
    this.grpcSet(id, { schemaLoading: true, schemaError: null });
    try {
      const schema = await g.list(server);
      this.grpcSet(id, { schema, schemaLoading: false });
      const t = this.tabs.find((x) => x.id === id);
      // Pick the first service and method when none is chosen yet.
      if (t && !t.grpcService) {
        const svc = schema.services.find((x) => !x.name.startsWith("grpc.reflection.")) ?? schema.services[0];
        if (svc) this.updateTabById(id, { grpcService: svc.name, grpcMethod: svc.methods[0]?.name ?? "" });
      }
    } catch (e) {
      this.grpcSet(id, { schemaLoading: false, schemaError: errorText(e) });
    }
  }

  // Replaces the message with a template of the method's request message.
  async grpcTemplate() {
    const tab = this.tab;
    const g = platform().grpc;
    if (!g) return;
    if (!tab.grpcService || !tab.grpcMethod) return this.notify("Pick a service and method first.");
    try {
      const d = await g.describe(this.grpcServer(tab), `${tab.grpcService}/${tab.grpcMethod}`);
      if (d.template) this.updateTabById(tab.id, { grpcMessage: JSON.stringify(d.template, null, 2) });
    } catch (e) {
      this.notify(`No template: ${errorText(e)}`);
    }
  }

  // Calls the tab's method with its message, variables filled in; the
  // headers, messages and status go to the tab's log.
  async grpcCall() {
    const tab = this.tab;
    const id = tab.id;
    const g = platform().grpc;
    if (!g) return this.notify("gRPC is available in the desktop app and CLI.");
    if (this.grpcState(id).status === "running") return;
    if (!tab.grpcService || !tab.grpcMethod) return this.notify("Pick a service and method first (Load schema).");
    // This call's identity: events of a cancelled call never touch a newer one.
    const token = {};
    this.grpcSet(id, { status: "running", token });
    const current = () => this.grpcState(id).token === token;
    let args;
    try {
      await this.resolveRefs({ url: tab.url, headers: tab.headers, auth: tab.auth, grpcMessage: tab.grpcMessage }, tab.loadedCollectionId);
      const fill = (text) => (text ? interpolateVariables(text, this.variables(tab.loadedCollectionId)) : text);
      args = { ...this.grpcServer(tab), service: tab.grpcService, method: tab.grpcMethod, message: fill(tab.grpcMessage || "") };
    } catch (e) {
      if (current()) this.grpcSet(id, { status: "done", token: null });
      return this.notify(`Not sent: ${errorText(e)}`);
    }
    if (!current()) return; // cancelled while references resolved
    this.grpcLog(id, { kind: "sent", text: `${args.service}/${args.method} ${args.message.trim() || "{}"}` });
    const done = () => this.grpcSet(id, { status: "done", call: null, token: null });
    const call = g.call(args, {
      onHeaders: (metadata) => current() && metadata.length && this.grpcLog(id, { kind: "headers", text: metadata.map(([k, v]) => `${k}: ${v}`).join("\n") }),
      onMessage: (data) => current() && this.grpcLog(id, { kind: "message", text: JSON.stringify(data, null, 2) }),
      onStatus: ({ code, name, message, trailers }) => {
        if (!current()) return;
        const head = code === 0 ? "OK" : `${name} (${code})${message ? `: ${message}` : ""}`;
        const tail = (trailers || []).map(([k, v]) => `${k}: ${v}`).join("\n");
        this.grpcLog(id, { kind: "status", ok: code === 0, text: tail ? `${head}\n${tail}` : head });
        done();
      },
      onError: (message) => {
        if (!current()) return;
        this.grpcLog(id, { kind: "error", text: message });
        done();
      },
    });
    if (current()) this.grpcSet(id, { call });
    else call?.cancel?.();
  }

  grpcCancel(tabId = this.tab?.id) {
    const g = this.grpcState(tabId);
    if (g.status !== "running") return;
    g.call?.cancel();
    this.grpcSet(tabId, { status: "done", call: null, token: null });
    // `cancelled` marks the call's outcome for the status chip (grpc-view.js), next to status and error.
    this.grpcLog(tabId, { kind: "system", text: "Cancelled", cancelled: true });
  }

  // Fetches the GraphQL schema of the current tab's endpoint by
  // introspection: its URL, headers, auth and cookies, through the same
  // transport as Send (proxy or direct; Rust on the desktop). The tab's
  // script doesn't run. The schema feeds completion in the query editor.
  async fetchSchema() {
    const tab = this.tab;
    if (!tab.url.trim()) {
      this.notify("Enter a URL first.");
      return null;
    }
    const id = tab.id;
    const set = (entry) => {
      this.schemas = new Map(this.schemas).set(id, entry);
      this.emit();
    };
    set({ ...this.schemas.get(id), loading: true, error: null });
    try {
      const gql = await import("../graphql-schema.js");
      const source = {
        url: tab.url,
        method: "POST",
        headers: tab.headers,
        auth: tab.auth,
        body: gql.INTROSPECTION_QUERY,
        bodyType: "graphql",
        graphql: { operationName: "IntrospectionQuery" },
        preRequestScript: "",
      };
      const { prepared, res } = await this.dispatch(source, tab.loadedCollectionId);
      const text = await res.text();
      if (!res.ok && !text.includes("__schema")) throw new Error(`the server answered ${res.status}${text ? `: ${text.slice(0, 160)}` : ""}`);
      const schema = gql.schemaFromResponse(text);
      const types = Object.keys(schema.getTypeMap()).filter((n) => !n.startsWith("__")).length;
      set({ schema, types, url: prepared.url, loading: false, error: null });
      return schema;
    } catch (e) {
      set({ schema: this.schemas.get(id)?.schema ?? null, loading: false, error: errorText(e) });
      return null;
    }
  }

  // Runs a collection's requests in order, stopping at the first failure (as `hitop run --bail`).
  // `folder`: only the requests in that folder of the collection (U39; their `file` paths).
  async runCollection(id, { folder = null } = {}) {
    const collection = this.collection(id);
    if (!collection?.requests?.length) return [];
    const requests = folder ? collection.requests.filter((r) => String(r.file ?? "").startsWith(`${folder}/`)) : collection.requests;
    if (!requests.length) return [];
    // One run at a time; the collections of runAll share its controller.
    const own = !this.runAbort;
    this.runAbort ??= new AbortController();
    const { signal } = this.runAbort;
    // WebSocket and gRPC requests connect on demand; a run skips them, as `hitop run` does.
    const planned = requests.filter((r) => !isWebsocket(r) && !isGrpc(r)).length;
    this.run = { ...this.run, collectionId: id, folder, planned, results: [], running: true, cancelled: false, report: null, ...(own ? { summary: null } : {}) };
    this.emit();
    const results = [];
    const runStarted = Date.now();
    // Secret values of the environment and of the globals this collection sees.
    this.refValues = {};
    const push = (r) => {
      r = maskSecrets(r, this.secretValuesFor(id));
      results.push(r);
      this.run = { ...this.run, results: [...results] };
      this.emit();
    };
    try {
      await this.runRequests(requests, { id, collection, push, signal });
    } finally {
      // Whatever happened, this run is over; a cancelled one is no measure for the next.
      const cancelled = signal.aborted;
      if (own) this.runAbort = null;
      this.run = { ...this.run, running: !own, cancelled, report: cancelled ? null : this.runReport(collection, results, runStarted, { folder }) };
      this.emit();
    }
    return results;
  }

  /** Stops the run: the request in flight is dropped, the results so far stay. */
  cancelRun() {
    this.runAbort?.abort(new DOMException("The run was cancelled", "AbortError"));
  }

  async runRequests(requests, { id, collection, push, signal }) {
    for (let i = 0; i < requests.length; i++) {
      const request = requests[i];
      if (isWebsocket(request) || isGrpc(request)) continue;
      if (signal.aborted) return;
      try {
        const started = performance.now();
        const limit = this.timeoutSignal();
        const { prepared, res } = await this.dispatch(request, id, { signal: limit ? AbortSignal.any([signal, limit]) : signal });
        const duration = res.timeMs ?? Math.round(performance.now() - started);
        const text = await res.text();
        const headers = {};
        res.headers.forEach((v, k) => (headers[k] = v));
        let tests = [];
        try {
          tests = await this.runPostScript(request.postRequestScript, id, text, headers, res.status, duration, { ...prepared, method: request.method });
        } catch (e) {
          tests = [...(e.tests ?? []), { name: "Script after", pass: false, detail: e.message }];
        }
        // The run item keeps the request as sent: mask secret values in it.
        const response = snapshotResponse({ status: res.status, headers, body: text, durationMs: duration });
        const item = maskSecrets({ ...historyService.createCollectionRunHistoryItem(request, prepared, res, collection.name, i), auth: request.auth, response }, this.secretValuesFor(id));
        this.history = historyService.addToHistory(this.history, item);
        const failed = tests.filter((t) => !t.pass);
        const success = res.ok && failed.length === 0;
        push({ requestId: request.id, requestName: request.name, method: request.method, url: prepared.url, statusCode: res.status, success, tests, response: failed.length ? failed.map((t) => `${t.name}: ${t.detail}`).join("; ") : text.slice(0, 200), ...(success ? {} : { excerpt: text.slice(0, 400), contentType: headers["content-type"] ?? null }), duration, ...(res.timings ? { timings: res.timings } : {}), ...(request.file ? { file: request.file } : {}) });
        if (!success) return;
      } catch (e) {
        if (signal.aborted) return;
        push({ requestId: request.id, requestName: request.name, method: request.method, url: request.url, statusCode: e.missingSecrets ? "Not sent" : e.stage === "prepare" ? "Script error" : "Error", success: false, response: e.message, ...(request.file ? { file: request.file } : {}) });
        return;
      }
    }
  }
  };
