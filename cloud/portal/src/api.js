// The sync server's API, as the portal uses it: the same sign-in and sessions as every hitop
// client (no second identity system), plus /v1/orgs and /v1/me/tokens. The portal never handles
// a workspace key or a collection: it has none.

export const DEFAULT_SERVER = "https://sync.hitop.rws8.tech";

export class ApiError extends Error {
  constructor(status, body) {
    super(body?.message || `the sync server answered ${status}`);
    this.status = status;
    this.body = body;
  }
}

export function b64url(bytes) {
  let s = "";
  for (const b of new Uint8Array(bytes)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function unb64url(text) {
  const s = atob(String(text).replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(s, (c) => c.charCodeAt(0));
}

export function normalizeServer(url) {
  const u = new URL(String(url || DEFAULT_SERVER).trim());
  if (u.protocol !== "https:" && !["localhost", "127.0.0.1", "[::1]"].includes(u.hostname)) {
    throw new Error("the sync server must use https (http only for localhost)");
  }
  return u.origin;
}

export class Api {
  /** `tracker.busy(+1 | -1)` is told about every request in flight (the shell shows it). */
  constructor(server, session = null, tracker = null) {
    this.server = server;
    this.session = session;
    this.tracker = tracker;
  }

  async call(method, path, body, { signal } = {}) {
    const headers = {};
    if (this.session) headers.Authorization = `Bearer ${this.session}`;
    if (body !== undefined) headers["Content-Type"] = "application/json";
    this.tracker?.busy(1);
    try {
      const r = await fetch(this.server + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal });
      const text = await r.text();
      let json = null;
      try {
        json = text ? JSON.parse(text) : null;
      } catch {
        json = null;
      }
      if (!r.ok) throw new ApiError(r.status, json);
      return json;
    } finally {
      this.tracker?.busy(-1);
    }
  }

  get(path, options) {
    return this.call("GET", path, undefined, options);
  }

  /** A response body as a Blob (the signed audit export). */
  async blob(path) {
    this.tracker?.busy(1);
    try {
      const r = await fetch(this.server + path, { headers: this.session ? { Authorization: `Bearer ${this.session}` } : {} });
      if (!r.ok) throw new ApiError(r.status, await r.json().catch(() => null));
      return r.blob();
    } finally {
      this.tracker?.busy(-1);
    }
  }
}

// --- passkeys (WebAuthn): the server's options are webauthn-rs JSON with base64url fields ---

function creationOptions(o) {
  const pk = { ...o.publicKey };
  pk.challenge = unb64url(pk.challenge);
  pk.user = { ...pk.user, id: unb64url(pk.user.id) };
  pk.excludeCredentials = (pk.excludeCredentials || []).map((c) => ({ ...c, id: unb64url(c.id) }));
  return { publicKey: pk };
}

function requestOptions(o) {
  const pk = { ...o.publicKey };
  pk.challenge = unb64url(pk.challenge);
  pk.allowCredentials = (pk.allowCredentials || []).map((c) => ({ ...c, id: unb64url(c.id) }));
  return { publicKey: pk };
}

export async function registerPasskey(api, name) {
  const start = await api.call("POST", "/v1/auth/passkey/register/start");
  const cred = await navigator.credentials.create(creationOptions(start.options));
  const credential = {
    id: cred.id,
    rawId: b64url(cred.rawId),
    type: cred.type,
    extensions: {},
    response: { attestationObject: b64url(cred.response.attestationObject), clientDataJSON: b64url(cred.response.clientDataJSON) },
  };
  return api.call("POST", "/v1/auth/passkey/register/finish", { challengeId: start.challengeId, name, credential });
}

export async function passkeySignIn(api, email) {
  const start = await api.call("POST", "/v1/auth/passkey/login/start", { email });
  const cred = await navigator.credentials.get(requestOptions(start.options));
  const r = cred.response;
  const credential = {
    id: cred.id,
    rawId: b64url(cred.rawId),
    type: cred.type,
    extensions: {},
    response: {
      authenticatorData: b64url(r.authenticatorData),
      clientDataJSON: b64url(r.clientDataJSON),
      signature: b64url(r.signature),
      userHandle: r.userHandle ? b64url(r.userHandle) : null,
    },
  };
  return api.call("POST", "/v1/auth/passkey/login/finish", { challengeId: start.challengeId, credential });
}

// --- single sign-on, Google, GitHub: PKCE verifier kept in this tab until the provider sends
// the browser back (`kind` = sso | google | github | apple, the server's /v1/auth/<kind>/...) ---

export async function ssoStart(api, email, kind = "sso") {
  const verifier = b64url(crypto.getRandomValues(new Uint8Array(32)));
  const challenge = b64url(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)));
  const returnTo = location.origin + location.pathname;
  const r = await api.call("POST", `/v1/auth/${kind}/start`, { challenge, returnTo, ...(email && kind === "sso" ? { email } : {}) });
  sessionStorage.setItem("hitop_cloud_sso", JSON.stringify({ server: api.server, verifier, kind }));
  return r.url;
}

