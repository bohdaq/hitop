// The Sync page (web and desktop): sign in to a hitop sync server, share a collection with a
// team end to end encrypted, sync it, resolve conflicts, manage members and invites, and make
// runner credentials for scheduled runs. Logic in src/sync.js.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { icon } from "../icons.js";
import { DEFAULT_SERVER, KeyError, SyncClient } from "../sync.js";
import { platform } from "../platform/index.js";

const errorText = (e) => (e && e.message) || String(e);

export function syncClient(store) {
  store.syncClient ??= new SyncClient(store);
  return store.syncClient;
}

class HitopSync extends LitElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    app: { attribute: false },
    busy: { state: true },
    error: { state: true },
    note: { state: true },
    fingerprint: { state: true },
    remote: { state: true },
    selected: { state: true },
    team: { state: true },
    conflicts: { state: true },
    shown: { state: true },
    invite: { state: true },
    runner: { state: true },
    awaitingCode: { state: true },
    ssoUrl: { state: true },
    audit: { state: true },
    warnings: { state: true },
    untrusted: { state: true },
  };

  createRenderRoot() {
    return this;
  }

  constructor() {
    super();
    this.conflicts = {};
    this.shown = null;
  }

  get sync() {
    return syncClient(this.store);
  }

  connectedCallback() {
    super.connectedCallback();
    this.refresh();
  }

  async refresh() {
    if (!this.sync.account()) return;
    this.fingerprint = await this.sync.fingerprint();
    try {
      this.remote = await this.sync.workspaces();
    } catch (e) {
      this.error = errorText(e);
    }
    for (const c of this.shared()) this.conflicts = { ...this.conflicts, [c.id]: await this.sync.conflicts(c.id) };
  }

  shared() {
    return this.store.collections.filter((c) => this.sync.link(c.id));
  }

  async run(what, fn) {
    this.busy = what;
    this.error = null;
    this.note = null;
    this.untrusted = null;
    try {
      await fn();
    } catch (e) {
      // A refused workspace key: forged, swapped or a hidden rotation is a security warning; a
      // key from a device not trusted yet can be trusted after comparing its fingerprint.
      this.error = e instanceof KeyError && e.security ? `Security warning: ${errorText(e)}` : errorText(e);
      if (e instanceof KeyError && e.kind === "untrustedSender" && e.fingerprint) this.untrusted = e;
    } finally {
      this.busy = null;
      this.warnings = this.sync.takeNotices();
    }
  }

  renderUntrusted() {
    const u = this.untrusted;
    return html`<div class="sync-untrusted" aria-label="Device not trusted">
      <p>The workspace key came from a device with fingerprint <code class="fingerprint">${u.fingerprint}</code>. Compare it with the person who added you (in person, on a call, or in a chat you trust). Only if it matches:</p>
      <lit-material-button variant="outlined" @click=${async () => {
        const ok = await confirm({ headline: "Trust this device?", message: `Fingerprint ${u.fingerprint}. If it isn't the one they see on their device, someone (the server, for instance) is trying to give you a key it knows: don't trust it.`, confirmLabel: "They match, trust it" });
        if (!ok) return;
        await this.run("trust", async () => {
          await this.sync.trustSender(u.workspace, u.senderKeyId, u.fingerprint);
          this.note = "Trusted. Sync again (or open the workspace) to use its key.";
        });
      }}>Trust this device…</lit-material-button>
    </div>`;
  }

  field(id) {
    return this.querySelector(`#${id}`)?.value?.trim() ?? "";
  }

  // --- sign in ----------------------------------------------------------------------

  renderSignIn() {
    return html`
      <section aria-label="Sign in">
        <h2>Sign in</h2>
        <p class="page-lead">Share collections with your team through a hitop sync server. Everything is encrypted on this device before it leaves: the server stores only ciphertext. Secret values never sync, only their names.</p>
        <div class="sync-form">
          <lit-material-text-field id="sync-server" variant="outlined" label="Sync server" .value=${this.sync.pending?.server ?? DEFAULT_SERVER}></lit-material-text-field>
          <lit-material-text-field id="sync-email" variant="outlined" label="Email" type="email" .value=${this.sync.pending?.email ?? ""}></lit-material-text-field>
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)}
            @click=${() => this.run("code", async () => {
              await this.sync.startSignIn(this.field("sync-server"), this.field("sync-email"));
              this.awaitingCode = true;
              this.note = "Check your mail for the sign-in code.";
            })}>Send code</lit-material-button>
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)}
            @click=${() => this.run("sso", async () => {
              this.ssoUrl = await this.sync.startSso(this.field("sync-server"), this.field("sync-email"));
              this.awaitingCode = false;
              // The web app opens it in a new tab; the desktop app shows the link to open.
              if (platform().name !== "tauri") window.open(this.ssoUrl, "_blank", "noopener,noreferrer");
              this.note = "Sign in on your organization's page, then paste the code it shows here.";
            })}>Sign in with SSO</lit-material-button>
        </div>
        ${this.ssoUrl
          ? html`<p class="hint">Sign-in page (open it in a browser if it didn't open): <a href=${this.ssoUrl} target="_blank" rel="noopener noreferrer">open</a></p>
              <pre class="code-preview" aria-label="SSO sign-in link">${this.ssoUrl}</pre>
              <div class="sync-form">
                <lit-material-text-field id="sync-sso-code" variant="outlined" label="Code shown after signing in"></lit-material-text-field>
                <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)}
                  @click=${() => this.run("signin", async () => {
                    await this.sync.finishSso(this.field("sync-sso-code"));
                    this.ssoUrl = null;
                    await this.refresh();
                  })}>Sign in</lit-material-button>
              </div>`
          : nothing}
        ${this.awaitingCode
          ? html`<div class="sync-form">
              <lit-material-text-field id="sync-code" variant="outlined" label="Code from the mail"></lit-material-text-field>
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)}
                @click=${() => this.run("signin", async () => {
                  await this.sync.finishSignIn(this.field("sync-code"));
                  this.awaitingCode = false;
                  await this.refresh();
                })}>Sign in</lit-material-button>
            </div>`
          : nothing}
      </section>`;
  }

  // --- shared collections -------------------------------------------------------------

  async syncOne(c) {
    await this.run(`sync-${c.id}`, async () => {
      const r = await this.sync.syncNow(c.id);
      const parts = [r.pulled.length && `${r.pulled.length} pulled`, r.removed.length && `${r.removed.length} deleted`, r.pushed.length && `${r.pushed.length} pushed`, r.merged.length && `${r.merged.length} merged`].filter(Boolean);
      this.note = r.conflicts.length ? `${c.name}: ${r.conflicts.length} conflict(s) to resolve below.` : `${c.name}: ${parts.length ? parts.join(", ") : "up to date"}.`;
      this.conflicts = { ...this.conflicts, [r.collectionId]: await this.sync.conflicts(r.collectionId) };
    });
  }

  renderConflict(c, k) {
    const open = this.shown === `${c.id}:${k.path}`;
    const pre = (label, text) => html`<div class="sync-side"><h4>${label}</h4><pre class="code-preview">${text ?? "(deleted)"}</pre></div>`;
    return html`<li class="sync-conflict">
      <span>${icon("warning-circle")} <code>${k.path}</code>: changed here and by someone else</span>
      <div class="page-actions">
        <lit-material-button variant="text" @click=${() => (this.shown = open ? null : `${c.id}:${k.path}`)}>${open ? "Hide" : "View both"}</lit-material-button>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("resolve", async () => {
          await this.sync.resolve(c.id, k.path, true);
          this.conflicts = { ...this.conflicts, [c.id]: await this.sync.conflicts(c.id) };
          this.note = `Kept yours: ${k.path}`;
        })}>Keep mine</lit-material-button>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("resolve", async () => {
          await this.sync.resolve(c.id, k.path, false);
          this.conflicts = { ...this.conflicts, [c.id]: await this.sync.conflicts(c.id) };
          this.note = `Took theirs: ${k.path}`;
        })}>Take theirs</lit-material-button>
      </div>
      ${open ? html`<div class="sync-both" aria-label=${`Both versions of ${k.path}`}>${pre("Mine (this device)", k.mine)}${pre("Theirs (the server)", k.theirs)}${pre("Before both changes", k.base)}</div>` : nothing}
    </li>`;
  }

  renderShared() {
    const shared = this.shared();
    const unshared = this.store.collections.filter((c) => !this.sync.link(c.id));
    const linked = new Set(shared.map((c) => this.sync.link(c.id).workspace));
    const joinable = (this.remote ?? []).filter((w) => !linked.has(w.id));
    return html`
      <section aria-label="Shared collections">
        <h2>Shared collections</h2>
        ${shared.length
          ? html`<ul class="sync-list">
              ${shared.map((c) => {
                const conflicts = this.conflicts[c.id] ?? [];
                const role = (this.remote ?? []).find((w) => w.id === this.sync.link(c.id).workspace)?.role;
                return html`<li class="sync-item">
                  <div class="sync-row">
                    <span><b>${c.name}</b> <span class="hint">${role ?? ""}</span></span>
                    <span class="page-actions">
                      <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.syncOne(c)}>${icon("arrows-clockwise")} Sync now</lit-material-button>
                      <lit-material-button variant="outlined" @click=${() => this.openTeam(c)}>Members</lit-material-button>
                    </span>
                  </div>
                  ${conflicts.length ? html`<ul class="sync-conflicts" aria-label=${`Conflicts in ${c.name}`}>${conflicts.map((k) => this.renderConflict(c, k))}</ul>` : nothing}
                  ${this.selected === c.id && this.team ? this.renderTeam(c) : nothing}
                </li>`;
              })}
            </ul>`
          : html`<p class="hint">Nothing shared yet.</p>`}
        ${unshared.length
          ? html`<div class="sync-form">
              <lit-material-select id="sync-share" variant="outlined" label="Collection to share">
                ${unshared.map((c) => html`<lit-material-select-option value=${String(c.id)}>${c.name}</lit-material-select-option>`)}
              </lit-material-select>
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("share", async () => {
                const id = this.querySelector("#sync-share").value || String(unshared[0].id);
                const c = unshared.find((x) => String(x.id) === id) ?? unshared[0];
                await this.sync.share(c.id);
                this.note = `Shared ${c.name}. Invite your team under Members.`;
                await this.refresh();
              })}>Share</lit-material-button>
            </div>`
          : nothing}
      </section>
      <section aria-label="Join">
        <h2>Join a workspace</h2>
        <div class="sync-form">
          <lit-material-text-field id="sync-invite-code" variant="outlined" label="Invite code"></lit-material-text-field>
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("accept", async () => {
            const r = await this.sync.accept(this.field("sync-invite-code"));
            this.note = r.inviter
              ? `Invite accepted (${r.role}). Tell the owner your fingerprint ${this.fingerprint}; once they add you, the workspace shows up below. Its key will be accepted from the inviter's device, fingerprint ${r.inviter}.`
              : `Invite accepted (${r.role}). Tell the owner your fingerprint ${this.fingerprint}; once they add you, the workspace shows up below. This code doesn't name the inviter's key: you'll be asked to compare the fingerprint of the device that sends the key.`;
          })}>Accept invite</lit-material-button>
        </div>
        ${joinable.length
          ? html`<ul class="sync-list">${joinable.map((w) => html`<li class="sync-row"><span>Workspace <code>${w.id.slice(0, 8)}</code> <span class="hint">${w.role}</span></span>
              <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("join", async () => {
                const id = await this.sync.join(w.id);
                this.note = `Opened ${this.store.collection(id)?.name ?? "the shared collection"}.`;
                await this.refresh();
              })}>Open here</lit-material-button></li>`)}</ul>`
          : nothing}
      </section>`;
  }

  // --- team -------------------------------------------------------------------------

  async openTeam(c) {
    if (this.selected === c.id) {
      this.selected = null;
      return;
    }
    await this.run("team", async () => {
      this.selected = c.id;
      this.team = await this.sync.members(c.id);
      this.invite = null;
      this.runner = null;
      this.audit = null;
    });
  }

  async reloadTeam(c) {
    this.team = await this.sync.members(c.id);
  }

  trustLabel(k) {
    return k.trust === "trusted" ? "trusted" : k.trust === "changed" ? "KEY CHANGED: not trusted" : "not checked yet";
  }

  renderKeys(c, keys, owner) {
    return html`<ul class="sync-keys">
      ${keys.map((k) => html`<li>
        <span>${k.name}</span> <code class="fingerprint" aria-label=${`Fingerprint of ${owner}'s ${k.name}`}>${k.fingerprint}</code>
        <span class=${`hint trust-${k.trust}`}>${this.trustLabel(k)}${k.hasKey === false ? ", waiting for the workspace key" : ""}${k.legacy ? ", key handed out without authentication (hitop 1.1)" : ""}</span>
        ${k.trust === "new"
          ? html`<lit-material-button variant="text" @click=${async () => {
              const ok = await confirm({ headline: `Trust this device of ${owner}?`, message: `Fingerprint ${k.fingerprint}. Compare it with ${owner} (in person, on a call, or in a chat you trust) before you say yes: the server hands out these keys, and a wrong key would let someone else read the workspace.`, confirmLabel: "They match, trust it" });
              if (!ok) return;
              this.sync.trust([k]);
              await this.reloadTeam(c);
            }}>Trust…</lit-material-button>`
          : nothing}
      </li>`)}
    </ul>`;
  }

  renderTeam(c) {
    const t = this.team;
    const owner = t.role === "owner";
    const waiting = t.members.some((m) => m.keys.some((k) => !k.hasKey && k.trust === "trusted"));
    return html`<div class="sync-team" aria-label=${`Members of ${c.name}`}>
      <h3>Members <span class="hint">key epoch ${t.epoch}, you are ${t.role}</span></h3>
      <ul class="sync-list">
        ${t.members.map((m) => {
          const who = m.runner ? "Runner credential" : m.email;
          const me = m.userId === this.sync.account().userId;
          return html`<li class="sync-member">
            <div class="sync-row">
              <span><b>${who}</b>${me ? " (you)" : ""}</span>
              <span class="page-actions">
                ${owner && !m.runner && !me
                  ? html`<lit-material-select variant="outlined" label=${`Role of ${who}`} .value=${m.role} @change=${(e) => this.run("role", async () => {
                      await this.sync.setRole(c.id, m.userId, e.target.value);
                      await this.reloadTeam(c);
                    })}>
                      ${["owner", "editor", "viewer"].map((r) => html`<lit-material-select-option value=${r}>${r}</lit-material-select-option>`)}
                    </lit-material-select>`
                  : html`<span class="hint">${m.role}</span>`}
                ${owner && !me
                  ? html`<lit-material-button variant="text" @click=${async () => {
                      const ok = await confirm({ headline: `Remove ${who}?`, message: "The workspace gets a new key and every file is encrypted again under it. They keep any copies they already have, but can't read anything written from now on.", confirmLabel: "Remove", destructive: true });
                      if (ok) await this.run("remove", async () => {
                        const r = await this.sync.removeMember(c.id, m.userId);
                        this.note = `Removed ${who}: new key (epoch ${r.epoch}), ${r.reencrypted} file(s) encrypted again.`;
                        await this.reloadTeam(c);
                      });
                    }}>Remove</lit-material-button>`
                  : nothing}
              </span>
            </div>
            ${this.renderKeys(c, m.keys, who)}
          </li>`;
        })}
      </ul>
      ${waiting && t.role !== "viewer"
        ? html`<lit-material-button variant="outlined" @click=${() => this.run("wrap", async () => {
            const n = await this.sync.shareKeysWithNewDevices(c.id);
            this.note = `Gave the workspace key to ${n} new device(s).`;
            await this.reloadTeam(c);
          })}>Give the key to new devices</lit-material-button>`
        : nothing}
      ${owner ? this.renderInvites(c) : nothing}
      ${owner ? this.renderRuns(c) : nothing}
      ${owner ? this.renderAudit(c) : nothing}
    </div>`;
  }

  renderInvites(c) {
    const t = this.team;
    return html`<h3>Invites</h3>
      ${t.invites.map((i) =>
        i.accepted
          ? html`<div class="sync-invite">
              <p><b>${i.accepted.email}</b> accepted (${i.role}). Check their fingerprint, trust it, then add them.</p>
              ${this.renderKeys(c, i.accepted.keys, i.accepted.email)}
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy) || !i.accepted.keys.every((k) => k.trust === "trusted")} @click=${() => this.run("add", async () => {
                await this.sync.addMember(c.id, i.accepted.email, i.role, i.accepted.keys);
                this.note = `Added ${i.accepted.email} as ${i.role}.`;
                await this.reloadTeam(c);
              })}>Add ${i.accepted.email}</lit-material-button>
            </div>`
          : html`<p class="hint">Open invite: ${i.email || "link"} (${i.role})</p>`,
      )}
      <div class="sync-form">
        <lit-material-text-field id="sync-invite-email" variant="outlined" label="Email (leave empty for a link)" type="email"></lit-material-text-field>
        <lit-material-select id="sync-invite-role" variant="outlined" label="Role" .value=${"editor"}>
          <lit-material-select-option value="editor">editor</lit-material-select-option>
          <lit-material-select-option value="viewer">viewer</lit-material-select-option>
          <lit-material-select-option value="owner">owner</lit-material-select-option>
        </lit-material-select>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("invite", async () => {
          const email = this.field("sync-invite-email");
          this.invite = await this.sync.invite(c.id, email, this.querySelector("#sync-invite-role").value || "editor");
          this.note = this.invite.mailed ? `Invite mailed to ${email}. So their hitop can check your key, also send them this code yourself (chat, in person):` : "Share this invite link privately:";
          await this.reloadTeam(c);
        })}>Invite</lit-material-button>
      </div>
      ${this.invite
        ? html`${this.invite.mailed ? html`<pre class="code-preview" aria-label="Invite code with your fingerprint">${this.invite.code}</pre>` : html`<pre class="code-preview" aria-label="Invite link">${this.invite.link}</pre>`}
            <p class="hint">It carries your key's fingerprint, <code class="fingerprint">${this.fingerprint ?? ""}</code>: their hitop accepts the workspace key only from this device. A mailed invite can't carry it (the server writes the mail).</p>`
        : nothing}
      <p class="hint">Roles: owners manage members; editors change requests; viewers only read. The server enforces them; every member holds the workspace key.</p>`;
  }

  renderRuns(c) {
    return html`<h3>Scheduled runs</h3>
      <p class="page-lead">Run this collection on a schedule from your own CI or cron: a runner credential pulls the workspace read-only (<code>hitop sync pull</code> with <code>HITOP_SYNC_RUNNER</code>), then <code>hitop run</code>. The runner decrypts on your machine, so the server still sees only ciphertext.</p>
      <p class="hint">Running on the sync server instead would mean handing the server a key that decrypts the workspace: it would then see your requests, responses and environment values while it runs them. hitop doesn't do that unless you set it up yourself (see the guide); prefer a runner you control.</p>
      <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("runner", async () => {
        this.runner = await this.sync.createRunner(c.id, "runner");
        await this.reloadTeam(c);
      })}>Create runner credential</lit-material-button>
      ${this.runner
        ? html`<p class="hint">Shown once. It decrypts this workspace: put it in your CI's secret store as HITOP_SYNC_RUNNER, nowhere else. Remove the runner under Members to revoke it.</p>
            <pre class="code-preview" aria-label="Runner credential">${this.runner.credential}</pre>`
        : nothing}`;
  }

  renderAudit(c) {
    const a = this.audit;
    const when = (t) => new Date(t * 1000).toISOString().replace("T", " ").replace(/\.\d+Z$/, " UTC");
    const detail = (d) => (d && Object.keys(d).length ? JSON.stringify(d) : "");
    return html`<h3>Audit log</h3>
      <p class="hint">Team actions in this workspace: members added and removed, role changes, invites, key rotations, runner credentials. The server keeps it for a year; each entry is chained to the one before, so an edited or deleted entry shows.</p>
      <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("audit", async () => {
        this.audit = await this.sync.audit(c.id);
      })}>${a ? "Reload" : "Show"} audit log</lit-material-button>
      ${a
        ? html`<p class=${a.check.ok ? "hint" : "sync-note"} role="status" aria-label="Audit chain">${a.check.ok ? `Chain intact: ${a.check.checked} entries checked by the server.` : `Chain broken: ${a.check.problem}`}</p>
            <table class="sync-audit" aria-label=${`Audit log of ${c.name}`}>
              <thead><tr><th>Time</th><th>Event</th><th>By</th><th>Details</th></tr></thead>
              <tbody>
                ${a.events.map((e) => html`<tr>
                  <td>${when(e.at)}</td><td><code>${e.event}</code></td><td>${e.actor.email || "-"}${e.ip ? html`<br><span class="hint">${e.ip}</span>` : nothing}</td><td class="hint">${detail(e.detail)}</td>
                </tr>`)}
              </tbody>
            </table>
            ${a.more ? html`<p class="hint">Older entries: <code>hitop sync audit --limit 0</code>.</p>` : nothing}`
        : nothing}`;
  }

  render() {
    const a = this.sync.account();
    return html`<div class="page sync-page">
      <header class="page-header"><h1>Sync</h1></header>
      ${a
        ? html`<p class="page-lead">Signed in to ${a.server} as <b>${a.email}</b>. This device: ${a.deviceName}, fingerprint <code class="fingerprint" aria-label="Your fingerprint">${this.fingerprint ?? ""}</code>
            <lit-material-button variant="text" @click=${() => this.run("signout", async () => {
              await this.sync.signOut();
              this.remote = null;
            })}>Sign out</lit-material-button></p>
          ${a.server === DEFAULT_SERVER && platform().name !== "tauri"
            ? html`<p class="hint">Organizations, members, usage and plan: <a href="https://cloud.hitop.rws8.tech/" target="_blank" rel="noopener noreferrer">HITOP Cloud</a></p>`
            : nothing}`
        : nothing}
      ${this.error ? html`<lit-material-alert variant="error" role="alert">${this.error}</lit-material-alert>` : nothing}
      ${this.untrusted ? this.renderUntrusted() : nothing}
      ${this.warnings?.length
        ? html`<ul class="sync-warnings" aria-label="Key warnings">${this.warnings.map((w) => html`<li class=${w.kind === "senderKeyChanged" ? "sync-note security" : "hint"}>${icon("warning-circle")} ${w.message}</li>`)}</ul>`
        : nothing}
      ${this.note ? html`<p class="sync-note" role="status">${this.note}</p>` : nothing}
      ${this.busy ? html`<p class="hint" aria-busy="true">Working…</p>` : nothing}
      ${a ? this.renderShared() : this.renderSignIn()}
    </div>`;
  }
}

customElements.define("hitop-sync", HitopSync);

export const syncPage = { kind: "sync", label: "Sync", icon: "arrows-clockwise", render: (store, app) => html`<hitop-sync .store=${store} .version=${store.version} .app=${app}></hitop-sync>` };
