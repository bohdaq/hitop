// The store's sending (a part of Store, src/store.js).
import historyService from "../../../shared/services/historyService.js";
import tabService from "../../../shared/services/tabService.js";
import { BODY_METHODS, NO_RESPONSE, SAVED_FIELDS, headersObject, readResponse } from "./shared.js";
import { asksForEvents, awsV4Headers, cleanAuth, cookieHeader, digestAnswer, maskSecrets, mergeCookies, multipartBody, parseDigestChallenge, parseEvents, urlEncodedBody } from "../features.js";
import { inheritDefaults, requestTimeout } from "../defaults.js";
import { interpolateVariables } from "../../../shared/services/variableInterpolation.js";
import { loadFormat } from "../wasm/format.js";
import { platform } from "../platform/index.js";
import { saveSettings, seconds } from "../settings.js";
import { snapshotResponse } from "../diff.js";

export const SendingPart = (Base) =>
  class extends Base {
  // Prepares and sends one request; returns { prepared, res }. Handles form
  // bodies and the cookie jar (proxied requests only: the browser keeps
  // localhost cookies itself and hides Set-Cookie from scripts).
  // `signal` stops it (Cancel, the request timeout); the desktop gets the
  // timeout too, so Rust gives up as well.
  // `onText` + `live`: send it as an event stream (pieces to onText). `onText` without `live`: an
  // answer that turns out to be an event stream (Content-Type text/event-stream) is read as one,
  // after `onStream()` (UX2 (8)): a direct answer's own body; through the proxy (which leaves the
  // body of such a GET unread when asked, `eventStreams`) the stream endpoint.
  async dispatch(source, collectionId, { signal, onText, live = Boolean(onText), onStream = () => {} } = {}) {
    signal ??= this.timeoutSignal();
    // Folder and collection auth and headers (Digest and AWS below read the auth too).
    source = inheritDefaults(source, this.collection(collectionId));
    let prepared;
    try {
      prepared = await this.prepare(source, collectionId, signal);
      // A URL whose variables leave it without a scheme gets one too; never a relative fetch.
      prepared = { ...prepared, url: (await loadFormat()).withScheme(prepared.url) };
    } catch (e) {
      e.stage = "prepare"; // a variable or pre-request script error: nothing was sent
      throw e;
    }
    const headers = [...prepared.headers];
    const hasHeader = (name) => headers.some((h) => h.name.toLowerCase() === name && h.value);
    let body = prepared.body;
    const mode = source.bodyMode || "raw";
    const scriptChangedBody = prepared.body !== (source.body ? interpolateVariables(source.body, this.variables(collectionId)) : source.body);
    if (BODY_METHODS.includes(source.method) && mode !== "raw" && !scriptChangedBody) {
      const variables = this.variables(collectionId);
      const fields = (source.formFields || []).map((f) => ({
        ...f,
        key: f.key ? interpolateVariables(f.key, variables) : "",
        value: f.type === "file" ? f.value : interpolateVariables(f.value || "", variables),
      }));
      const built = mode === "multipart" ? await multipartBody(fields, this.files) : urlEncodedBody(fields);
      body = built.body;
      if (!hasHeader("content-type")) headers.push({ name: "Content-Type", value: built.contentType });
    } else if (BODY_METHODS.includes(source.method) && body && typeof body === "string" && !hasHeader("content-type")) {
      // A raw body says what it is, unless the request does (GraphQL already has its own):
      // hitop-core's rules (spec/vectors/send.json), as the CLI, MCP and HitopKit send it.
      const type = (await loadFormat()).rawContentType(body);
      if (type) headers.push({ name: "Content-Type", value: type });
    }
    // AWS Signature v4 signs the request as it goes out (headers and body final).
    if (source.auth?.type === "awsv4") {
      const vars = this.variables(collectionId);
      const fill = (t) => interpolateVariables(t || "", vars);
      const aws = { accessKey: fill(source.auth.accessKey), secretKey: fill(source.auth.secretKey), sessionToken: fill(source.auth.sessionToken), region: fill(source.auth.region), service: fill(source.auth.service) };
      if (!aws.accessKey || !aws.secretKey || !aws.region || !aws.service) {
        const e = new Error("AWS Signature: give the access key, secret key, region and service");
        e.stage = "prepare";
        throw e;
      }
      const signed = await awsV4Headers(aws, source.method, prepared.url, headers, BODY_METHODS.includes(source.method) ? body : "");
      headers.splice(0, headers.length, ...signed.headers);
    }
    const p = platform();
    const proxied = p.usesProxy(prepared.url);
    const jar = p.managesCookies(prepared.url);
    if (jar && this.cookieJarEnabled && !hasHeader("cookie")) {
      const cookie = cookieHeader(this.cookies, prepared.url);
      if (cookie) headers.push({ name: "Cookie", value: cookie });
    }
    const options = { method: source.method, headers: headersObject(headers), ...(p.cookies ? { cookies: this.cookieJarEnabled } : {}) };
    if (signal) options.signal = signal;
    const limit = requestTimeout(source, this.settings);
    if (limit) options.timeoutMs = limit;
    // The request's Settings: "Follow redirects" off answers with the redirect itself (the web
    // proxy and the desktop; a direct browser fetch can only stop at it without its details).
    if (source.settings?.followRedirects === false) {
      options.followRedirects = false;
      if (!proxied && p.name === "web") options.redirect = "manual";
    }
    if (body && BODY_METHODS.includes(source.method)) options.body = body;
    let res;
    try {
      if (onText && live && p.stream) res = await p.stream(prepared.url, options, onText);
      else {
        res = await p.send(prepared.url, onText && p.stream ? { ...options, eventStreams: true } : options);
        if (onText && p.stream && /text\/event-stream/i.test(res.headers?.get?.("content-type") ?? "")) {
          if (res.eventStream) {
            onStream();
            res = await p.stream(prepared.url, options, onText);
          } else if (p.streamFrom && typeof Response !== "undefined" && res instanceof Response) {
            onStream();
            res = await p.streamFrom(res, options, onText);
          }
        }
      }
    } catch (e) {
      if (signal?.aborted) throw signal.reason;
      // fetch says only "Failed to fetch"; for direct requests the usual cause
      // is a server that doesn't allow CORS.
      if (p.name === "web" && !proxied && e instanceof TypeError)
        e.message = "Couldn't reach the server. Requests to localhost and private networks go straight from your browser, so the server must allow CORS from this site; the desktop and mobile apps don't need it.";
      throw e;
    }
    // Digest: the server's 401 challenge answered once.
    const challenge = source.auth?.type === "digest" && res.status === 401 ? parseDigestChallenge(res.headers.get("www-authenticate")) : null;
    if (challenge) {
      const vars = this.variables(collectionId);
      const fill = (t) => interpolateVariables(t || "", vars);
      const u = new URL(prepared.url);
      const cnonce = [...crypto.getRandomValues(new Uint8Array(8))].map((b) => b.toString(16).padStart(2, "0")).join("");
      const value = await digestAnswer(challenge, fill(source.auth.username), fill(source.auth.password), source.method, `${u.pathname}${u.search}`, cnonce);
      headers.splice(0, headers.length, ...headers.filter((h) => h.name.toLowerCase() !== "authorization"), { name: "Authorization", value });
      options.headers = headersObject(headers);
      res = await p.send(prepared.url, options);
    }
    if (Array.isArray(res.cookies)) this.cookies = res.cookies;
    // Cookies set by redirects on the way (a login's 302) go in the jar too, each for its own URL.
    if (jar && this.cookieJarEnabled && Array.isArray(res.redirects)) {
      for (const hop of res.redirects) if (hop.setCookies?.length) this.cookies = mergeCookies(this.cookies, hop.setCookies, hop.url);
    }
    if (jar && this.cookieJarEnabled && res.headers.pairs) {
      const lines = res.headers.pairs.filter(([k]) => k.toLowerCase() === "set-cookie").map(([, v]) => v);
      if (lines.length) this.cookies = mergeCookies(this.cookies, lines, res.url || prepared.url);
    }
    return { prepared: { ...prepared, headers }, res };
  }

  async send() {
    const index = this.currentTab;
    let tab = this.tabs[index];
    if (!tab.url.trim()) {
      this.urlErrors.set(tab.id, "Enter a URL, such as https://api.example.com/users");
      this.emit();
      this.notify("Enter a URL first.");
      return;
    }
    this.refValues = {};
    if (tab.kind === "websocket") return this.wsConnect();
    if (tab.kind === "grpc") return this.grpcCall();
    // "api.example.com/users" is sent (and kept) as https://api.example.com/users;
    // a URL with variables is checked once they are filled in (dispatch).
    if (!tab.url.includes("${")) {
      try {
        const url = (await loadFormat()).withScheme(tab.url);
        if (url !== tab.url) {
          this.tabs = tabService.updateTab(this.tabs, index, { url });
          tab = this.tabs[index];
        }
      } catch (e) {
        this.urlErrors.set(tab.id, e.message);
        this.emit();
        return;
      }
    }
    const source = {
      url: tab.url,
      method: tab.method,
      headers: tab.headers,
      body: tab.requestBody,
      preRequestScript: tab.preRequestScript,
      auth: tab.auth,
      ...SAVED_FIELDS(tab),
      // Its folder, for the folder's auth and headers.
      file: this.loadedRequest(tab)?.file,
    };
    this.notSent.delete(tab.id);
    this.sentPlugins.delete(tab.id);
    this.tabs = tabService.updateTab(this.tabs, index, { ...NO_RESPONSE, loading: true });
    // An event stream (Accept: text/event-stream, web app): its events as they arrive, in memory
    // (store.events by tab id), until it ends or Stop; no time limit.
    // UX2 (8): without that header an answer with Content-Type text/event-stream turns into one
    // too (dispatch `onStream`), so the Events view never waits on a body that doesn't end.
    let streaming = Boolean(platform().stream) && asksForEvents(tab.headers);
    this.events ??= new Map();
    this.events.delete(tab.id);
    let live = null;
    const openLive = () => {
      if (live) return;
      live = { events: [], open: true, rest: "" };
      this.events.set(tab.id, live);
      this.emit();
    };
    if (streaming) openLive();
    const onText = platform().stream
      ? (piece) => {
          openLive();
          const { events, rest } = parseEvents(live.rest + piece);
          live.rest = rest;
          if (!events.length) return;
          const at = Date.now();
          live.events = [...live.events, ...events.map((e) => ({ ...e, at }))].slice(-1000);
          this.version = (this.version ?? 0) + 1;
          this.dispatchEvent(new Event("change"));
        }
      : undefined;
    // Cancel (store.cancel) and the request timeout (Settings) abort it.
    const call = this.startCall(tab.id, { timeout: !streaming, ms: requestTimeout(source, this.settings) });
    const onStream = () => {
      streaming = true;
      call.keepOpen();
      openLive();
    };
    this.emit();
    const started = performance.now();
    try {
      const { prepared, res } = await this.dispatch(source, tab.loadedCollectionId, { signal: call.signal, onText, live: streaming, onStream });
      if (live) live.open = false;
      const headers = {};
      res.headers.forEach((v, k) => (headers[k] = v));
      const { data, type, raw, jsonError, size } = await readResponse(res);
      const durationMs = res.timeMs ?? Math.round(performance.now() - started);
      // The body as received, for Raw (memory only: the tab keeps the formatted one).
      if (raw !== undefined) this.rawBodies.set(tab.id, raw);
      else this.rawBodies.delete(tab.id);
      // The plugin answers that went out (memory only, masked; threat model part 3 decision 1).
      if (prepared.plugins?.length) this.sentPlugins.set(tab.id, prepared.plugins);
      else this.sentPlugins.delete(tab.id);
      this.patchTab(tab.id, {
        response: data,
        responseType: type,
        jsonError: jsonError ?? null,
        statusCode: res.status,
        responseHeaders: headers,
        durationMs,
        // DNS, connect, waiting, download (hitop-core timing.rs: the desktop and the web proxy).
        timings: res.timings ?? null,
        // The response's HTTP version and the final URL's scheme, when the engine (desktop, web
        // proxy) said; a direct browser fetch can't see the version, so both stay null.
        httpVersion: res.httpVersion ?? null,
        redirects: res.redirects?.length ? res.redirects : null,
        responseScheme: res.httpVersion ? (/^([a-z][a-z0-9+.-]*):/i.exec(res.url || "")?.[1]?.toLowerCase() ?? null) : null,
        truncated: Boolean(res.truncated),
        responseSize: type === "binary" ? size : null,
        loading: false,
      });
      // UX2 (8): an event stream sent without `Accept: text/event-stream` came back whole (the
      // proxy or engine read it until its limit): its events from the body, shown in Events.
      if (!streaming && /text\/event-stream/i.test(headers["content-type"] ?? "") && typeof data === "string") {
        const at = Date.now();
        const { events } = parseEvents(`${data}\n\n`);
        if (events.length) this.events.set(tab.id, { events: events.map((e) => ({ ...e, at })), open: false, rest: "", fromBody: true });
      }
      this.history = historyService.addToHistory(
        this.history,
        historyService.createHistoryItem({
          url: tab.url,
          method: tab.method,
          headers: tab.headers,
          requestBody: tab.requestBody,
          statusCode: res.status,
          preRequestScript: tab.preRequestScript,
          postRequestScript: tab.postRequestScript,
        }),
      );
      this.history[this.history.length - 1].auth = cleanAuth(tab.auth);
      this.history[this.history.length - 1].response = maskSecrets(snapshotResponse({ status: res.status, headers, body: data, durationMs }), this.secretValuesFor(tab.loadedCollectionId));
      // Results the script got before it threw are kept; console.* output goes to Console.
      const post = await this.runPostScriptFull(tab.postRequestScript, tab.loadedCollectionId, data, headers, res.status, durationMs, { ...prepared, method: tab.method }).catch((e) => ({ tests: [], logs: [], error: e.message }));
      const tests = post.error ? [...post.tests, { name: "Script after", pass: false, detail: post.error }] : post.tests;
      if (post.error) this.notify(`The script after the request stopped: ${post.error}`);
      const consoleLogs = [...(prepared.logs ?? []).map((l) => ({ ...l, phase: "before" })), ...post.logs.map((l) => ({ ...l, phase: "after" }))];
      this.patchTab(tab.id, { tests, consoleLogs });
    } catch (e) {
      if (call.signal.aborted && call.signal.reason?.name !== "TimeoutError") {
        // Cancelled: no response, nothing in history.
        this.patchTab(tab.id, { response: null, responseType: "cancelled", statusCode: null, responseHeaders: null, loading: false });
      } else if (call.signal.aborted) {
        const limit = seconds(requestTimeout(source, this.settings));
        const own = Number(source.settings?.timeoutMs) > 0;
        this.patchTab(tab.id, { response: `Error: No response within ${limit}. ${own ? "This request's time limit is in its Settings tab." : "The time limit is in Settings, under Requests; a request can have its own in its Settings tab."}`, responseType: "error", statusCode: null, responseHeaders: null, loading: false });
      } else if (e.stage === "prepare") {
        this.patchTab(tab.id, { loading: false });
        // U19: the response pane keeps the note until the next send (memory only): a secret with
        // no value, a variable nothing defines, or the script before the request stopping.
        const vars = /^Variables? (.+) not found in variables object$/.exec(e.message);
        const names = vars ? [...vars[1].matchAll(/'([^']+)'/g)].map((m) => m[1]) : [];
        if (e.missingSecrets) this.notSent.set(tab.id, { text: e.message, missing: e.missingSecrets });
        else if (names.length) {
          const list = names.map((n) => `\${${n}}`).join(", ");
          this.notSent.set(tab.id, { text: `${list} ${names.length === 1 ? "has" : "have"} no value: no environment, collection variable or global defines ${names.length === 1 ? "it" : "them"}.`, variables: names, collectionId: tab.loadedCollectionId });
        } else this.notSent.set(tab.id, { text: e.message, script: Boolean(source.preRequestScript?.trim()) });
        this.notify(`The request wasn't sent: ${this.notSent.get(tab.id).text}`);
      } else {
        this.patchTab(tab.id, { response: `Error: ${e.message}`, responseType: "error", statusCode: null, responseHeaders: null, loading: false });
      }
    } finally {
      call.done();
      const live = this.events?.get(tab.id);
      if (live) live.open = false;
    }
    this.emit();
  }

  // A request in flight for this tab: { signal, done() }. The signal aborts on
  // Cancel (AbortError) or after the Settings time limit (TimeoutError).
  startCall(tabId, { timeout = true, ms: limit = this.settings.timeoutMs } = {}) {
    const controller = new AbortController();
    const ms = timeout ? limit : 0;
    let timer = ms > 0 ? setTimeout(() => controller.abort(new DOMException(`No response within ${seconds(ms)}`, "TimeoutError")), ms) : null;
    this.inflight.set(tabId, controller);
    // When it started, for the response pane's "Waiting · 1.4 s" (UX2).
    (this.callStarts ??= new Map()).set(tabId, Date.now());
    return {
      signal: controller.signal,
      // It turned into an event stream: no time limit any more.
      keepOpen: () => {
        clearTimeout(timer);
        timer = null;
      },
      done: () => {
        clearTimeout(timer);
        if (this.inflight.get(tabId) === controller) this.inflight.delete(tabId);
      },
    };
  }

  // Stops the tab's request in flight (the Cancel button).
  cancel(tabId = this.tab?.id) {
    this.inflight.get(tabId)?.abort(new DOMException("Cancelled", "AbortError"));
  }

  // The Settings time limit as a signal, for requests with no Cancel (runs, schema).
  timeoutSignal() {
    const ms = this.settings.timeoutMs;
    return ms > 0 && typeof AbortSignal.timeout === "function" ? AbortSignal.timeout(ms) : undefined;
  }

  setSettings(changes) {
    this.settings = { ...this.settings, ...changes };
    saveSettings(this.settings);
    this.emit();
  }
  };
