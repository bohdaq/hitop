// The signed-in account: the same localStorage record as before (`hitop_cloud`: server, session,
// email, expiresAt), so a session survives a portal release. One object the shell and the pages
// share; it fires `change` when the account changes and `busy` while requests are in flight.
import { Api, DEFAULT_SERVER } from "./api.js";

const STORE = "hitop_cloud";
const ORG = "hitop_cloud_org";
const NEXT = "hitop_cloud_next";

function load() {
  try {
    return JSON.parse(localStorage.getItem(STORE) || "null") || {};
  } catch {
    return {};
  }
}

function save(value) {
  try {
    if (value) localStorage.setItem(STORE, JSON.stringify(value));
    else localStorage.removeItem(STORE);
  } catch {
    // Storage refused (private window): the session lasts this page only.
  }
}

function stored(key) {
  try {
    return localStorage.getItem(key) || "";
  } catch {
    return "";
  }
}

function store(key, value) {
  try {
    if (value) localStorage.setItem(key, value);
    else localStorage.removeItem(key);
  } catch {
    // ignored
  }
}

class Session extends EventTarget {
  constructor() {
    super();
    this.account = load();
    this.pending = 0;
    // ?server=<url> picks another sync server (self-hosted, tests); the hosted one otherwise.
    const asked = new URLSearchParams(location.search).get("server");
    if (asked && asked !== this.account.server) {
      this.account = { server: asked };
      save(this.account);
    }
    this.orgId = stored(ORG);
  }

  get server() {
    return this.account.server || DEFAULT_SERVER;
  }

  get hosted() {
    return !this.account.server || this.account.server === DEFAULT_SERVER;
  }

  get email() {
    return this.account.email || "";
  }

  /** The signed-in server's API; counts its requests for the shell's busy state. */
  get api() {
    return new Api(this.server, this.account.session || null, this);
  }

  get signedIn() {
    return Boolean(this.account.session) && (!this.account.expiresAt || this.account.expiresAt * 1000 > Date.now());
  }

  signIn(server, reply) {
    this.account = { server, session: reply.session, email: reply.user.email, expiresAt: reply.expiresAt };
    save(this.account);
    this.emit();
  }

  /** Remembers the address for next time (before a code is sent). */
  remember(server, email) {
    this.account = { ...this.account, server, email };
    save(this.account);
  }

  signOut() {
    this.account = { server: this.account.server, email: this.account.email };
    save(this.account);
    this.emit();
  }

  /** The server said 401 for a session the portal still had: it ended elsewhere. */
  expire() {
    this.signOut();
  }

  /** Where to go after sign-in (the page a signed-out visitor asked for). */
  set next(path) {
    try {
      if (path && path !== "/sign-in") sessionStorage.setItem(NEXT, path);
      else sessionStorage.removeItem(NEXT);
    } catch {
      // ignored
    }
  }

  get next() {
    try {
      const v = sessionStorage.getItem(NEXT) || "";
      sessionStorage.removeItem(NEXT);
      return v;
    } catch {
      return "";
    }
  }

  setOrg(id) {
    if (id === this.orgId) return;
    this.orgId = id || "";
    store(ORG, this.orgId);
    this.emit();
  }

  busy(delta) {
    this.pending = Math.max(0, this.pending + delta);
    this.dispatchEvent(new Event("busy"));
  }

  emit() {
    this.dispatchEvent(new Event("change"));
  }
}

export const session = new Session();
