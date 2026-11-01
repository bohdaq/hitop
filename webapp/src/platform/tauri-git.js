// The desktop app's Git panel for collection folders: status, diff, stage,
// commit, pull (fetch + fast-forward or merge, never a rebase), push,
// branches and merge conflicts. Rust does the work through libgit2
// (cli/crates/hitop-git, the `git` Tauri command in
// desktop/src-tauri/src/git.rs); HTTPS tokens live in the OS keychain.
// Only the Tauri adapter imports this file, so the web build has no Git page.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { registerIcons } from "@lit-material/icon";
import gitBranch from "@phosphor-icons/core/assets/regular/git-branch.svg?raw";
import gitCommit from "@phosphor-icons/core/assets/regular/git-commit.svg?raw";
import gitMerge from "@phosphor-icons/core/assets/regular/git-merge.svg?raw";
import { icon } from "../icons.js";

registerIcons({ "git-branch": gitBranch, "git-commit": gitCommit, "git-merge": gitMerge });

// `invoke` is the adapter's. Every call takes the folder path.
export function gitApi(invoke) {
  const git = (cmd, args) => invoke("git", { cmd, args });
  return {
    call: git,
    tokenStatus: (path) => invoke("git_token_status", { path }),
    setToken: (path, token, username) => invoke("git_token_set", { path, token, username: username || null }),
    deleteToken: (path) => invoke("git_token_delete", { path }),
  };
}

const CHANGE = { added: "A", modified: "M", deleted: "D", renamed: "R", typechange: "T", untracked: "U" };
const errorText = (e) => String(e?.message ?? e);
const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;

class HitopGit extends LitElement {
  static properties = {
    store: { attribute: false },
    app: { attribute: false },
    api: { attribute: false },
    version: { attribute: false },
    status: { state: true },
    branches: { state: true },
    error: { state: true },
    busy: { state: true },
    shown: { state: true },
    token: { state: true },
    message: { state: true },
  };

  createRenderRoot() {
    return this;
  }

  constructor() {
    super();
    this.status = null;
    this.branches = null;
    this.error = null;
    this.busy = null;
    // The diff or conflict on screen: { kind: "diff", file, staged, diff } | { kind: "conflict", file, conflict }
    this.shown = null;
    this.token = null;
    this.message = "";
    this.loadedFor = null;
  }

  // The folder collections, and the one this page works on.
  folders() {
    const s = this.store;
    return s.collections.filter((c) => s.folderOf(c.id)?.connected);
  }

  current() {
    const list = this.folders();
    return list.find((c) => c.id === this.store.view.collectionId) ?? list[0] ?? null;
  }

  path() {
    const c = this.current();
    return c ? this.store.folderOf(c.id).ref : null;
  }

  updated() {
    const path = this.path();
    if (path !== this.loadedFor) {
      this.loadedFor = path;
      this.shown = null;
      this.message = "";
      this.refresh();
    }
  }

  async refresh() {
    const path = this.path();
    if (!path) return;
    try {
      const [status, branches] = await Promise.all([this.api.call("status", { path }), this.api.call("branches", { path })]);
      this.status = status;
      this.branches = branches;
      this.error = null;
      if (status.state === "merge" && !this.message) this.message = status.mergeMessage?.trim() ?? "";
      if (this.shown?.kind === "diff") {
        const still = status.files.some((f) => f.path === this.shown.file && (this.shown.staged ? f.index : f.worktree));
        if (still) this.showDiff(this.shown.file, this.shown.staged);
        else this.shown = null;
      }
      if (this.shown?.kind === "conflict" && !status.files.some((f) => f.path === this.shown.file && f.conflicted)) this.shown = null;
    } catch (e) {
      this.status = null;
      this.branches = null;
      this.error = errorText(e);
    }
    try {
      this.token = await this.api.tokenStatus(path);
    } catch {
      this.token = null;
    }
  }

