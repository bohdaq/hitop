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
  constructor(server, session = null) {
    this.server = server;
    this.session = session;
  }

  async call(method, path, body) {
    const headers = {};
    if (this.session) headers.Authorization = `Bearer ${this.session}`;
    if (body !== undefined) headers["Content-Type"] = "application/json";
    const r = await fetch(this.server + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await r.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = null;
    }
    if (!r.ok) throw new ApiError(r.status, json);
    return json;
  }

  get(path) {
    return this.call("GET", path);
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

// --- single sign-on: PKCE verifier kept in this tab until the provider sends the browser back ---

export async function ssoStart(api, email) {
  const verifier = b64url(crypto.getRandomValues(new Uint8Array(32)));
  const challenge = b64url(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)));
  const returnTo = location.origin + location.pathname;
  const r = await api.call("POST", "/v1/auth/sso/start", { challenge, returnTo, ...(email ? { email } : {}) });
  sessionStorage.setItem("hitop_cloud_sso", JSON.stringify({ server: api.server, verifier }));
  return r.url;
}

export async function ssoFinish(code) {
  const saved = JSON.parse(sessionStorage.getItem("hitop_cloud_sso") || "null");
  sessionStorage.removeItem("hitop_cloud_sso");
  if (!saved) throw new Error("this single sign-on wasn't started in this tab; start again");
  const api = new Api(saved.server);
  return { server: saved.server, reply: await api.call("POST", "/v1/auth/sso/finish", { code, verifier: saved.verifier }) };
}
