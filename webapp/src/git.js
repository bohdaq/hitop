// HITOP Cloud Git sync, client side (sync-server gitsync.rs): one shared state for the sidebar
// icons, the collection page's storage chip, the "Save to GitHub" dialog and the Sync page.
// The server pushes every library save of a linked collection and pulls GitHub's commits back;
// this only links, reads states, and answers conflicts. `store.git` (main.js) is the instance.

import { IS_TAURI } from "./platform/index.js";

const PENDING_KEY = "hitop_git_pending"; // sessionStorage: the collection whose dialog to reopen after GitHub
const POLL_EVERY = 15_000;

export const slug = (name) =>
  String(name || "collection")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "collection";

export const STATE_WORDS = {
  ok: "In sync with GitHub",
  pending: "Saving to GitHub…",
  conflict: "Changed both here and on GitHub",
  error: "Couldn't sync",
  disconnected: "Disconnected",
};

// Short states for the collection page's chip.
const CHIP_WORDS = { pending: "saving…", conflict: "needs your choice", error: "couldn't sync", disconnected: "disconnected" };

/** hitop-core `format::names::slug`: lower case, letters and digits, other runs one `-`. */
function fileSlug(name) {
  let out = "";
  let dash = false;
  for (const ch of String(name ?? "").toLowerCase()) {
    if (/[\p{L}\p{N}]/u.test(ch)) {
      if (dash && out) out += "-";
      dash = false;
      out += ch;
    } else dash = true;
    if (out.length >= 80) break;
  }
  if (!out) return "request";
  return /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/.test(out) ? `${out}-file` : out;
}

/** hitop-core `assign_files`: each request's file (explicit `file` first, then numbered slugs). */
export function requestFiles(requests) {
  const used = new Set((requests ?? []).filter((r) => r.file).map((r) => String(r.file).toLowerCase()));
  return (requests ?? []).map((r) => {
    if (r.file) return `${r.file}.yaml`;
    const base = fileSlug(r.name);
    let candidate = base;
    for (let n = 2; used.has(candidate.toLowerCase()); n++) candidate = `${base}-${n}`;
    used.add(candidate.toLowerCase());
    return `${candidate}.yaml`;
  });
}

/** GitHub's own "new repository" page, filled in (the App can't create one in a personal account). */
export function newRepoUrl(info, name) {
  const base = info?.appUrl ? new URL(info.appUrl).origin : "https://github.com";
  const owner = info?.installations?.[0]?.account;
  const q = new URLSearchParams({ name, visibility: "private", description: "API collections kept by HITOP" });
  if (owner) q.set("owner", owner);
  return `${base}/new?${q}`;
}

/** `repo/path` (or `repo`) on `branch`. */
export const linkWhere = (l) => `${l.repo}${l.path ? `/${l.path}` : ""} · ${l.branch}`;

/** Sync time as "2 min ago". */
export function ago(secs) {
  if (!secs) return "";
  const s = Math.max(0, Math.round(Date.now() / 1000 - secs));
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return new Date(secs * 1000).toLocaleDateString();
}

export class GitState extends EventTarget {
  /** `library`: library.js Library (its client signs the calls; Git sync needs the library on). */
  constructor(library) {
    super();
    this.library = library;
    this.info = null; // GET /v1/git: {enabled, app, appUrl, installations: [{id, account}]}
    this.links = {}; // collection id → link (GET /v1/library `git`)
    this.repos = null; // [{fullName, defaultBranch, private, installation}] over every installation
    this.details = {}; // collection id → GET .../git (conflicts) while in conflict
    this.returned = null; // back from GitHub: {ok, reason, collection}
    this.timer = 0;
  }

  get enabled() {
    return Boolean(this.info?.enabled && this.library?.status.on);
  }

  get connected() {
    return this.enabled && this.info.installations?.length > 0;
  }

  api(method, path, body) {
    return this.library.client.api(method, path, body);
  }

  changed() {
    this.dispatchEvent(new Event("change"));
  }

  /** Reads the server's answer after GitHub (#git=...) and starts polling. */
  start() {
    const m = /^#git=([a-z]+)(?:&reason=([a-z_]+))?/.exec(location.hash);
    if (m) {
      history.replaceState(history.state, "", location.pathname + location.search);
      let collection = null;
      try {
        collection = sessionStorage.getItem(PENDING_KEY);
        sessionStorage.removeItem(PENDING_KEY);
      } catch {
        // storage blocked: the dialog just isn't reopened
      }
      this.returned = { ok: m[1] === "connected", reason: m[2] ?? "", collection };
    }
    // Signed in or out, library switched on or off: where collections are kept changes.
    this.wasOn = this.library.status.on;
    this.library.addEventListener("change", () => {
      const on = this.library.status.on;
      if (on === this.wasOn) return;
      this.wasOn = on;
      if (on) this.load();
      else this.changed();
    });
    this.load();
    this.timer = setInterval(() => document.visibilityState === "visible" && this.loadLinks(), POLL_EVERY);
  }