  // Runs one action: saves pending edits first, reloads the folder when
  // the files may have changed, then refreshes.
  async run(label, fn, { files = false } = {}) {
    const c = this.current();
    if (!c || this.busy) return null;
    this.busy = label;
    let out = null;
    try {
      await this.store.flushFolder(c.id);
      out = await fn(this.path());
      if (files) await this.store.reloadFolder(c.id);
    } catch (e) {
      this.store.notify(errorText(e));
    } finally {
      this.busy = null;
    }
    await this.refresh();
    return out;
  }

  async showDiff(file, staged) {
    try {
      const diff = await this.api.call("diff", { path: this.path(), file, staged });
      this.shown = { kind: "diff", file, staged, diff };
    } catch (e) {
      this.store.notify(errorText(e));
    }
  }

  async showConflict(file) {
    try {
      const conflict = await this.api.call("conflict", { path: this.path(), file });
      this.shown = { kind: "conflict", file, conflict };
    } catch (e) {
      this.store.notify(errorText(e));
    }
  }

  stage(files) {
    return this.run("Staging", (path) => this.api.call("stage", { path, files }));
  }

  unstage(files) {
    return this.run("Unstaging", (path) => this.api.call("unstage", { path, files }));
  }

  async discard(files) {
    const ok = await confirm({
      headline: files.length === 1 ? `Discard changes to ${files[0]}?` : `Discard changes to ${plural(files.length, "file")}?`,
      message: "The files go back to their last commit (new files are deleted). This can't be undone.",
      confirmLabel: "Discard",
      destructive: true,
    });
    if (ok) await this.run("Discarding", (path) => this.api.call("discard", { path, files }), { files: true });
  }

  async commit() {
    const message = this.querySelector("#git-message")?.value ?? this.message;
    const done = await this.run("Committing", (path) => this.api.call("commit", { path, message }));
    if (done) {
      this.message = "";
      const field = this.querySelector("#git-message");
      if (field) field.value = "";
      this.store.notify(`Committed ${done.id}: ${done.summary}`);
    }
  }

  async setIdentity() {
    const name = this.querySelector("#git-name")?.value ?? "";
    const email = this.querySelector("#git-email")?.value ?? "";
    await this.run("Saving", (path) => this.api.call("setIdentity", { path, name, email }));
  }

  // What a pull or merge did, said in the snackbar.
  outcome(o, what) {
    if (!o) return;
    if (o.result === "upToDate") this.store.notify(`${what}: already up to date`);
    else if (o.result === "fastForward") this.store.notify(`${what}: fast-forwarded to ${o.head}`);
    else if (o.result === "merged") this.store.notify(`${what}: merged (${o.head})`);
    else if (o.result === "conflicts") {
      this.store.notify(`${what} stopped with ${plural(o.files.length, "conflict")}: resolve them below, then commit`);
      this.message = "";
    }
  }

  async pull() {
    this.outcome(await this.run("Pulling", (path) => this.api.call("pull", { path }), { files: true }), "Pull");
  }

  async push() {
    const out = await this.run("Pushing", (path) => this.api.call("push", { path }));
    if (out) this.store.notify(`Pushed ${out.branch} to ${out.remote}`);
  }

  async fetch() {
    await this.run("Fetching", (path) => this.api.call("fetch", { path }));
  }

  async createBranch() {
    const field = this.querySelector("#git-new-branch");
    const name = field?.value.trim();
    if (!name) return;
    const done = await this.run("Creating", (path) => this.api.call("createBranch", { path, name, checkout: true }).then(() => true));
    if (done && field) field.value = "";
  }

  switchTo(name) {
    return this.run("Switching", (path) => this.api.call("switchBranch", { path, name }), { files: true });
  }

  async deleteBranch(name) {
    const ok = await confirm({ headline: `Delete the branch ${name}?`, message: "It's merged into the current branch, so no commits are lost.", confirmLabel: "Delete", destructive: true });
    if (ok) await this.run("Deleting", (path) => this.api.call("deleteBranch", { path, name }));
  }

