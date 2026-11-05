// HITOP Cloud Git sync in the web app (state in ../git.js, server sync-server gitsync.rs).
// - <hitop-git-panel>: the "Save to GitHub" dialog of one collection (opened from its storage
//   chip on the collection page, or from the Sync page): connect GitHub, pick a repository and a
//   folder with a preview of the files; once linked, where it lives, its state, Sync now, Unlink,
//   and a clash shown side by side (keep mine, take GitHub's, keep both).
// - <hitop-github>: the Sync page's GitHub section: connected accounts, repository access, and
//   every collection with where it is kept.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { icon } from "../icons.js";
import { STATE_WORDS, ago, linkWhere, newRepoUrl, requestFiles, slug, storageOf } from "../git.js";

class LightElement extends LitElement {
  createRenderRoot() {
    return this;
  }
}

const openGitDialog = (el, c) => el.closest("hitop-app")?.openDialog("git", { collectionId: c.id });

class HitopGitPanel extends LightElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    collection: { attribute: false },
    repo: { state: true },
    branch: { state: true },
    path: { state: true },
    choices: { state: true },
    busy: { state: true },
    error: { state: true },
    note: { state: true },
    creating: { state: true },
  };

  constructor() {
    super();
    this.creating = null;
    this.repo = "";
    this.branch = "";
    this.path = null;
    this.choices = {};
    this.busy = "";
    this.error = "";
    this.note = "";
  }

  get git() {
    return this.store?.git;
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

  connectedCallback() {
    super.connectedCallback();
    const r = this.git?.returned;
    if (r && String(r.collection) === String(this.collection?.id)) {
      this.git.returned = null;
      if (r.ok) this.note = "GitHub is connected.";
      else this.error = r.reason === "no_installation" ? "The HITOP Cloud App isn't installed on an account you can reach. Install it, then try again." : "GitHub didn't finish connecting. Try again.";
    }
    // Fresh repositories each time (access may have changed on GitHub).
    if (this.git?.connected) this.git.loadRepos();
    // Back from GitHub's "new repository" tab: the list again, and the new one chosen.
    this.onFocus = () => this.creating && this.pickNew();
    addEventListener("focus", this.onFocus);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    removeEventListener("focus", this.onFocus);
  }

  async pickNew() {
    const before = this.creating;
    await this.git.loadRepos();
    const fresh = (this.git.repos ?? []).find((r) => !before.has(r.fullName));
    if (fresh) {
      this.creating = null;
      this.repo = fresh.fullName;
      this.branch = "";
      this.note = `${fresh.fullName} is chosen.`;
    }
  }

  createRepo() {
    this.creating = new Set((this.git.repos ?? []).map((r) => r.fullName));
    this.note = "";
    window.open(newRepoUrl(this.git.info, "hitop-collections"), "_blank", "noopener");
  }

  chosenRepo() {
    const repos = this.git?.repos ?? [];
    return repos.find((r) => r.fullName === this.repo) ?? repos[0] ?? null;
  }

  /** Enter in a field of the dialog (hitop-app's submitDialog). */
  submit() {
    if (!this.git?.linkOf(this.collection) && this.git?.connected) this.save();
  }

  async save() {
    const c = this.collection;
    const r = this.chosenRepo();
    await this.run("link", async () => {
      if (!r) throw new Error("Choose a repository first.");
      const l = await this.git.link(c, { repo: r.fullName, path: this.path ?? slug(c.name), branch: this.branch || r.defaultBranch });
      this.note = l.state === "ok" ? "Saved to GitHub." : l.message;
    });
  }

  async unlink() {
    const c = this.collection;
    const ok = await confirm({ headline: `Stop saving ${c.name} to GitHub?`, message: "Saves stay on HITOP Cloud and stop going to GitHub. The files stay in the repository.", confirmLabel: "Stop saving to GitHub" });
    if (!ok) return;
    await this.run("unlink", async () => {
      await this.git.unlink(c);
      this.note = `${c.name} is saved on HITOP Cloud only.`;
    });
  }

  async resolve(conflicts) {
    const c = this.collection;
    const picks = this.choices;
    await this.run("resolve", async () => {
      const l = await this.git.resolve(c, Object.fromEntries(conflicts.map((k) => [k.path, picks[k.path]])));
      this.choices = {};
      this.note = l.state === "ok" ? `${c.name} is in sync with GitHub again.` : l.message;
    });
  }

  renderConflicts() {
    const c = this.collection;
    const conflicts = this.git.details[String(c.id)]?.conflicts ?? [];
    if (!conflicts.length) return nothing;
    const picks = this.choices;
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
              ].map(([v, label]) => html`<lit-material-button variant=${picks[k.path] === v ? "filled" : "outlined"} aria-pressed=${picks[k.path] === v ? "true" : "false"} @click=${() => (this.choices = { ...picks, [k.path]: v })}>${label}</lit-material-button>`)}
            </span>
          </li>`,
        )}
      </ul>
      <lit-material-button variant="filled" ?disabled=${!ready || Boolean(this.busy)} @click=${() => this.resolve(conflicts)}>Apply and sync</lit-material-button>`;
  }

  renderLinked(l) {
    const c = this.collection;
    const words = STATE_WORDS[l.state] ?? l.state;
    return html`
      <div class="git-where" aria-label=${`GitHub link of ${c.name}`}>
        ${icon("github-logo")}
        <a href=${l.url} target="_blank" rel="noopener noreferrer">${linkWhere(l)} ${icon("arrow-square-out")}</a>
      </div>
      <p class=${l.state === "ok" || l.state === "pending" ? "hint" : "sync-note"} role="status" aria-label=${`GitHub state of ${c.name}`}>
        ${l.state === "conflict" && l.message ? `${l.message}.` : html`${words}${l.state === "ok" && l.syncedAt ? `, last synced ${ago(l.syncedAt)}` : ""}${l.message ? `: ${l.message}` : "."}`}
      </p>
      ${l.state === "conflict" ? this.renderConflicts() : nothing}
      <div class="page-actions">
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("sync", async () => {
          const r = await this.git.syncNow(c);
          if (r.state !== "ok" && r.state !== "conflict") this.error = r.message;
        })}>${icon("arrows-clockwise")} Sync now</lit-material-button>
        <lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.unlink()}>Stop saving to GitHub</lit-material-button>
      </div>
      <p class="hint">Every save in HITOP commits to this folder, and commits on GitHub come back within seconds. The rest of the repository is never touched, and secret values stay on HITOP Cloud.</p>`;
  }

  renderForm() {
    const c = this.collection;
    const g = this.git;
    const repos = g.repos ?? [];
    const r = this.chosenRepo();
    const path = this.path ?? slug(c.name);
    const prefix = path ? `${path}/` : "";
    const files = [`${prefix}hitop.yaml`, ...requestFiles(c.requests).map((f) => `${prefix}requests/${f}`)];
    const shown = files.slice(0, 6);
    return html`
      <ol class="git-steps">
        <li>
          <span class="git-step-title">GitHub</span>
          ${g.connected
            ? html`<span>${icon("check-circle")} Connected as <b>${g.info.installations.map((i) => i.account).join(", ")}</b></span>`
            : html`<span class="hint">HITOP asks GitHub only for the repositories you choose.</span>
                <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("connect", () => g.connect({ collection: c.id }))}>${icon("github-logo")} Connect GitHub</lit-material-button>`}
        </li>
        ${g.connected
          ? html`<li>
                <span class="git-step-title">Repository</span>
                ${repos.length
                  ? html`<lit-material-select id="git-repo" variant="outlined" label="Repository" .value=${r?.fullName ?? ""}
                      @change=${(e) => ((this.repo = e.target.value), (this.branch = ""))}>
                      ${repos.map((x) => html`<lit-material-select-option value=${x.fullName}>${x.fullName}${x.private ? " (private)" : ""}</lit-material-select-option>`)}
                    </lit-material-select>`
                  : html`<span class="hint">${g.repos ? "HITOP can't see any repository yet." : "Loading repositories…"}</span>`}
                <span class="git-links">
                  <button type="button" class="link-button" ?disabled=${Boolean(this.busy)} @click=${() => this.run("connect", () => g.connect({ add: true, collection: c.id }))}>
                    ${repos.length ? "Don't see it? Give HITOP access to more repositories" : "Choose repositories on GitHub"} ${icon("arrow-square-out")}
                  </button>
                  <button type="button" class="link-button" @click=${() => this.createRepo()}>Create a new repository ${icon("arrow-square-out")}</button>
                </span>
                ${this.creating
                  ? html`<span class="hint" role="status">Create it on GitHub, then come back to this tab: it is chosen here. If HITOP may see only selected repositories, add the new one with "Give HITOP access" first.
                      <button type="button" class="link-button" @click=${() => this.pickNew()}>Check again</button></span>`
                  : nothing}
              </li>
              <li>
                <span class="git-step-title">Where in the repository</span>
                <div class="git-fields">
                  <lit-material-text-field id="git-path" variant="outlined" label="Folder" .value=${path} placeholder="(the repository's root)"
                    @input=${(e) => (this.path = e.target.value.trim().replace(/^\/+|\/+$/g, ""))}></lit-material-text-field>
                  <lit-material-text-field id="git-branch" variant="outlined" label="Branch" .value=${this.branch || r?.defaultBranch || "main"}
                    @input=${(e) => (this.branch = e.target.value.trim())}></lit-material-text-field>
                </div>
              </li>`
          : nothing}
      </ol>
      ${g.connected && r
        ? html`<div class="git-preview git-indent" aria-label="Files HITOP will write">
            <p>Will write in <b>${r.fullName}</b> on <b>${this.branch || r.defaultBranch}</b>:</p>
            <ul>${shown.map((f) => html`<li><code>${f}</code></li>`)}${files.length > shown.length ? html`<li class="hint">and ${files.length - shown.length} more request files</li>` : nothing}</ul>
            <p class="hint">Every save in HITOP commits there, and commits on GitHub come back here. If the folder already holds a hitop collection, that one wins the first sync.</p>
          </div>
          <div class="page-actions git-indent">
            <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.save()}>${icon("github-logo")} ${this.busy === "link" ? "Saving…" : "Save to GitHub"}</lit-material-button>
          </div>`
        : nothing}`;
  }

  render() {
    const c = this.collection;
    const g = this.git;
    if (!c || !g) return nothing;
    const where = storageOf(this.store, c);
    let body;
    if (where.kind === "folder" || where.kind === "shared") {
      body = html`<p>${where.detail}. ${where.kind === "folder" ? "Use the Git page of the desktop app, or your own git, for that folder." : "Shared workspaces stay end-to-end encrypted, so HITOP Cloud can't write them to GitHub."}</p>`;
    } else if (!this.store.library?.status.on) {
      body = html`<p>Collections reach GitHub through HITOP Cloud. Sign in on the Sync page and turn on "Save my collections to HITOP Cloud" first.</p>
        <lit-material-button variant="filled" @click=${() => (this.closest("hitop-app")?.closeDialog(), this.store.show("sync"))}>Open the Sync page</lit-material-button>`;
    } else if (!g.info) {
      body = html`<p class="hint" role="status">Loading…</p>`;
    } else if (!g.info.enabled) {
      body = html`<p>Git sync isn't available on this server.</p>`;
    } else if (where.kind === "browser") {
      body = html`<p>${c.name} is kept in this browser only (it was here before you signed in). GitHub sync works through HITOP Cloud, so save it there first: it then shows on every device you sign in on.</p>
        <div class="page-actions">
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("upload", async () => {
            await this.store.library.upload(c.id);
            this.git.changed();
            this.note = `${c.name} is saved to HITOP Cloud.`;
          })}>${icon("cloud")} ${this.busy === "upload" ? "Saving…" : "Save to HITOP Cloud"}</lit-material-button>
        </div>`;
    } else {
      const l = g.linkOf(c);
      body = l ? this.renderLinked(l) : this.renderForm();
    }
    return html`<div class="git-panel">
      ${this.error ? html`<lit-material-alert variant="error" role="alert">${this.error}</lit-material-alert>` : nothing}
      ${this.note ? html`<p class="sync-note" role="status">${this.note}</p>` : nothing}
      ${body}
    </div>`;
  }
}

