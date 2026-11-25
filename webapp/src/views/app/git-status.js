// A part of the app shell (HitopApp, ../hitop-app.js).
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { platform } from "../../platform/index.js";
import { slugName } from "../../collection-tree.js";

export const GitStatusPart = (Base) =>
  class extends Base {
  // --- git status in the shell (desktop) --------------------------------------------------------

  /** Folder collections git can look at: connected, on disk (not hitop sync's app storage). */
  gitFolders() {
    const s = this.store;
    if (!platform().git) return [];
    return s.collections.map((c) => [c, s.folderOf(c.id)]).filter(([, f]) => f?.connected && !String(f.ref ?? "").startsWith("hitop-sync:"));
  }

  // A moment after the last change (edits save to the folder in batches), every folder's status.
  scheduleGitStatus() {
    const folders = this.gitFolders();
    if (!folders.length) {
      if (this.gitStatus.size) {
        this.gitStatus = new Map();
        this.requestUpdate();
      }
      return;
    }
    const key = `${this.store.version}|${folders.map(([c]) => c.id).join()}`;
    if (key === this.gitSeen) return;
    this.gitSeen = key;
    clearTimeout(this.gitTimer);
    this.gitTimer = setTimeout(() => this.refreshGitStatus(), 700);
  }

  async refreshGitStatus() {
    const api = platform().git;
    if (!api) return;
    const next = new Map();
    await Promise.all(
      this.gitFolders().map(async ([c, f]) => {
        try {
          next.set(c.id, await api.call("status", { path: f.ref }));
        } catch {
          // Not a repository, or git can't read it: no pill, no letters.
        }
      }),
    );
    this.gitStatus = next;
    this.requestUpdate();
  }

  /** The git letter of a saved request in a folder collection: M, A, D, R, ? (untracked). */
  gitLetter(c, r) {
    const st = this.gitStatus.get(c.id);
    if (!st) return "";
    // A request's file: its `file` (in a folder) or its name as hitop-core slugs it.
    const file = `${st.prefix ? `${st.prefix.replace(/\/$/, "")}/` : ""}requests/${r.file || slugName(r.name)}.yaml`;
    const f = st.files.find((x) => x.path === file);
    if (!f) return "";
    if (f.conflicted) return "!";
    const change = f.worktree ?? f.index;
    return { added: "A", modified: "M", deleted: "D", renamed: "R", typechange: "T", untracked: "?" }[change] ?? "M";
  }

  // The title bar's branch pill: the collection on screen, when git knows it; opens the Git page.
  branchPill() {
    const c = this.currentCollection();
    const st = c ? this.gitStatus.get(c.id) : null;
    if (!st) return nothing;
    const name = st.branch ?? (st.head ? `detached ${st.head}` : "no commits");
    const label = `Git: ${name}${st.upstream ? `, ${st.ahead} ahead, ${st.behind} behind` : ""}. Open the Git page`;
    return html`<button type="button" class="chip branch-pill" aria-label=${label} title=${label} @click=${() => this.go("git", c.id)}>
      ${icon("git-branch")}<span class="mono branch-name">${name}</span>
      ${st.upstream && st.ahead ? html`<span class="mono branch-ahead">↑${st.ahead}</span>` : nothing}
      ${st.upstream && st.behind ? html`<span class="mono branch-behind">↓${st.behind}</span>` : nothing}
    </button>`;
  }
  };