  async merge(name) {
    this.outcome(await this.run("Merging", (path) => this.api.call("mergeBranch", { path, name }), { files: true }), `Merge ${name}`);
  }

  async abortMerge() {
    const ok = await confirm({ headline: "Abort the merge?", message: "The folder goes back to your last commit; resolutions made so far are lost.", confirmLabel: "Abort merge", destructive: true });
    if (ok) {
      await this.run("Aborting", (path) => this.api.call("abortMerge", { path }), { files: true });
      this.message = "";
    }
  }

  async resolve(file, resolution) {
    const done = await this.run("Resolving", (path) => this.api.call("resolve", { path, file, resolution }).then(() => true), { files: true });
    if (done) this.store.notify(`Marked ${file} resolved`);
  }

  async saveToken() {
    const token = this.querySelector("#git-token")?.value ?? "";
    const username = this.querySelector("#git-token-user")?.value ?? "";
    const done = await this.run("Saving", (path) => this.api.setToken(path, token, username).then(() => true));
    if (done) {
      this.querySelector("#git-token").value = "";
      this.store.notify(`Saved the token for ${this.token?.host} in your system keychain`);
    }
  }

  async removeToken() {
    await this.run("Removing", (path) => this.api.deleteToken(path));
  }

  // --- rendering ----------------------------------------------------------------

  fileRow(f, where) {
    const shown = this.shown?.file === f.path && (where === "conflicts" ? this.shown.kind === "conflict" : this.shown.kind === "diff" && this.shown.staged === (where === "staged"));
    const change = where === "staged" ? f.index : f.worktree;
    const open = () => (where === "conflicts" ? this.showConflict(f.path) : this.showDiff(f.path, where === "staged"));
    return html`<li class="git-file" aria-current=${shown ? "true" : "false"}>
      <button type="button" class="git-file-name" @click=${open} title=${where === "conflicts" ? "Show the conflict" : "Show the diff"}>
        <span class="git-badge" data-change=${where === "conflicts" ? "conflicted" : change} aria-label=${where === "conflicts" ? "conflicted" : change}>${where === "conflicts" ? "C" : CHANGE[change] ?? "?"}</span>
        <span>${f.path}</span>
      </button>
      ${where === "staged"
        ? html`<lit-material-button variant="text" ?disabled=${!!this.busy} @click=${() => this.unstage([f.path])} aria-label=${`Unstage ${f.path}`}>Unstage</lit-material-button>`
        : where === "changes"
          ? html`<lit-material-button variant="text" ?disabled=${!!this.busy} @click=${() => this.discard([f.path])} aria-label=${`Discard ${f.path}`}>Discard</lit-material-button>
              <lit-material-button variant="text" ?disabled=${!!this.busy} @click=${() => this.stage([f.path])} aria-label=${`Stage ${f.path}`}>Stage</lit-material-button>`
          : html`<lit-material-button variant="text" ?disabled=${!!this.busy} aria-label=${`Mark ${f.path} resolved`} @click=${() => this.resolve(f.path, { take: "worktree" })}>Mark resolved</lit-material-button>`}
    </li>`;
  }

  renderDiff() {
    const d = this.shown.diff;
    return html`<section class="git-detail" aria-label=${`Diff of ${d.path}`}>
      <h2>${d.path} <span class="hint">${d.staged ? "staged" : "not staged"}</span></h2>
      ${d.binary
        ? html`<p class="hint">Binary file: no text diff.</p>`
        : d.hunks.length
          ? html`<pre class="code-preview git-diff">${d.hunks.map(
              (h) => html`<span class="git-hunk">${h.header}</span>\n${h.lines.map((l) => html`<span class=${`git-line git-${l.kind}`}>${l.kind === "add" ? "+" : l.kind === "del" ? "-" : l.kind === "note" ? "\\" : " "}${l.kind === "note" ? ` ${l.text.replace(/^\\\s*/, "")}` : l.text}</span>\n`)}`,
            )}</pre>`
          : html`<p class="hint">No changes in the text.</p>`}
    </section>`;
  }

