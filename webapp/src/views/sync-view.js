// <hitop-sync>, two uses (web and desktop; logic in src/sync.js):
// - the Account page (kind "sync", opened from the top bar's account menu): sign in (email,
//   then a password or a mailed code; Google, GitHub; More options: the server, SSO); signed in,
//   a short page: the address and its save state, GitHub, the plan, shared collections waiting
//   to open here, and under Password & recovery the password, recovery phrase and fingerprint;
// - with `.join`: the account menu's Join dialog (an invite code to accept);
// - with `.collection`: the People tab of a collection's Share dialog: share it end to end
//   encrypted, members and invites, and under Advanced its run history, CI runner and audit log.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { icon } from "../icons.js";
import { DEFAULT_SERVER, KeyError, SyncClient } from "../sync.js";
import { planLink } from "../plan-link.js";
import { orgLabel } from "../orgs.js";
import { platform } from "../platform/index.js";
import "./github-view.js";

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
    limitUrl: { state: true },
    recovery: { state: true },
    recoveryWords: { state: true },
    recovering: { state: true },
    mode: { state: true },
    methods: { state: true },
    resetSent: { state: true },
    history: { state: true },
    collection: { attribute: false },
    join: { attribute: false },
    continued: { state: true },
    server: { state: true },
    email: { state: true },
  };

  createRenderRoot() {
    return this;
  }

  constructor() {
    super();
    this.conflicts = {};
    this.shown = null;
    // Sign-in: "password" (default), "code" (a mailed code), "signup" (waiting for the
    // confirmation code), "forgot" (reset by mail), "paste" (a code from SSO, Google or GitHub).
    this.mode = "password";
    // The first step asks only the address; Continue shows the password (kept while editing).
    this.continued = false;
    this.server = DEFAULT_SERVER;
    this.email = "";
  }

  get sync() {
    return syncClient(this.store);
  }

  connectedCallback() {
    super.connectedCallback();
    this.onLibrary ??= () => this.requestUpdate();
    this.store?.library?.addEventListener("change", this.onLibrary);
    this.refresh().then(() => this.loadTeam());
    if (!this.sync.account()) this.loadMethods(DEFAULT_SERVER);
  }

  // In a Share dialog: the collection's members, shown at once.
  async loadTeam() {
    const c = this.collection;
    if (!c || !this.sync.account() || !this.sync.link(c.id)) return;
    try {
      this.selected = c.id;
      this.team = await this.sync.members(c.id);
    } catch (e) {
      this.error = errorText(e);
    }
  }

  // Which buttons the server's sign-in offers (Google, GitHub); quietly nothing when unknown.
  async loadMethods(server) {
    const m = await this.sync.authMethods(server).catch(() => null);
    if (this.server === server || server === DEFAULT_SERVER) this.methods = m;
  }

  async refresh() {
    if (!this.sync.account()) return;
    this.fingerprint = await this.sync.fingerprint();
    try {
      this.recovery = await this.sync.recoveryStatus();
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
    this.limitUrl = null;
    try {
      await fn();
    } catch (e) {
      this.limitUrl = planLink(e);
      // A refused workspace key: forged, swapped or a hidden rotation is a security warning; a
      // key from a device not trusted yet can be trusted after comparing its fingerprint.
      this.error = e instanceof KeyError && e.security ? `Security warning: ${errorText(e)}` : errorText(e);
      if (e instanceof KeyError && e.kind === "untrustedSender" && e.fingerprint) this.untrusted = e;
    } finally {
      this.busy = null;
      this.warnings = this.sync.takeNotices();
      // Signed in or out just now: the personal library links or stops (library.js), and the top
      // bar's account menu changes.
      this.store?.library?.checkAccount().catch(() => {});
      this.closest("hitop-app")?.requestUpdate();
      // Keep the recovery data up to date with this device's trust and key memory.
      if (!String(what).startsWith("recovery") && what !== "signout") {
        await this.sync.refreshRecovery().catch((e) => (this.error ??= `Couldn't update your recovery data: ${errorText(e)}`));
      }
    }
  }

  renderRecovery() {
    const r = this.recovery;
    const web = platform().name !== "tauri";
    return html`<section class="sync-recovery" aria-label="Recovery phrase">
      <h3>Recovery phrase</h3>
      <p class="hint" role="status" aria-label="Recovery status">${r?.set
        ? `Recovery phrase set${r.updatedAt ? `, updated ${new Date(r.updatedAt * 1000).toISOString().slice(0, 10)}` : ""}.`
        : `Not set.${web ? " Your device key lives only in this browser." : ""}`}</p>
      <div class="page-actions">
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("recovery-create", async () => {
          if (r?.set && !(await confirm({ headline: "Make a new recovery phrase?", message: "Your current phrase stops working.", confirmLabel: "Make a new one" }))) return;
          this.recoveryWords = await this.sync.createRecovery();
          this.recovery = await this.sync.recoveryStatus();
        })}>${r?.set ? "Make a new recovery phrase" : "Create recovery phrase"}</lit-material-button>
        <lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => (this.recovering = !this.recovering)}>Recover a lost device</lit-material-button>
        ${r?.set
          ? html`<lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.run("recovery-remove", async () => {
              if (!(await confirm({ headline: "Remove the recovery phrase?", message: "It then recovers nothing.", confirmLabel: "Remove", destructive: true }))) return;
              await this.sync.removeRecovery();
              this.recovery = await this.sync.recoveryStatus();
            })}>Remove</lit-material-button>`
          : nothing}
      </div>
      ${this.recoveryWords
        ? html`<p class="sync-note">Write these 24 words down and keep them offline. They are shown only now; with them and your mailbox anyone can become this device.</p>
            <pre class="code-preview" aria-label="Your recovery phrase">${this.recoveryWords}</pre>
            <lit-material-button variant="text" @click=${() => (this.recoveryWords = null)}>I wrote it down</lit-material-button>`
        : nothing}
      ${this.recovering
        ? html`<lit-material-text-field id="sync-recovery-phrase" variant="outlined" label="Recovery phrase (24 words)"></lit-material-text-field>
            <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("recovery-use", async () => {
              const words = this.field("sync-recovery-phrase");
              const got = await this.sync.useRecovery(words);
              this.recovering = false;
              this.note = `Recovered the device "${got.deviceName}" (fingerprint ${got.fingerprint}) here.`;
              await this.refresh();
            })}>Recover</lit-material-button>`
        : nothing}
    </section>`;
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

  // A password as typed (spaces kept).
  rawField(id) {
    return String(this.querySelector(`#${id}`)?.value ?? "");
  }

  renderPassword() {
    const r = this.recovery;
    const set = Boolean(r?.password);
    return html`<section class="sync-password" aria-label="Password">
      <h3>Password</h3>
      <p class="hint" role="status" aria-label="Password status">${set
        ? `Your account has a password.${r.passwordBundle ? (r.passwordKept ? " It unlocks this device's keys on a new device." : " It unlocks another device's keys.") : ""}`
        : "Not set."}</p>
      ${r?.passwordBundle && !r?.passwordKept
        ? html`<div class="sync-form">
            <lit-material-text-field id="sync-unlock-password" variant="outlined" label="Your password" type="password"></lit-material-text-field>
            <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("recovery-unlock", async () => {
              const got = await this.sync.unlockWithPassword(this.rawField("sync-unlock-password"));
              this.note = `Your password unlocked the device "${got.deviceName}" (fingerprint ${got.fingerprint}) here.`;
              await this.refresh();
            })}>Unlock with password</lit-material-button>
          </div>`
        : nothing}
      <div class="sync-form">
        ${set ? html`<lit-material-text-field id="sync-current-password" variant="outlined" label="Current password" type="password"></lit-material-text-field>` : nothing}
        <lit-material-text-field id="sync-new-password" variant="outlined" label="New password" type="password"></lit-material-text-field>
        <lit-material-text-field id="sync-new-password2" variant="outlined" label="Repeat the new password" type="password"></lit-material-text-field>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("password", async () => {
          const next = this.rawField("sync-new-password");
          if (next !== this.rawField("sync-new-password2")) throw new Error("The two new passwords differ");
          await this.sync.changePassword(this.rawField("sync-current-password"), next);
          this.note = set ? "Password changed. Your other devices were signed out; they sign in with the new one." : "Password set. It now signs you in and unlocks this device's keys on a new device.";
          this.recovery = await this.sync.recoveryStatus();
        })}>${set ? "Change password" : "Set password"}</lit-material-button>
      </div>
    </section>`;
  }

  // --- sign in ----------------------------------------------------------------------

  renderSignIn() {
    return html`
      <section aria-label="Sign in">${this.renderSignInForms()}</section>`;
  }

  async browserSignIn(kind) {
    const server = (this.server || DEFAULT_SERVER);
    this.ssoUrl = kind === "sso" ? await this.sync.startSso(server, this.field("sync-email")) : await this.sync.startOAuth(server, kind);
    this.ssoKind = kind;
    this.mode = "paste";
    // The web app opens it in a new tab; the desktop app shows the link to open.
    if (platform().name !== "tauri") window.open(this.ssoUrl, "_blank", "noopener,noreferrer");
    this.note = kind === "sso" ? "Sign in on your organization's page, then paste the code it shows here." : `Sign in with ${kind === "github" ? "GitHub" : "Google"}, then paste the code the page shows here.`;
  }

  signedInNote(r) {
    if (r?.unlocked) this.note = `Signed in. Your password unlocked your device "${r.deviceName}" (fingerprint ${r.fingerprint}) here: your workspaces open without a trust step.`;
  }

  renderAlternatives() {
    const m = this.methods;
    return html`
      <div class="page-actions sync-alternatives">
        ${this.mode === "code"
          ? html`<lit-material-button variant="text" @click=${() => ((this.mode = "password"), (this.awaitingCode = false))}>Use a password instead</lit-material-button>`
          : html`<lit-material-button variant="text" @click=${() => (this.mode = "code")}>Email me a code instead</lit-material-button>`}
      </div>
      ${m?.google || m?.github
        ? html`<div class="sync-or" role="presentation"><span>or</span></div>
            <div class="page-actions sync-alternatives">
              ${m?.google ? html`<lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("google", () => this.browserSignIn("google"))}>Continue with Google</lit-material-button>` : nothing}
              ${m?.github ? html`<lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("github", () => this.browserSignIn("github"))}>Continue with GitHub</lit-material-button>` : nothing}
            </div>`
        : nothing}
      ${this.renderMore()}`;
  }

  // The rare parts of signing in: another sync server (self-hosted), and single sign-on.
  renderMore() {
    return html`<details class="sync-more">
      <summary>More options</summary>
      <div class="sync-form">
        ${this.serverField()}
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("sso", () => this.browserSignIn("sso"))}>Sign in with SSO</lit-material-button>
      </div>
      <p class="hint">HITOP Cloud is the default server. Use your company's address for a self-hosted one.</p>
    </details>`;
  }

  serverField() {
    return html`<lit-material-text-field id="sync-server" variant="outlined" label="Sync server" .value=${this.server}
      @input=${(e) => (this.server = String(e.target.value ?? "").trim())}
      @change=${(e) => this.loadMethods(String(e.target.value ?? "").trim())}></lit-material-text-field>`;
  }

  emailField() {
    return html`<lit-material-text-field id="sync-email" variant="outlined" label="Email" type="email" autocomplete="email" .value=${this.email}
      @input=${(e) => (this.email = String(e.target.value ?? "").trim())}
      @keydown=${(e) => e.key === "Enter" && this.mode === "password" && !this.continued && this.next()}></lit-material-text-field>`;
  }

  // Continue: the address is there, so the password field opens.
  next() {
    this.error = null;
    if (!this.field("sync-email")) {
      this.error = "Enter your email address";
      return;
    }
    this.continued = true;
    this.updateComplete.then(() => this.querySelector("#sync-password")?.focus());
  }

  renderSignInForms() {
    const mode = this.mode;
    const pending = this.sync.pending ?? this.sync.pendingSignUp ?? this.sync.pendingReset;
    const head = html`<h2>${mode === "signup" ? "Confirm your address" : mode === "forgot" ? "Reset your password" : "Sign in to HITOP"}</h2>
      ${mode === "signup" || mode === "forgot" ? nothing : html`<p class="page-lead">Your collections, saved and on every device you sign in on. Share them with your team, keep them in GitHub. New here? The same steps create your account.</p>`}`;
    if (mode === "paste") {
      return html`${head}
        <p class="hint">Sign-in page (open it in a browser if it didn't open): <a href=${this.ssoUrl} target="_blank" rel="noopener noreferrer">open</a></p>
        <pre class="code-preview" aria-label="SSO sign-in link">${this.ssoUrl}</pre>
        <div class="sync-form">
          <lit-material-text-field id="sync-sso-code" variant="outlined" label="Code shown after signing in"></lit-material-text-field>
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)}
            @click=${() => this.run("signin", async () => {
              if (this.ssoKind === "sso") await this.sync.finishSso(this.field("sync-sso-code"));
              else await this.sync.finishOAuth(this.field("sync-sso-code"));
              this.ssoUrl = null;
              this.mode = "password";
              if (this.ssoKind !== "sso") this.note = "Signed in. Google and GitHub don't unlock your encrypted workspaces: on a new device, unlock them with your password or your recovery phrase below.";
              await this.refresh();
            })}>Sign in</lit-material-button>
          <lit-material-button variant="text" @click=${() => {
            this.ssoUrl = null;
            this.mode = "password";
          }}>Cancel</lit-material-button>
        </div>`;
    }
    if (pending?.email && !this.email) this.email = pending.email;
    const fields = this.emailField();
    if (mode === "signup") {
      return html`${head}
        <p class="hint">We mailed a confirmation code to ${this.sync.pendingSignUp?.email ?? "you"}. Paste it (or the whole link) here to finish.</p>
        <div class="sync-form">
          <lit-material-text-field id="sync-signup-code" variant="outlined" label="Code from the confirmation mail"></lit-material-text-field>
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)}
            @click=${() => this.run("signin", async () => {
              const r = await this.sync.finishSignUp(this.field("sync-signup-code"));
              this.mode = "password";
              this.signedInNote(r);
              await this.refresh();
            })}>Confirm</lit-material-button>
          <lit-material-button variant="text" @click=${() => (this.mode = "password")}>Back</lit-material-button>
        </div>`;
    }
    if (mode === "forgot") {
      return html`${head}
        <p class="sync-note security" aria-label="Reset warning">A reset signs you in again, but it can't unlock your end-to-end encrypted workspaces on a new device: your old password sealed their keys and nobody, the server included, can open them without it. Keep them with your recovery phrase (Recover a lost device, after signing in) or a device that is still signed in. Remembered it? Go back and sign in.</p>
        <div class="sync-form">${fields}
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)}
            @click=${() => this.run("forgot", async () => {
              await this.sync.forgotPassword((this.server || DEFAULT_SERVER), this.field("sync-email"));
              this.resetSent = true;
              this.note = "If the address has an account, a reset code is on its way.";
            })}>Send reset code</lit-material-button>
        </div>
        ${this.resetSent
          ? html`<div class="sync-form">
              <lit-material-text-field id="sync-reset-code" variant="outlined" label="Code from the reset mail"></lit-material-text-field>
              <lit-material-text-field id="sync-reset-password" variant="outlined" label="New password" type="password"></lit-material-text-field>
              <lit-material-text-field id="sync-reset-password2" variant="outlined" label="Repeat the new password" type="password"></lit-material-text-field>
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)}
                @click=${() => this.run("signin", async () => {
                  const next = this.rawField("sync-reset-password");
                  if (next !== this.rawField("sync-reset-password2")) throw new Error("The two new passwords differ");
                  const r = await this.sync.resetPassword(this.field("sync-reset-code"), next);
                  this.mode = "password";
                  this.resetSent = false;
                  this.note = r.keptDevice
                    ? `Password reset. This device still has your workspaces and sealed them under the new password.${r.bundleLost ? " The old password's key bundle was deleted." : ""}`
                    : `Password reset.${r.bundleLost ? " Your old password's key bundle is gone: it can't be opened with the new one." : ""} This device has no workspace keys: use your recovery phrase (Recover a lost device, below), or sign in on a device that still has them and set the password there.`;
                  await this.refresh();
                })}>Reset password</lit-material-button>
            </div>`
          : nothing}
        <lit-material-button variant="text" @click=${() => (this.mode = "password")}>Back to sign in</lit-material-button>
        ${this.renderMore()}`;
    }
    if (mode === "code") {
      return html`${head}
        <div class="sync-form">${fields}
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)}
            @click=${() => this.run("code", async () => {
              await this.sync.startSignIn((this.server || DEFAULT_SERVER), this.field("sync-email"));
              this.awaitingCode = true;
              this.note = "Check your mail for the sign-in code.";
            })}>Send code</lit-material-button>
        </div>
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
        ${this.renderAlternatives()}`;
    }
    if (!this.continued) {
      return html`${head}
        <div class="sync-form">${fields}
          <lit-material-button variant="filled" @click=${() => this.next()}>Continue</lit-material-button>
        </div>
        ${this.renderAlternatives()}`;
    }
    return html`${head}
      <div class="sync-form">${fields}
        <lit-material-text-field id="sync-password" variant="outlined" label="Password" type="password" autocomplete="current-password"
          @keydown=${(e) => e.key === "Enter" && this.querySelector("#sync-signin")?.click()}></lit-material-text-field>
        <lit-material-button id="sync-signin" variant="filled" ?disabled=${Boolean(this.busy)}
          @click=${() => this.run("signin", async () => {
            const r = await this.sync.passwordSignIn((this.server || DEFAULT_SERVER), this.field("sync-email"), this.rawField("sync-password"));
            this.signedInNote(r);
            await this.refresh();
          })}>Sign in</lit-material-button>
      </div>
      <div class="page-actions">
        <lit-material-button variant="text" @click=${() => {
          this.mode = "forgot";
          this.resetSent = false;
        }}>Forgot password?</lit-material-button>
        <span class="hint">No account yet?</span>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)}
          @click=${() => this.run("signup", async () => {
            await this.sync.signUp((this.server || DEFAULT_SERVER), this.field("sync-email"), this.rawField("sync-password"));
            this.mode = "signup";
            this.note = "Check your mail for the confirmation code.";
          })}>Create account</lit-material-button>
      </div>
      ${this.renderAlternatives()}`;
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

  // The account menu's Join dialog: accept an invite code.
  renderJoin() {
    return html`<section aria-label="Join">
      <div class="sync-form">
        <lit-material-text-field id="sync-invite-code" variant="outlined" label="Invite code"></lit-material-text-field>
        <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("accept", async () => {
          const r = await this.sync.accept(this.field("sync-invite-code"));
          this.note = r.inviter
            ? `Invite accepted (${r.role}). Tell the owner your fingerprint ${this.fingerprint}; once they add you, the collection is ready to open on your Account page. Its key will be accepted from the inviter's device, fingerprint ${r.inviter}.`
            : `Invite accepted (${r.role}). Tell the owner your fingerprint ${this.fingerprint}; once they add you, the collection is ready to open on your Account page. This code doesn't name the inviter's key: you'll be asked to compare the fingerprint of the device that sends the key.`;
        })}>Accept invite</lit-material-button>
      </div>
    </section>`;
  }

  // Account page rows: collections shared with you that aren't open on this device yet.
  renderWaiting() {
    const linked = new Set(this.shared().map((c) => this.sync.link(c.id).workspace));
    const joinable = (this.remote ?? []).filter((w) => !linked.has(w.id));
    return joinable.map((w) => html`<div class="account-row" aria-label="Shared with you">
      <span class="account-key">${icon("users-three")} Shared with you</span>
      <span class="account-value">Collection <code>${w.id.slice(0, 8)}</code> <span class="hint">${w.role}</span></span>
      <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("join", async () => {
        const id = await this.sync.join(w.id);
        this.note = `Opened ${this.store.collection(id)?.name ?? "the shared collection"}.`;
        await this.refresh();
      })}>Open here</lit-material-button>
    </div>`);
  }

  // The People tab of a collection's Share dialog.
  renderPeople() {
    const c = this.collection;
    if (!this.sync.account()) {
      return html`<p>Sign in to share ${c.name} with people.</p>
        <lit-material-button variant="filled" @click=${() => {
          const app = this.closest("hitop-app");
          app?.closeDialog();
          app?.go("sync");
        }}>Sign in</lit-material-button>`;
    }
    const link = this.sync.link(c.id);
    if (!link) {
      const git = this.store.git?.linkOf?.(c);
      return html`<p>Invite people to work on ${c.name} with you. It becomes a shared collection: <b>end-to-end encrypted</b>, so only its members can read it, not even HITOP Cloud.</p>
        ${git
          ? html`<p class="sync-note">${c.name} is saved to GitHub. Stop saving it to GitHub first (GitHub tab): a shared collection stays encrypted, and GitHub would need it readable.</p>`
          : html`<lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("share", async () => {
              await this.sync.share(c.id);
              this.note = `Shared ${c.name}. Invite people below.`;
              await this.refresh();
              await this.loadTeam();
            })}>${icon("users-three")} Share with people</lit-material-button>`}`;
    }
    const conflicts = this.conflicts[c.id] ?? [];
    const role = (this.remote ?? []).find((w) => w.id === link.workspace)?.role ?? this.team?.role;
    return html`
      <div class="sync-form">
        <span>${icon("users-three")} Shared, end-to-end encrypted${role ? html` <span class="hint">· you are ${role}</span>` : nothing}</span>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.syncOne(c)}>${icon("arrows-clockwise")} Sync now</lit-material-button>
      </div>
      ${conflicts.length ? html`<ul class="sync-conflicts" aria-label=${`Conflicts in ${c.name}`}>${conflicts.map((k) => this.renderConflict(c, k))}</ul>` : nothing}
      ${this.team ? this.renderTeam(c) : html`<p class="hint" aria-busy="true">Loading members…</p>`}
      <details class="sync-advanced" @toggle=${(e) => e.target.open && !this.history && role && this.openHistory(c)}>
        <summary>Advanced: run history, CI runner, audit log</summary>
        ${this.history?.id === c.id ? this.renderHistory(c, role) : nothing}
        ${this.team?.role === "owner" ? html`${this.renderRuns(c)}${this.renderAudit(c)}` : nothing}
      </details>`;
  }

  // --- team -------------------------------------------------------------------------

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
      <h3>People</h3>
      <ul class="sync-list">
        ${t.members.map((m) => {
          const who = m.runner ? "Runner credential" : m.email;
          const me = m.userId === this.sync.account().userId;
          return html`<li class="sync-member">
            <div class="sync-form">
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
                        if (r.sharesRekeyed || r.sharesNotRekeyed) this.note += ` Shared environment values: ${r.sharesRekeyed} re-keyed${r.sharesNotRekeyed ? `, ${r.sharesNotRekeyed} not shared with this device (their sharer re-shares them)` : ""}. ${who} could see those values: rotate the shared secrets this person could see where they come from.`;
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
      <p class="page-lead">Run this collection on a schedule from your own CI or cron: a runner credential pulls the workspace read-only (<code>hitop sync pull</code> with <code>HITOP_SYNC_RUNNER</code>), then <code>hitop run --sync-report</code>, which uploads the run to the workspace's history (Run history): a status row and the report sealed end to end. The runner decrypts on your machine, so the server still sees only ciphertext and status rows.</p>
      <p class="hint">Running on the sync server instead would mean handing the server a key that decrypts the workspace: it would then see your requests, responses and environment values while it runs them. hitop doesn't do that unless you set it up yourself (see the guide); prefer a runner you control.</p>
      <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("runner", async () => {
        this.runner = await this.sync.createRunner(c.id, "runner");
        await this.reloadTeam(c);
      })}>Create runner credential</lit-material-button>
      ${this.runner
        ? html`<p class="hint">Shown once. It decrypts this workspace: put it in your CI's secret store as HITOP_SYNC_RUNNER, nowhere else. Remove the runner under People to revoke it.</p>
            <pre class="code-preview" aria-label="Runner credential">${this.runner.credential}</pre>`
        : nothing}`;
  }

  // --- run history and alerts (HITOP Cloud C4) ---------------------------------------------

  async openHistory(c) {
    if (this.history?.id === c.id) {
      this.history = null;
      return;
    }
    await this.run("history", async () => {
      const runs = await this.sync.runs(c.id);
      const shares = await this.sync.sharedEnvironments(c.id).catch(() => null);
      this.history = { id: c.id, runs, report: null, alerts: null, shares };
    });
  }

  async openReport(c, id) {
    await this.run("report", async () => {
      this.history = { ...this.history, report: { id, ...(await this.sync.runReport(c.id, id)) } };
    });
  }

  renderReport() {
    const r = this.history.report;
    if (!r.report) return html`<p class="hint">Run ${r.id} has a status row only (this plan keeps no sealed reports, or the runner sent none).</p>`;
    const rep = r.report;
    return html`<div class="sync-run" aria-label=${`Report of run ${r.id}`}>
      ${r.mismatch ? html`<p class="sync-note security">${icon("warning-circle")} The server's status row (${r.row.status}) doesn't match the sealed report: the report is what ran.</p>` : nothing}
      <p><b>${rep.passed ? "Passed" : "Failed"}</b> at ${rep.startedAt}, ${rep.durationMs} ms, ${rep.summary.requests.passed}/${rep.summary.requests.total} requests passed</p>
      <ul class="sync-list">
        ${rep.collections.flatMap((col) => col.iterations.flatMap((it) => it.requests.map((q) => html`<li class="sync-row"><span>${q.passed ? "✓" : "✗"} <code>${q.method}</code> ${q.name}</span><span class="hint">${q.status ?? "-"} · ${q.durationMs} ms${q.failure || q.error ? ` · ${q.failure || q.error}` : ""}</span></li>`)))}
      </ul>
    </div>`;
  }

  renderAlerts(c) {
    const a = this.history.alerts;
    if (!a) {
      return html`<lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("alerts", async () => {
        this.history = { ...this.history, alerts: await this.sync.alertSettings(c.id) };
      })}>Alert settings</lit-material-button>`;
    }
    const s = a.settings;
    const channels = a.plan?.alertChannels ?? ["email"];
    const hooks = (kind) => s.webhooks.filter((w) => w.kind === kind).map((w) => w.url).join("\n");
    return html`<div class="sync-alerts" aria-label=${`Alerts of ${c.name}`}>
      <p class="hint">Alerts say a series (collection and environment) failed, passes again, or missed its run: status and a link, never a response, body or secret value. This plan: ${channels.join(", ")}.</p>
      <lit-material-text-field id="sync-alert-email" variant="outlined" label="Mail to members (comma separated)" .value=${s.email.join(", ")}></lit-material-text-field>
      ${channels.includes("webhook") ? html`<lit-material-text-field id="sync-alert-webhook" variant="outlined" label="Webhook URLs (https, one per line)" .value=${hooks("webhook")}></lit-material-text-field>` : nothing}
      ${channels.includes("slack") ? html`<lit-material-text-field id="sync-alert-slack" variant="outlined" label="Slack incoming webhook URLs (one per line)" .value=${hooks("slack")}></lit-material-text-field>` : nothing}
      <lit-material-text-field id="sync-alert-missed" variant="outlined" type="number" label="Missed-run alert after minutes (0 = off)" .value=${String(s.missedAfterMinutes ?? 0)}></lit-material-text-field>
      <label class="sync-check"><input type="checkbox" id="sync-alert-names" .checked=${Boolean(s.failedNames)}> Name failed requests in status rows and alerts</label>
      <label class="sync-check"><input type="checkbox" id="sync-alert-rows" .checked=${s.statusRows !== false}> Keep status rows (off: history only, no alerts)</label>
      <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("alerts", async () => {
        const lines = (id) => (this.rawField(id) ?? "").split(/[\n,]/).map((x) => x.trim()).filter(Boolean);
        const settings = {
          email: lines("sync-alert-email"),
          webhooks: [...lines("sync-alert-webhook").map((url) => ({ kind: "webhook", url })), ...lines("sync-alert-slack").map((url) => ({ kind: "slack", url }))],
          missedAfterMinutes: Number(this.rawField("sync-alert-missed") || 0),
          failedNames: this.querySelector("#sync-alert-names")?.checked ?? false,
          statusRows: this.querySelector("#sync-alert-rows")?.checked ?? true,
        };
        const r = await this.sync.saveAlerts(c.id, settings);
        this.history = { ...this.history, alerts: { ...a, settings: r.settings } };
        this.note = "Alert settings saved.";
      })}>Save alerts</lit-material-button>
    </div>`;
  }

  renderHistory(c, role) {
    const h = this.history;
    const when = (t) => new Date(t * 1000).toISOString().replace("T", " ").replace(/:\d\d\.\d+Z$/, " UTC");
    const runs = h.runs?.runs ?? [];
    return html`<div class="sync-history" aria-label=${`Run history of ${c.name}`}>
      <h3>Run history <span class="hint">kept ${h.runs?.historyDays ?? 0} days${h.runs?.sealedHistory === false ? ", status rows only on this plan" : ""}</span></h3>
      <p class="hint">Runs your CI or cron uploaded (<code>hitop run --sync-report</code>): the server keeps status rows and the sealed report, which opens only on members' devices.</p>
      ${h.runs?.series?.length
        ? html`<p aria-label="Last status">${h.runs.series.map((s) => html`<span class=${`chip status-${s.status}`}>${s.collection}${s.environment ? ` (${s.environment})` : ""}: ${s.status}</span> `)}</p>`
        : nothing}
      ${runs.length
        ? html`<table class="sync-runs" aria-label=${`Runs of ${c.name}`}>
            <thead><tr><th>Time</th><th>Status</th><th>Collection</th><th>Requests</th><th>Duration</th><th></th></tr></thead>
            <tbody>
              ${runs.map((r) => html`<tr>
                <td>${when(r.createdAt)}</td><td><b>${r.status}</b>${r.runner ? html` <span class="hint">runner</span>` : nothing}</td>
                <td>${r.collection}${r.environment ? ` (${r.environment})` : ""}${r.failedNames?.length ? html`<br><span class="hint">${r.failedNames.join(", ")}</span>` : nothing}</td>
                <td>${r.status === "sealed" ? "-" : `${r.requests - r.failed}/${r.requests}`}</td><td>${r.status === "sealed" ? "-" : `${r.durationMs} ms`}</td>
                <td>${r.hasReport ? html`<lit-material-button variant="text" @click=${() => this.openReport(c, r.id)}>Open report</lit-material-button>` : html`<span class="hint">status only</span>`}</td>
              </tr>`)}
            </tbody>
          </table>`
        : html`<p class="hint">No runs yet.</p>`}
      ${h.report ? this.renderReport() : nothing}
      ${h.shares?.shared?.length || h.shares?.closed
        ? html`<h3>Shared environment values</h3>
            <p class="hint">Opt-in, end to end: secret values an owner or editor shared (<code>hitop sync secrets share</code>) for the devices they picked. Your runs and sends use them where you have no value of your own; the server can't read them.</p>
            <ul class="sync-list">${(h.shares.shared ?? []).map((x) => html`<li class="sync-row"><span><b>${x.environment}</b> (version ${x.version}): ${x.names.join(", ")}</span><span class="hint">${x.devices} device(s)</span></li>`)}</ul>
            ${h.shares.closed ? html`<p class="hint">${h.shares.closed} shared environment(s) not shared with this device.</p>` : nothing}`
        : nothing}
      ${role === "owner" ? this.renderAlerts(c) : nothing}
    </div>`;
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

  disconnectedCallback() {
    super.disconnectedCallback();
    this.store?.library?.removeEventListener("change", this.onLibrary);
  }

  // HITOP Cloud's personal library (library.js): its save state next to the address. The switch
  // is on the Settings page (Privacy).
  renderLibrary() {
    const st = this.store?.library?.status;
    if (!st) return nothing;
    if (!st.on) return html`<span class="hint sync-library">Kept in this browser only</span>`;
    const when = st.savedAt ? new Date(st.savedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "";
    return html`<span class=${st.error ? "sync-note" : "hint"} role="status" aria-label="Cloud library">${st.error
      ? html`${icon("warning-circle")} ${st.error}`
      : html`${icon("check-circle")} ${st.saving ? "Saving to HITOP Cloud…" : `Saved to HITOP Cloud${when ? ` at ${when}` : ""}`}`}</span>`;
  }

  renderMessages() {
    return html`${this.error ? html`<lit-material-alert variant="error" role="alert">${this.error}</lit-material-alert>` : nothing}
      ${this.limitUrl && platform().name !== "tauri"
        ? html`<p class="hint" aria-label="Plan limit">A plan with more room: <a href=${this.limitUrl} target="_blank" rel="noopener noreferrer">Manage plan</a></p>`
        : nothing}
      ${this.untrusted ? this.renderUntrusted() : nothing}
      ${this.warnings?.length
        ? html`<ul class="sync-warnings" aria-label="Key warnings">${this.warnings.map((w) => html`<li class=${w.kind === "senderKeyChanged" ? "sync-note security" : "hint"}>${icon("warning-circle")} ${w.message}</li>`)}</ul>`
        : nothing}
      ${this.note ? html`<p class="sync-note" role="status">${this.note}</p>` : nothing}
      ${this.busy ? html`<p class="hint" aria-busy="true">Working…</p>` : nothing}`;
  }

  render() {
    if (this.collection) return html`<div class="sync-people">${this.renderMessages()}${this.renderPeople()}</div>`;
    if (this.join) return html`<div class="sync-join">${this.renderMessages()}${this.renderJoin()}</div>`;
    const a = this.sync.account();
    if (!a) {
      return html`<div class="page sync-page sync-signin">${this.renderMessages()}${this.renderSignIn()}</div>`;
    }
    const web = platform().name !== "tauri";
    const r = this.recovery;
    const unset = r && !r.password && !r.set;
    return html`<div class="page sync-page account-page">
      <header class="page-header"><h1>Account</h1>
        <div class="page-actions">
          <lit-material-button variant="outlined" @click=${() => this.run("signout", async () => {
            await this.sync.signOut();
            this.remote = null;
            // The next sign-in starts at the first step again.
            this.continued = false;
            this.mode = "password";
            this.awaitingCode = false;
          })}>Sign out</lit-material-button>
        </div>
      </header>
      <p class="account-lead"><b>${a.email}</b>${a.server !== DEFAULT_SERVER ? html` <span class="hint">on ${a.server}</span>` : nothing} ${this.renderLibrary()}</p>
      ${this.renderMessages()}
      <div class="account-rows">
        ${this.renderWaiting()}
        ${platform().name === "web" ? html`<hitop-github .store=${this.store} .version=${this.store.version}></hitop-github>` : nothing}
        ${a.server === DEFAULT_SERVER && web
          ? html`<div class="account-row" aria-label="Plan">
              <span class="account-key">${icon("cloud")} Plan & billing</span>
              <span class="account-value">${this.store.currentOrgInfo ? orgLabel(this.store.currentOrgInfo) : ""}</span>
              <lit-material-button variant="text" href=${this.app?.planUrl?.() ?? "https://cloud.hitop.rws8.tech/"} target="_blank">Manage ${icon("arrow-square-out")}</lit-material-button>
            </div>`
          : nothing}
      </div>
      <details class="account-security">
        <summary>${icon("lock")} Password & recovery${unset ? html` <span class="hint">· not set up</span>` : nothing}</summary>
        ${this.renderPassword()}
        ${this.renderRecovery()}
        <section aria-label="This device" class="sync-device">
          <h3>This device</h3>
          <p class="hint">${a.deviceName} · <code class="fingerprint" aria-label="Your fingerprint">${this.fingerprint ?? ""}</code></p>
        </section>
      </details>
    </div>`;
  }
}

customElements.define("hitop-sync", HitopSync);

// Not in the sidebar: the top bar's account menu opens it.
export const syncPage = { kind: "sync", label: "Account", icon: "user-circle", sidebar: false, render: (store, app) => html`<hitop-sync .store=${store} .version=${store.version} .app=${app}></hitop-sync>` };