  async load() {
    if (!this.library?.status.on) return;
    try {
      this.info = await this.api("GET", "/v1/git");
    } catch {
      this.info = null;
    }
    if (this.info?.enabled) {
      await this.loadLinks();
      await this.loadRepos();
    }
    this.changed();
  }

  /** Repositories of every connected account, each with its installation. */
  async loadRepos() {
    const lists = await Promise.all(
      (this.info?.installations ?? []).map((i) =>
        this.api("GET", `/v1/git/installations/${i.id}/repos`)
          .then((r) => r.repositories.map((x) => ({ ...x, installation: i.id, account: i.account })))
          .catch(() => []),
      ),
    );
    this.repos = lists.flat().sort((a, b) => a.fullName.localeCompare(b.fullName));
    this.changed();
  }

  async loadLinks() {
    if (!this.enabled) return;
    const r = await this.api("GET", "/v1/library").catch(() => null);
    if (!r) return;
    this.links = r.git || {};
    for (const [id, l] of Object.entries(this.links)) {
      if (l.state === "conflict" && !this.details[id]) this.details = { ...this.details, [id]: await this.api("GET", `/v1/library/collection/${encodeURIComponent(id)}/git`) };
    }
    for (const id of Object.keys(this.details)) {
      if (this.links[id]?.state !== "conflict") {
        const { [id]: _gone, ...rest } = this.details;
        this.details = rest;
      }
    }
    this.changed();
  }

  linkOf(c) {
    return this.links[String(c.id)] ?? null;
  }

  /**
   * To GitHub: the App's install page (pick an account, or Configure an installation's
   * repositories) when `add` or nothing is installed yet, else GitHub's authorization screen.
   * `collection`: its dialog opens again when the browser comes back.
   */
  async connect({ add = false, collection = null } = {}) {
    const r = await this.api("POST", "/v1/git/github/connect", { returnTo: location.origin + location.pathname });
    try {
      if (collection != null) sessionStorage.setItem(PENDING_KEY, String(collection));
    } catch {
      // ignored
    }
    location.assign(add || !this.info.installations?.length ? r.installUrl : r.authorizeUrl);
  }

  async link(c, { repo, path, branch }) {
    const r = this.repos?.find((x) => x.fullName === repo);
    if (!r) throw new Error("Choose a repository first.");
    // The server links what the library holds: save this browser's last edits first.
    await this.library.flush();
    const l = await this.api("PUT", `/v1/library/collection/${encodeURIComponent(c.id)}/git`, {
      installation: r.installation,
      repo,
      path,
      ...(branch && branch !== r.defaultBranch ? { branch } : {}),
    });
    await this.library.pull();
    await this.loadLinks();
    return l;
  }

  async syncNow(c) {
    await this.library.flush();
    const l = await this.api("POST", `/v1/library/collection/${encodeURIComponent(c.id)}/git/sync`);
    await this.library.pull();
    await this.loadLinks();
    return l;
  }

  async unlink(c) {
    await this.api("DELETE", `/v1/library/collection/${encodeURIComponent(c.id)}/git`);
    await this.loadLinks();
  }

  async resolve(c, choices) {
    const l = await this.api("POST", `/v1/library/collection/${encodeURIComponent(c.id)}/git/resolve`, { choices });
    await this.library.pull();
    await this.loadLinks();
    return l;
  }
}

/**
 * Where a collection is kept, for the sidebar icon and the collection page's chip:
 * {kind: github|cloud|shared|folder|browser, icon, label, detail, attention}.
 */
export function storageOf(store, c) {
  const folder = store.folderOf?.(c.id);
  if (folder) {
    const shared = String(folder.ref ?? "").startsWith("hitop-sync:");
    return shared
      ? { kind: "shared", icon: "users-three", label: "Shared workspace", detail: "End-to-end encrypted, shared with your team through HITOP sync", attention: Boolean(folder.error) }
      : { kind: "folder", icon: "folder-open", label: folder.label, detail: `Saved to the folder ${folder.label}`, attention: Boolean(folder.error || !folder.connected) };
  }
  const lib = store.library;
  const local = new Set(lib?.state?.localOnly ?? []);
  if (!lib?.status.on || local.has(String(c.id))) {
    const web = !IS_TAURI;
    return {
      kind: "browser",
      icon: web ? "browser" : "desktop",
      label: web ? "This browser only" : "This computer only",
      detail: `${web ? "Kept in this browser only" : "Kept on this computer only"}${lib?.status.on ? "" : ". Sign in on the Sync page to save it to HITOP Cloud"}`,
      attention: false,
    };
  }
  const l = store.git?.linkOf(c);
  if (l) {
    const state = l.state === "ok" ? `synced ${ago(l.syncedAt)}` : (CHIP_WORDS[l.state] ?? l.state);
    return { kind: "github", icon: "github-logo", label: linkWhere(l), detail: `GitHub ${linkWhere(l)}, ${state}`, state, link: l, attention: l.state === "conflict" || l.state === "error" || l.state === "disconnected" };
  }
  return { kind: "cloud", icon: "cloud", label: "HITOP Cloud", detail: "Saved to HITOP Cloud, on every device you sign in on", attention: false };
}