  renderConflict() {
    const c = this.shown.conflict;
    const side = (label, text) => html`<div class="git-side"><h3>${label}</h3>${text == null ? html`<p class="hint">${c.binary ? "Not text" : "No file on this side"}</p>` : html`<pre class="code-preview">${text}</pre>`}</div>`;
    return html`<section class="git-detail" aria-label=${`Conflict in ${c.path}`}>
      <h2>${c.path} <span class="hint">conflict</span></h2>
      <div class="git-sides">${side("Ours (this branch)", c.ours)}${side("Theirs (merged in)", c.theirs)}${side("Base", c.base)}</div>
      <div class="page-actions">
        <lit-material-button variant="outlined" ?disabled=${!!this.busy} @click=${() => this.resolve(c.path, { take: "ours" })}>Take ours</lit-material-button>
        <lit-material-button variant="outlined" ?disabled=${!!this.busy} @click=${() => this.resolve(c.path, { take: "theirs" })}>Take theirs</lit-material-button>
      </div>
      ${c.binary
        ? nothing
        : html`<lit-material-textarea id="git-resolution" variant="outlined" label="Resolved text" rows="12" .value=${c.merged ?? c.ours ?? c.theirs ?? ""}></lit-material-textarea>
            <div class="page-actions">
              <lit-material-button variant="filled" ?disabled=${!!this.busy} @click=${() => this.useEdited(c.path)}>Use this text and mark resolved</lit-material-button>
            </div>
            <p class="hint">Or edit the file in your editor, then choose Mark resolved in the list.</p>`}
    </section>`;
  }

  useEdited(file) {
    const text = this.querySelector("#git-resolution")?.value ?? "";
    if (/^(<{7}|>{7}) /m.test(text)) {
      this.store.notify("The text still has conflict markers (<<<<<<< or >>>>>>>)");
      return;
    }
    return this.resolve(file, { take: "content", text });
  }

  renderBranches() {
    const b = this.branches;
    if (!b) return nothing;
    const remoteOnly = b.remote.filter((r) => !b.local.some((l) => l.upstream === r || l.name === r.split("/").slice(1).join("/")));
    const merging = this.status?.state === "merge";
    return html`<section aria-label="Branches">
      <h2>Branches</h2>
      <ul class="git-branches">
        ${b.local.map(
          (br) => html`<li>
            ${icon("git-branch")}
            <span class="git-branch-name">${br.name}</span>
            ${br.current ? html`<span class="hint">current</span>` : nothing}
            ${br.upstream ? html`<span class="hint">↔ ${br.upstream}</span>` : nothing}
            ${br.current
              ? nothing
              : html`<lit-material-button variant="text" ?disabled=${!!this.busy || merging} aria-label=${`Switch to ${br.name}`} @click=${() => this.switchTo(br.name)}>Switch</lit-material-button>
                  <lit-material-button variant="text" ?disabled=${!!this.busy || merging} aria-label=${`Merge ${br.name}`} @click=${() => this.merge(br.name)}>Merge into current</lit-material-button>
                  ${br.merged ? html`<lit-material-button variant="text" ?disabled=${!!this.busy} aria-label=${`Delete ${br.name}`} @click=${() => this.deleteBranch(br.name)}>Delete</lit-material-button>` : nothing}`}
          </li>`,
        )}
        ${remoteOnly.map(
          (r) => html`<li>
            ${icon("globe")}<span class="git-branch-name">${r}</span>
            <lit-material-button variant="text" ?disabled=${!!this.busy || merging} aria-label=${`Check out ${r}`} @click=${() => this.switchTo(r)}>Check out</lit-material-button>
          </li>`,
        )}
      </ul>
      <form class="git-inline-form" @submit=${(e) => (e.preventDefault(), this.createBranch())}>
        <lit-material-text-field id="git-new-branch" variant="outlined" label="New branch"></lit-material-text-field>
        <lit-material-button variant="outlined" type="submit" ?disabled=${!!this.busy || merging}>Create and switch</lit-material-button>
      </form>
    </section>`;
  }