export async function ssoFinish(code) {
  const saved = JSON.parse(sessionStorage.getItem("hitop_cloud_sso") || "null");
  sessionStorage.removeItem("hitop_cloud_sso");
  if (!saved) throw new Error("this sign-in wasn't started in this tab; start again");
  const api = new Api(saved.server);
  return { server: saved.server, reply: await api.call("POST", `/v1/auth/${saved.kind || "sso"}/finish`, { code, verifier: saved.verifier }) };
}

// --- passwords (sync-server/THREAT_MODEL.md "Passwords"): the web app's hitop-sync-crypto build
// derives the keys (Argon2id in WASM, its own chunk, loaded on first use). The password never
// leaves this page; the server gets an auth key, and the portal opens and re-seals the key bundle
// only to change the password (it has no device key of its own). ---

let wasm = null;
async function crypto_() {
  wasm ??= import("../../../webapp/src/wasm/format.js").then((m) => m.loadFormat()).then((f) => f.sync);
  return wasm;
}

async function keysFor(api, email, password) {
  const { kdf } = await api.call("POST", "/v1/auth/password/params", { email });
  // Refuses parameters weaker than hitop's floor (a server can't weaken them).
  return (await crypto_()).passwordKeys(password, kdf);
}

export async function passwordSignIn(api, email, password) {
  const keys = await keysFor(api, email, password);
  return api.call("POST", "/v1/auth/password/login", { email, authKey: keys.authKey });
}

async function newKeys(password) {
  const w = await crypto_();
  w.passwordCheck(password);
  const kdf = w.passwordKdfNew();
  return { kdf, keys: w.passwordKeys(password, kdf) };
}

export async function passwordSignUp(api, email, password) {
  const { kdf, keys } = await newKeys(password);
  return api.call("POST", "/v1/auth/password/signup", { email, kdf, authKey: keys.authKey });
}

export async function passwordReset(api, token, password) {
  const { kdf, keys } = await newKeys(password);
  return api.call("POST", "/v1/auth/password/reset", { token, kdf, authKey: keys.authKey });
}

// Sets or changes the password of the signed-in account (`me` = GET /v1/me); the password's key
// bundle, when there is one, is opened with the current password and sealed under the new one.
export async function changePassword(api, me, current, next) {
  const w = await crypto_();
  const { kdf, keys } = await newKeys(next);
  const body = { kdf, authKey: keys.authKey };
  if (me.password?.set) {
    const cur = await keysFor(api, me.email, current);
    body.currentAuthKey = cur.authKey;
    if (me.bundles?.password) {
      const { blob } = await api.get("/v1/me/recovery?method=password");
      let text;
      try {
        text = w.bundleOpen(cur.bundleKey, me.id, blob);
      } catch {
        throw new Error("the current password is wrong (it doesn't open your key bundle)");
      }
      body.bundle = w.bundleSeal(keys.bundleKey, me.id, text);
    }
  }
  return api.call("POST", "/v1/me/password", body);
}
