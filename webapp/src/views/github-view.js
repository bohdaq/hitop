// HITOP Cloud Git sync (sync-server gitsync.rs) on the Sync page: connect GitHub through the HITOP
// Cloud App, link a library collection to a folder of a repository, and see each link's state.
// Every save is pushed by the server; GitHub's changes come back into the library. A request
// changed on both sides is shown here with both versions: keep mine, take GitHub's, or keep both.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { icon } from "../icons.js";

const slug = (name) =>
  String(name || "collection")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "collection";

const STATE_WORDS = {
  ok: "In sync with GitHub",
  pending: "Saving to GitHub…",
  conflict: "Changed both here and on GitHub",
  error: "Couldn't sync",
  disconnected: "Disconnected",
};

class HitopGithub extends LitElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    info: { state: true },
    links: { state: true },
    repos: { state: true },
    linking: { state: true },
    details: { state: true },
    choices: { state: true },
    busy: { state: true },
    error: { state: true },
    note: { state: true },
  };

  constructor() {
    super();
    this.info = null;
    this.links = {};
    this.repos = null;
    this.linking = null;
    this.details = {};
    this.choices = {};
    this.busy = "";
    this.error = "";
    this.note = "";
  }

  createRenderRoot() {
    return this;
  }

  get lib() {
    return this.store?.library;
  }

  api(method, path, body) {
    return this.lib.client.api(method, path, body);
  }

  connectedCallback() {
    super.connectedCallback();
    // Back from GitHub: #git=connected or #git=error&reason=...
    const m = /^#git=([a-z]+)(?:&reason=([a-z_]+))?/.exec(location.hash);
    if (m) {
      history.replaceState(null, "", location.pathname + location.search);
      if (m[1] === "connected") this.note = "GitHub is connected. Link a collection to a repository below.";
      else this.error = m[2] === "no_installation" ? "The HITOP Cloud App isn't installed on an account you can reach. Install it, then connect again." : "GitHub didn't finish connecting. Try again.";
    }
    this.load();
    this.timer = setInterval(() => document.visibilityState === "visible" && this.loadLinks(), 15_000);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    clearInterval(this.timer);
  }

  async run(what, fn) {
    this.busy = what;
    this.error = "";
    this.note = "";
    try {
      await fn();
    } catch (e) {
      this.error = e?.message || String(e);
    } finally {
      this.busy = "";
    }
  }

  async load() {
    if (!this.lib?.status.on) return;
    try {
      this.info = await this.api("GET", "/v1/git");
    } catch {
      this.info = null;
      return;
    }
    if (!this.info.enabled) return;
    await this.loadLinks();
    const inst = this.info.installations?.[0];
    if (inst && !this.repos) {
      this.repos = (await this.api("GET", `/v1/git/installations/${inst.id}/repos`).catch(() => ({ repositories: [] }))).repositories;
    }
  }

  async loadLinks() {
    if (!this.info?.enabled) return;
    const r = await this.api("GET", "/v1/library").catch(() => null);
    if (!r) return;
    this.links = r.git || {};
    for (const [id, l] of Object.entries(this.links)) {
      if (l.state === "conflict" && !this.details[id]) this.details = { ...this.details, [id]: await this.api("GET", `/v1/library/collection/${encodeURIComponent(id)}/git`) };
      if (l.state !== "conflict" && this.details[id]) {
        const { [id]: _gone, ...rest } = this.details;
        this.details = rest;
      }
    }
  }

  async connect() {
    await this.run("connect", async () => {
      const r = await this.api("POST", "/v1/git/github/connect", { returnTo: location.origin + location.pathname });
      // Already installed: GitHub's authorization screen; otherwise the App's install page.
      location.assign(this.info.installations?.length ? r.authorizeUrl : r.installUrl);
    });
  }

  collections() {
    const s = this.store;
    const localOnly = new Set(this.lib?.state?.localOnly ?? []);
    return s.collections.filter((c) => !s.folders.has(c.id) && !localOnly.has(String(c.id)));
  }

  async link(c) {
    await this.run(`link-${c.id}`, async () => {
      const repo = this.querySelector(`#gh-repo-${c.id}`)?.value || this.repos?.[0]?.fullName;
      const path = this.querySelector(`#gh-path-${c.id}`)?.value ?? slug(c.name);
      if (!repo) throw new Error("Choose a repository first (add one to the HITOP Cloud App on GitHub).");
      // The server links what the library holds: save this browser's last edits first.
      await this.lib.flush();
      const l = await this.api("PUT", `/v1/library/collection/${encodeURIComponent(c.id)}/git`, { installation: this.info.installations[0].id, repo, path });
      this.linking = null;
      await this.lib.pull();
      await this.loadLinks();
      this.note = l.state === "ok" ? `${c.name} is linked to ${l.repo}${l.path ? `/${l.path}` : ""}. Every save goes to GitHub.` : l.message;
    });
  }

  async syncNow(c) {
    await this.run(`sync-${c.id}`, async () => {
      await this.lib.flush();
      const l = await this.api("POST", `/v1/library/collection/${encodeURIComponent(c.id)}/git/sync`);
      await this.lib.pull();
      await this.loadLinks();
      if (l.state !== "ok") this.error = l.message;
    });
  }

  async unlink(c) {
    const ok = await confirm({ headline: `Unlink ${c.name} from GitHub?`, message: "Saves stop going to GitHub. The files stay in the repository.", confirmLabel: "Unlink" });
    if (!ok) return;
    await this.run(`unlink-${c.id}`, async () => {
      await this.api("DELETE", `/v1/library/collection/${encodeURIComponent(c.id)}/git`);
      await this.loadLinks();
    });
  }

  async resolve(c, conflicts) {
    const picks = this.choices[c.id] || {};
    await this.run(`resolve-${c.id}`, async () => {
      const l = await this.api("POST", `/v1/library/collection/${encodeURIComponent(c.id)}/git/resolve`, { choices: Object.fromEntries(conflicts.map((k) => [k.path, picks[k.path]])) });
      await this.lib.pull();
      await this.loadLinks();
      this.note = l.state === "ok" ? `${c.name} is in sync with GitHub again.` : l.message;
    });
  }

  pick(c, path, choice) {
    this.choices = { ...this.choices, [c.id]: { ...(this.choices[c.id] || {}), [path]: choice } };
  }

  renderConflicts(c) {
    const conflicts = this.details[c.id]?.conflicts ?? [];
    if (!conflicts.length) return nothing;
    const picks = this.choices[c.id] || {};
    const ready = conflicts.every((k) => picks[k.path]);
    return html`<ul class="sync-conflicts" aria-label=${`GitHub conflicts in ${c.name}`}>
        ${conflicts.map(
          (k) => html`<li class="sync-item">
            <b>${k.name}</b> <span class="hint">${k.path}</span>
            <div class="sync-compare">
              <div><span class="hint">Here</span><pre class="code-preview">${k.mine ?? "(deleted here)"}</pre></div>
              <div><span class="hint">On GitHub</span><pre class="code-preview">${k.theirs ?? "(deleted on GitHub)"}</pre></div>
            </div>
            <span class="page-actions" role="group" aria-label=${`Keep for ${k.name}`}>
              ${[
                ["mine", "Keep mine"],
                ["theirs", "Take GitHub's"],
                ...(k.path !== "hitop.yaml" && k.mine && k.theirs ? [["both", "Keep both"]] : []),
              ].map(([v, label]) => html`<lit-material-button variant=${picks[k.path] === v ? "filled" : "outlined"} aria-pressed=${picks[k.path] === v ? "true" : "false"} @click=${() => this.pick(c, k.path, v)}>${label}</lit-material-button>`)}
            </span>
          </li>`,
        )}
      </ul>
      <lit-material-button variant="filled" ?disabled=${!ready || Boolean(this.busy)} @click=${() => this.resolve(c, conflicts)}>Apply and sync</lit-material-button>`;
  }

  renderRow(c) {
    const l = this.links[String(c.id)];
    if (l) {
      const where = `${l.repo}${l.path ? `/${l.path}` : ""} on ${l.branch}`;
      return html`<li class="sync-item" aria-label=${`GitHub link of ${c.name}`}>
        <div class="sync-form">
          <span><b>${c.name}</b> <a href=${l.url} target="_blank" rel="noopener noreferrer">${where}</a></span>
          <span class="page-actions">
            <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.syncNow(c)}>${icon("arrows-clockwise")} Sync now</lit-material-button>
            <lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.unlink(c)}>Unlink</lit-material-button>
          </span>
        </div>
        <p class=${l.state === "ok" || l.state === "pending" ? "hint" : "sync-note"} role="status" aria-label=${`GitHub state of ${c.name}`}>
          ${l.state === "conflict" && l.message
            ? `${l.message}.`
            : html`${STATE_WORDS[l.state] ?? l.state}${l.state === "ok" && l.syncedAt ? `, last synced ${new Date(l.syncedAt * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : ""}${l.message ? `: ${l.message}` : "."}`}
        </p>
        ${l.state === "conflict" ? this.renderConflicts(c) : nothing}
      </li>`;
    }
    if (this.linking !== c.id) {
      return html`<li class="sync-item"><div class="sync-form"><span><b>${c.name}</b> <span class="hint">not on GitHub</span></span>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => (this.linking = c.id)}>Link to GitHub</lit-material-button></div></li>`;
    }
    return html`<li class="sync-item" aria-label=${`Link ${c.name} to GitHub`}>
      <div class="sync-form">
        <lit-material-select id=${`gh-repo-${c.id}`} variant="outlined" label="Repository" .value=${this.repos?.[0]?.fullName ?? ""}>
          ${(this.repos ?? []).map((r) => html`<lit-material-select-option value=${r.fullName}>${r.fullName}${r.private ? " (private)" : ""}</lit-material-select-option>`)}
        </lit-material-select>
        <lit-material-text-field id=${`gh-path-${c.id}`} variant="outlined" label="Folder in the repository" .value=${slug(c.name)} supporting-text="Empty for the repository's root. A folder that already holds a hitop collection wins the first sync."></lit-material-text-field>
        <span class="page-actions">
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.link(c)}>Link</lit-material-button>
          <lit-material-button variant="text" @click=${() => (this.linking = null)}>Cancel</lit-material-button>
        </span>
      </div>
      ${this.repos?.length ? nothing : html`<p class="hint">The App can't see any repository yet: choose repositories for it on GitHub.</p>`}
    </li>`;
  }

  render() {
    if (!this.info?.enabled || !this.lib?.status.on) return nothing;
    const connected = this.info.installations?.length > 0;
    return html`<section aria-label="GitHub">
      <h2>GitHub</h2>
      ${this.error ? html`<lit-material-alert variant="error" role="alert">${this.error}</lit-material-alert>` : nothing}
      ${this.note ? html`<p class="sync-note" role="status">${this.note}</p>` : nothing}
      ${connected
        ? html`<p class="hint">Linked collections are kept in a GitHub repository as hitop files (hitop.yaml and requests/, never secret values). Every save is pushed; your team's commits come back here.</p>
            <div class="sync-form">
              <span class="hint">Connected: <b>${this.info.installations.map((i) => i.account).join(", ")}</b></span>
              <lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.connect()}>Add repositories or accounts</lit-material-button>
            </div>
            <ul class="sync-list">${this.collections().map((c) => this.renderRow(c))}</ul>`
        : html`<p class="hint">Keep collections in a GitHub repository: review changes in pull requests and run them in CI. The HITOP Cloud App asks only for the repositories you choose.</p>
            <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.connect()}>Connect GitHub</lit-material-button>`}
    </section>`;
  }
}

customElements.define("hitop-github", HitopGithub);