  renderRemoteAccess() {
    const t = this.token;
    if (!t?.host) return nothing;
    return html`<section aria-label="Remote access">
      <h2>Remote access</h2>
      <p class="hint">
        HTTPS token for ${t.host}: ${t.saved ? html`<strong>saved in your system keychain</strong>${t.username ? ` (user ${t.username})` : ""}` : "none"}. Without one, git's credential helper is
        asked (not in the Mac App Store build); SSH remotes use your SSH agent.
      </p>
      <form class="git-inline-form" @submit=${(e) => (e.preventDefault(), this.saveToken())}>
        <lit-material-text-field id="git-token" type="password" variant="outlined" label=${`Token for ${t.host}`}></lit-material-text-field>
        <lit-material-text-field id="git-token-user" variant="outlined" label="User name (optional)" .value=${t.username ?? ""}></lit-material-text-field>
        <lit-material-button variant="outlined" type="submit" ?disabled=${!!this.busy}>Save token</lit-material-button>
        ${t.saved ? html`<lit-material-button variant="text" ?disabled=${!!this.busy} @click=${() => this.removeToken()}>Remove token</lit-material-button>` : nothing}
      </form>
    </section>`;
  }

  render() {
    const list = this.folders();
    const col = this.current();
    if (!col) {
      return html`<div class="page">
        <header class="page-header"><h1>Git</h1></header>
        <lit-material-empty-state headline="No collection folder open">
          Open a collection folder that is in a git repository (Collections, Open folder) to see its changes, commit, pull and push here.
        </lit-material-empty-state>
      </div>`;
    }
    const s = this.status;
    const files = s?.files ?? [];
    const conflicts = files.filter((f) => f.conflicted);
    const staged = files.filter((f) => f.index);
    const changes = files.filter((f) => f.worktree);
    const merging = s?.state === "merge";
    const busy = !!this.busy;
    return html`<div class="page git-page">
      <header class="page-header">
        <h1>Git</h1>
        <div class="page-actions">
          ${list.length > 1
            ? html`<lit-material-select variant="outlined" label="Collection folder" .value=${String(col.id)} @change=${(e) => this.store.show("git", list.find((c) => String(c.id) === e.target.value)?.id ?? null)}>
                ${list.map((c) => html`<lit-material-select-option value=${String(c.id)}>${c.name}</lit-material-select-option>`)}
              </lit-material-select>`
            : nothing}
          <lit-material-icon-button aria-label="Refresh" ?disabled=${busy} @click=${() => this.refresh()}>${icon("arrows-clockwise")}</lit-material-icon-button>
        </div>
      </header>
      <p class="page-lead">${col.name}: ${this.store.folderOf(col.id).label}${s?.prefix ? html` <span class="hint">(${s.prefix} in ${s.root})</span>` : nothing}</p>
      ${this.error
        ? html`<p class="hint" role="alert">${this.error}</p>`
        : !s
          ? html`<p class="hint">Loading…</p>`
          : html`
              <div class="git-bar" role="status">
                ${icon("git-branch")}
                <strong class="git-current">${s.branch ?? `detached at ${s.head}`}</strong>
                ${s.upstream ? html`<span>${s.upstream}</span><span class="git-ahead" aria-label="ahead, behind">↑${s.ahead} ↓${s.behind}</span>` : html`<span class="hint">no upstream yet</span>`}
                ${s.head ? html`<span class="hint">at ${s.head}</span>` : html`<span class="hint">no commits yet</span>`}
                <span class="git-bar-actions">
                  <lit-material-button variant="outlined" ?disabled=${busy || !s.remotes.length} @click=${() => this.fetch()}>Fetch</lit-material-button>
                  <lit-material-button variant="outlined" ?disabled=${busy || !s.remotes.length || merging} @click=${() => this.pull()}>${icon("arrow-down")} Pull</lit-material-button>
                  <lit-material-button variant="outlined" ?disabled=${busy || !s.remotes.length || merging} @click=${() => this.push()}>${icon("arrow-up")} Push</lit-material-button>
                </span>
              </div>
              ${this.busy ? html`<p class="hint" role="status">${this.busy}…</p>` : nothing}
              ${merging
                ? html`<lit-material-alert variant="warning" class="git-merging">
                    ${conflicts.length ? `Merging: ${plural(conflicts.length, "conflict")} to resolve, then commit to finish the merge.` : "Merging: all conflicts are resolved. Commit to finish the merge."}
                    <lit-material-button slot="action" variant="text" ?disabled=${busy} @click=${() => this.abortMerge()}>Abort merge</lit-material-button>
                  </lit-material-alert>`
                : nothing}
              ${s.state !== "clean" && !merging ? html`<p class="hint" role="alert">A ${s.state} is in progress: finish it with git.</p>` : nothing}
              ${conflicts.length
                ? html`<section aria-label="Conflicts">
                    <h2>Conflicts</h2>
                    <ul class="git-files">
                      ${conflicts.map((f) => this.fileRow(f, "conflicts"))}
                    </ul>
                  </section>`
                : nothing}
              <section aria-label="Staged changes">
                <h2>Staged ${staged.length ? html`<lit-material-button variant="text" ?disabled=${busy} @click=${() => this.unstage(staged.map((f) => f.path))}>Unstage all</lit-material-button>` : nothing}</h2>
                ${staged.length ? html`<ul class="git-files">${staged.map((f) => this.fileRow(f, "staged"))}</ul>` : html`<p class="hint">Nothing staged.</p>`}
              </section>
              <section aria-label="Changes">
                <h2>Changes ${changes.length ? html`<lit-material-button variant="text" ?disabled=${busy} @click=${() => this.stage(changes.map((f) => f.path))}>Stage all</lit-material-button>` : nothing}</h2>
                ${changes.length ? html`<ul class="git-files">${changes.map((f) => this.fileRow(f, "changes"))}</ul>` : html`<p class="hint">No changes.</p>`}
              </section>
              ${this.shown?.kind === "diff" ? this.renderDiff() : this.shown?.kind === "conflict" ? this.renderConflict() : nothing}
              <section aria-label="Commit">
                <h2>${icon("git-commit")} Commit</h2>
                ${s.identity
                  ? html`<p class="hint">As ${s.identity.name} &lt;${s.identity.email}&gt; (git config)</p>`
                  : html`<p class="hint">Set your name and email for commits in this repository (git config user.name and user.email aren't set, or the app can't read them).</p>
                      <form class="git-inline-form" @submit=${(e) => (e.preventDefault(), this.setIdentity())}>
                        <lit-material-text-field id="git-name" variant="outlined" label="Name"></lit-material-text-field>
                        <lit-material-text-field id="git-email" variant="outlined" label="Email"></lit-material-text-field>
                        <lit-material-button variant="outlined" type="submit" ?disabled=${busy}>Save</lit-material-button>
                      </form>`}
                <lit-material-textarea id="git-message" variant="outlined" label="Commit message" rows="3" .value=${this.message} @input=${(e) => (this.message = e.target.value)}></lit-material-textarea>
                <div class="page-actions">
                  <lit-material-button variant="filled" ?disabled=${busy || conflicts.length > 0 || (!staged.length && !merging) || !s.identity} @click=${() => this.commit()}>
                    ${icon(merging ? "git-merge" : "git-commit")} ${merging ? "Commit merge" : `Commit ${plural(staged.length, "file")}`}
                  </lit-material-button>
                </div>
              </section>
              ${this.renderBranches()} ${this.renderRemoteAccess()}
            `}
    </div>`;
  }
}

customElements.define("hitop-git", HitopGit);

// The sidebar page. `api` from gitApi().
export function gitPages(api) {
  return [{ kind: "git", label: "Git", icon: "git-branch", render: (store, app) => html`<hitop-git .store=${store} .app=${app} .api=${api} .version=${store.version}></hitop-git>` }];
}