class HitopGithub extends LightElement {
  static properties = { store: { attribute: false }, version: { attribute: false }, busy: { state: true }, error: { state: true } };

  get git() {
    return this.store?.git;
  }

  render() {
    const g = this.git;
    if (!g?.enabled) return nothing;
    const s = this.store;
    const returned = g.returned && g.returned.collection == null ? g.returned : null;
    const rows = s.collections.map((c) => [c, storageOf(s, c)]).filter(([, w]) => w.kind === "cloud" || w.kind === "github");
    return html`<section aria-label="GitHub">
      <h2>GitHub</h2>
      ${returned ? html`<p class="sync-note" role="status">${returned.ok ? "GitHub is connected." : "GitHub didn't finish connecting. Try again."}</p>` : nothing}
      ${this.error ? html`<lit-material-alert variant="error" role="alert">${this.error}</lit-material-alert>` : nothing}
      <p class="hint">Save a collection to a GitHub repository to review changes in pull requests and run it in CI with <code>hitop run</code>. Every save in HITOP commits there; commits on GitHub come back here.</p>
      <div class="sync-form">
        ${g.connected
          ? html`<span>${icon("github-logo")} Connected as <b>${g.info.installations.map((i) => i.account).join(", ")}</b></span>
              <lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.go({ add: true })}>Manage repository access ${icon("arrow-square-out")}</lit-material-button>`
          : html`<lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.go({})}>${icon("github-logo")} Connect GitHub</lit-material-button>`}
      </div>
      ${rows.length
        ? html`<ul class="sync-list" aria-label="Collections and GitHub">
            ${rows.map(
              ([c, w]) => html`<li class="sync-item git-row">
                <span><b>${c.name}</b>
                  <span class=${w.attention ? "sync-note" : "hint"} aria-label=${`Where ${c.name} is saved`}>${icon(w.icon)} ${w.kind === "github" ? `${w.label}, ${w.state}` : "HITOP Cloud only"}</span></span>
                <lit-material-button variant="outlined" @click=${() => openGitDialog(this, c)}>${w.kind === "github" ? "Details" : "Save to GitHub…"}</lit-material-button>
              </li>`,
            )}
          </ul>`
        : nothing}
    </section>`;
  }

  async go(opts) {
    this.busy = "connect";
    this.error = "";
    try {
      await this.git.connect(opts);
    } catch (e) {
      this.error = e?.message || String(e);
      this.busy = "";
    }
  }
}

customElements.define("hitop-git-panel", HitopGitPanel);
customElements.define("hitop-github", HitopGithub);
