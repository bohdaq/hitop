// The store's scripts (a part of Store, src/store.js).
import contextService from "../../../shared/services/contextService.js";
import { applyAuth, oauthAuthorizeUrl, oauthCodeRequest, oauthParseToken, oauthTokenRequest, pkcePair } from "../features.js";
import { applyPathVariables, inheritDefaults } from "../defaults.js";
import { fillVariables, usesPostmanApi } from "../pm.js";
import { interpolateVariables } from "../../../shared/services/variableInterpolation.js";
import { isGraphql } from "./shared.js";
import { loadFormat } from "../wasm/format.js";
import { platform } from "../platform/index.js";
import { withPluginValues } from "../plugin-calls.js";

export const ScriptsPart = (Base) =>
  class extends Base {
  setContext(collectionId, key, value) {
    this.contexts = contextService.updateCollectionContext(this.contexts, collectionId, key, value);
  }

  // Scripts may be someone else's (a shared or imported collection): they run isolated from this
  // page, in the adapter's runner, and there is no other way to run one.
  async runPreScript(script, collectionId, method, url, headers, body) {
    if (!script?.trim()) return { url, headers, body };
    const context = contextService.getCollectionContext(this.contexts, collectionId);
    const variables = this.variables(collectionId);
    const r = await platform().scripts.pre({ script, url, method, headers, body, variables, context, globals: this.globalsFor(collectionId) });
    for (const [k, v] of Object.entries(r.contextUpdates)) this.setContext(collectionId, k, v);
    this.setScriptVariables(collectionId, r.variables);
    this.setScriptGlobals(collectionId, r.globals);
    return { url: r.url, headers: r.headers, body: r.body, logs: r.logs ?? [] };
  }

  // The agent's dry run (HITOP Next A7): `script` as the script after the request, run against the
  // current tab's last response. Nothing it sets (context, variables, globals) is kept.
  async dryRunChecks(script) {
    const t = this.tab;
    if (t?.statusCode == null || !script?.trim()) return null;
    const saved = { contexts: this.contexts, vars: structuredClone(this.scriptVariables), globals: structuredClone(this.scriptGlobals) };
    try {
      const raw = this.rawBodies.get(t.id);
      const text = typeof raw === "string" ? raw : typeof t.response === "string" ? t.response : JSON.stringify(t.response ?? "");
      return await this.runPostScript(script, t.loadedCollectionId, text, t.responseHeaders || {}, t.statusCode, t.durationMs ?? 0, { url: t.url, method: t.method, headers: t.headers, body: t.requestBody });
    } catch (e) {
      return [...(e.tests ?? []), { name: "Script after", pass: false, detail: e.message }];
    } finally {
      this.contexts = saved.contexts;
      this.scriptVariables = saved.vars;
      this.scriptGlobals = saved.globals;
    }
  }

  // Returns the assert.* results (an empty list without a script).
  // `sent` is the request as sent ({url, method, headers, body}), pm.request.
  // A script that throws: an Error whose `tests` are the results it got before (kept, not lost)
  // and whose message says where it stopped (line, column).
  async runPostScript(script, collectionId, text, headers, status, durationMs, sent) {
    const r = await this.runPostScriptFull(script, collectionId, text, headers, status, durationMs, sent);
    if (r.error) throw Object.assign(new Error(r.error), { tests: r.tests, logs: r.logs });
    return r.tests;
  }

  // {tests, logs, error}: the results, what console.* printed, and why it stopped (or null).
  async runPostScriptFull(script, collectionId, text, headers, status, durationMs, sent) {
    if (!script?.trim()) return { tests: [], logs: [], error: null };
    const r = await platform().scripts.post({ script, text, headers, status, durationMs, sent, variables: this.variables(collectionId), context: contextService.getCollectionContext(this.contexts, collectionId), globals: this.globalsFor(collectionId) });
    for (const [k, v] of Object.entries(r.contextUpdates ?? {})) this.setContext(collectionId, k, v);
    this.setScriptVariables(collectionId, r.variables ?? {});
    this.setScriptGlobals(collectionId, r.globals ?? {});
    // A script that threw answers with `error` and what it did before.
    return { tests: r.tests ?? [], logs: r.logs ?? [], error: r.error ?? null };
  }

  // Interpolates variables, adds auth and runs the pre-request script: the
  // request as sent. Auth comes before the script, so a script can override it.
  async prepare(source, collectionId, signal) {
    source = inheritDefaults(source, this.collection(collectionId));
    const answers = await this.resolveRefs(source, collectionId);
    const variables = withPluginValues(this.variables(collectionId), answers);
    // A Postman script may set variables this request uses: the ones still
    // missing are filled in after it runs.
    const postman = usesPostmanApi(source.preRequestScript);
    // U19: a secret with no value on this device stops the send with a note saying where to fill
    // it in (after a Postman pre-script, which may set it, below).
    if (!postman) this.checkSecrets(source, collectionId, variables);
    const interpolate = postman ? (text) => fillVariables(text, variables) : (text) => interpolateVariables(text, variables);
    const url = applyPathVariables(interpolate(source.url), source.pathVariables, interpolate);
    const headers = (source.headers || []).map((h) => ({
      name: h.name ? interpolate(h.name) : "",
      value: h.value ? interpolate(h.value) : "",
    }));
    // GraphQL: the JSON body hitop-core builds (hitop-wasm), from the query,
    // variables and operation name with variables filled in.
    const gql = isGraphql(source) ? { format: await loadFormat(), query: source.body || "", variables: source.graphql?.variables || "", operationName: source.graphql?.operationName || "" } : null;
    const graphqlBody = (fill) => gql.format.graphqlBody(fill(gql.query), fill(gql.variables), fill(gql.operationName));
    let body;
    if (gql) {
      try {
        body = graphqlBody(interpolate);
      } catch (e) {
        // Variables a Postman script is still to set may leave them unparsable for now.
        if (!postman || !/\$\{/.test(gql.variables)) throw e;
        body = gql.format.graphqlBody(interpolate(gql.query), "", interpolate(gql.operationName));
      }
      if (!headers.some((h) => h.name.toLowerCase() === "content-type" && h.value)) headers.push({ name: "Content-Type", value: "application/json" });
    } else {
      body = source.body ? interpolate(source.body) : source.body;
    }
    const auth = source.auth?.type === "oauth2" ? await this.oauthBearer(source.auth, interpolate, signal) : source.auth;
    const authed = applyAuth(auth, { url, headers, body }, interpolate);
    const out = await this.runPreScript(source.preRequestScript, collectionId, source.method, authed.url, authed.headers, authed.body);
    if (!postman) return out;
    const after = withPluginValues(this.variables(collectionId), answers);
    this.checkSecrets(source, collectionId, after);
    const strict = (text) => (text ? interpolateVariables(text, after) : text);
    const sentBody = gql && out.body === authed.body ? graphqlBody(strict) : strict(out.body);
    return { url: strict(out.url), headers: out.headers.map((h) => ({ ...h, name: strict(h.name), value: strict(h.value) })), body: sentBody, logs: out.logs };
  }

  checkSecrets(source, collectionId, variables) {
    const { missing, text } = this.missingSecretsFor(source, collectionId, variables);
    if (!missing.length) return;
    const e = new Error(text);
    e.missingSecrets = missing;
    throw e;
  }

  // OAuth 2.0: a Bearer preset with a token kept in memory until 30 s before it runs out.
  async oauthBearer(auth, fill, signal) {
    if (auth.grant === "authorization_code") return this.oauthSignIn(auth, fill, signal);
    const req = oauthTokenRequest(auth, fill);
    this.oauthTokens ??= new Map();
    const now = Date.now() / 1000;
    const have = this.oauthTokens.get(req.key);
    if (have && (have.until == null || now < have.until)) return { type: "bearer", token: have.token };
    const res = await platform().send(req.url, { method: "POST", headers: req.headers, body: req.body });
    const { token, lifetime } = oauthParseToken(res.status, await res.text());
    this.oauthTokens.set(req.key, { token, until: lifetime ? now + Math.max(0, lifetime - 30) : null });
    return { type: "bearer", token };
  }

  // OAuth 2.0 authorization code with PKCE (web app): the provider's sign-in page in a popup,
  // the code back through /app/oauth-callback.html (postMessage, this origin), traded through the
  // proxy. The token is kept in memory (a refresh token renews it); a page reload signs in again.
  async oauthSignIn(auth, fill, signal) {
    this.oauthTokens ??= new Map();
    // With the token URL: a refresh token goes only where it came from.
    const key = `code\n${fill(auth.authUrl || "")}\n${fill(auth.tokenUrl || "")}\n${fill(auth.clientId || "")}\n${fill(auth.scope || "")}`;
    const now = Date.now() / 1000;
    const have = this.oauthTokens.get(key);
    if (have && (have.until == null || now < have.until)) return { type: "bearer", token: have.token };
    const exchange = async (body, headers, url) => {
      const res = await platform().send(url, { method: "POST", headers, body });
      const text = await res.text();
      const { token, lifetime } = oauthParseToken(res.status, text);
      let refresh = null;
      try {
        refresh = JSON.parse(text).refresh_token ?? null;
      } catch {
        // no refresh token
      }
      this.oauthTokens.set(key, { token, refresh: refresh ?? have?.refresh ?? null, until: lifetime ? now + Math.max(0, lifetime - 30) : null });
      return { type: "bearer", token };
    };
    // Expired: the refresh token first.
    if (have?.refresh) {
      try {
        const base = oauthCodeRequest(auth, fill, "", "", "");
        const params = { grant_type: "refresh_token", refresh_token: have.refresh, client_id: fill(auth.clientId || "") };
        if (auth.clientAuth === "body" && auth.clientSecret) params.client_secret = fill(auth.clientSecret);
        const body = new URLSearchParams(params).toString();
        return await exchange(body, base.headers, base.url);
      } catch {
        // sign in again below
      }
    }
    // Desktop: the system browser, the code back on a loopback address (Rust).
    if (platform().oauthSignIn) {
      const filled = Object.fromEntries(Object.entries(auth).map(([k, v]) => [k, typeof v === "string" ? fill(v) : v]));
      const { token, lifetime, refresh } = await platform().oauthSignIn(filled, signal);
      this.oauthTokens.set(key, { token, refresh: refresh ?? null, until: lifetime ? Date.now() / 1000 + Math.max(0, lifetime - 30) : null });
      return { type: "bearer", token };
    }
    const redirect = fill(auth.redirectUri || "") || `${location.origin}${import.meta.env.BASE_URL}oauth-callback.html`;
    const { verifier, challenge } = await pkcePair();
    const state = [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, "0")).join("");
    const popup = window.open(oauthAuthorizeUrl(auth, fill, redirect, challenge, state), `hitop-oauth-${state.slice(0, 8)}`, "popup,width=520,height=720");
    if (!popup) throw new Error("OAuth 2.0: allow pop-ups for HITOP to sign in, then send again");
    const code = await new Promise((resolve, reject) => {
      const done = (fn, v) => {
        removeEventListener("message", on);
        clearInterval(watch);
        signal?.removeEventListener("abort", cancelled);
        fn(v);
      };
      const on = (e) => {
        if (e.origin !== location.origin || !e.data?.hitopOAuth) return;
        const m = e.data.hitopOAuth;
        // Another request's window (each has its own state).
        if (m.state && m.state !== state) return;
        if (m.error) done(reject, new Error(`OAuth 2.0: the sign-in was refused: ${m.error}`));
        else if (m.state !== state) done(reject, new Error("OAuth 2.0: the sign-in answered for another request"));
        else done(resolve, m.code);
      };
      // Cancel closes the window; the send's time limit doesn't run while a person signs in.
      const cancelled = () => {
        if (signal.reason?.name === "TimeoutError") return;
        popup.close();
        done(reject, signal.reason);
      };
      signal?.addEventListener("abort", cancelled);
      if (signal?.aborted) return cancelled();
      addEventListener("message", on);
      const watch = setInterval(() => popup.closed && done(reject, new Error("OAuth 2.0: the sign-in window was closed")), 500);
    });
    const req = oauthCodeRequest(auth, fill, code, verifier, redirect);
    return exchange(req.body, req.headers, req.url);
  }
  };
