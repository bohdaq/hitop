// <hitop-cloud>: the HITOP Cloud portal. Sign in (email + password, Google, GitHub, mail code,
// passkey or single sign-on, all through the sync server), organizations, members and roles, usage against the plan's limits, the plan
// (read only until billing opens), access tokens and passkeys. Light DOM, hash routes:
//   #/                 organizations          #/orgs/<id>        an organization
//   #/orgs/<id>/plan   its plan               #/tokens           access tokens
//   #/account          the account            #sso=<code>        back from single sign-on
import { LitElement, html, nothing } from "lit";
import { Api, ApiError, DEFAULT_SERVER, changePassword, normalizeServer, passkeySignIn, passwordReset, passwordSignIn, passwordSignUp, registerPasskey, ssoFinish, ssoStart } from "./api.js";

const STORE = "hitop_cloud";

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

export function bytes(n) {
  if (n === null || n === undefined) return "no limit";
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${v} bytes` : `${v >= 10 || Number.isInteger(v) ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

const date = (secs) => (secs ? new Date(secs * 1000).toISOString().slice(0, 10) : "never");

const METRICS = [
  ["seats", "People (seats)", String],
  ["workspaces", "Shared workspaces", String],
  ["storageBytes", "Encrypted storage", bytes],
  ["devices", "Devices per person (most used)", String],
  ["runners", "Runner credentials", String],
  ["invitesPerDay", "Invites today", String],
];

const PLAN_NOTES = {
  free: "Free: 3 people, 3 shared workspaces, 100 MB, 3 devices per person, 5 runner credentials.",
  pro: "Pro: one person and two guests, 25 workspaces, 2 GB, 10 devices, 10 runner credentials.",
  team: "Team: per seat, unlimited workspaces, 10 GB + 1 GB per seat, 5 free guest viewers.",
  business: "Business: per seat, 50 GB + 2 GB per seat; hosted SSO and audit export come with it.",
  enterprise: "Enterprise: limits as the contract says.",
};

export class HitopCloud extends LitElement {
  static properties = {
    account: { state: true },
    route: { state: true },
    data: { state: true },
    busy: { state: true },
    error: { state: true },
    note: { state: true },
    awaitingCode: { state: true },
    sso: { state: true },
    methods: { state: true },
    mode: { state: true },
    resetSent: { state: true },
    newToken: { state: true },
  };

  constructor() {
    super();
    this.account = load();
    this.route = location.hash;
    this.data = {};
    this.busy = "";
    this.error = "";
    this.note = "";
    this.awaitingCode = false;
    this.sso = null;
    this.methods = null;
    // "password" (default), "code", "signup" (waiting for the confirmation code), "forgot".
    this.mode = "password";
    this.resetSent = false;
    this.newToken = null;
  }

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.textContent = "";
    window.addEventListener("hashchange", () => this.go(location.hash));
    const sso = /^#sso=(.+)$/.exec(location.hash);
    if (sso) {
      history.replaceState(null, "", location.pathname);
      this.run("sso", async () => {
        const { server, reply } = await ssoFinish(decodeURIComponent(sso[1]));
        this.signedIn(server, reply);
      });
    } else {
      this.go(location.hash);
    }
    this.checkSso();
  }

  get api() {
    return new Api(this.account.server || DEFAULT_SERVER, this.account.session || null);
  }

  get signedInNow() {
    return Boolean(this.account.session) && (!this.account.expiresAt || this.account.expiresAt * 1000 > Date.now());
  }

  async run(what, fn) {
    this.busy = what;
    this.error = "";
    try {
      await fn();
    } catch (e) {
      if (e instanceof ApiError && e.status === 401 && this.account.session) {
        this.account = { server: this.account.server };
        save(this.account);
        this.error = "Your session ended. Sign in again.";
      } else {
        this.error = e.message || String(e);
      }
    } finally {
      this.busy = "";
    }
  }

  field(id) {
    return this.querySelector(`#${id}`)?.value?.trim() ?? "";
  }

  async go(hash) {
    this.route = hash || "#/";
    this.newToken = null;
    if (!this.signedInNow) return;
    await this.run("load", async () => {
      const parts = this.route.replace(/^#\/?/, "").split("/").filter(Boolean);
      if (parts[0] === "orgs" && parts[1]) {
        const id = decodeURIComponent(parts[1]);
        const [org, members, usage] = await Promise.all([
          this.api.get(`/v1/orgs/${encodeURIComponent(id)}`),
          this.api.get(`/v1/orgs/${encodeURIComponent(id)}/members`),
          this.api.get(`/v1/orgs/${encodeURIComponent(id)}/usage`),
        ]);
        this.data = { org, members, usage };
      } else if (parts[0] === "tokens") {
        this.data = { tokens: (await this.api.get("/v1/me/tokens")).tokens };
      } else if (parts[0] === "account") {
        this.data = { me: await this.api.get("/v1/me") };
      } else {
        this.data = { orgs: (await this.api.get("/v1/orgs")).orgs };
      }
    });
  }

  signedIn(server, reply) {
    this.account = { server, session: reply.session, email: reply.user.email, expiresAt: reply.expiresAt };
    save(this.account);
    this.awaitingCode = false;
    this.note = "";
    this.go(this.route && !this.route.startsWith("#sso") ? this.route : "#/");
  }

  async checkSso() {
    try {
      const server = normalizeServer(this.account.server || DEFAULT_SERVER);
      this.sso = await new Api(server).get("/v1/auth/sso");
      this.methods = await new Api(server).get("/v1/auth/methods").catch(() => null);
    } catch {
      this.sso = null;
    }
  }

  updated() {
    // The header's navigation lives outside the element (index.html).
    const nav = document.getElementById("portal-nav");
    if (nav) {
      nav.replaceChildren();
      if (this.signedInNow) {
        for (const [href, label] of [["#/", "Organizations"], ["#/tokens", "Access tokens"], ["#/account", "Account"]]) {
          const a = document.createElement("a");
          a.href = href;
          a.textContent = label;
          if (this.route === href || (href === "#/" && this.route.startsWith("#/orgs"))) a.setAttribute("aria-current", "page");
          nav.append(a);
        }
      }
    }
    const commit = document.getElementById("portal-commit");
    if (commit && typeof __HITOP_COMMIT__ === "string") {
      commit.textContent = __HITOP_COMMIT__;
      commit.href = `https://github.com/bohdaq/hitop/tree/${__HITOP_COMMIT__}/cloud/portal`;
    }
  }

  render() {
    return html`
      <div aria-busy=${this.busy ? "true" : "false"}>
        ${this.error ? html`<p class="portal-error" role="alert">${this.error}</p>` : nothing}
        ${this.note ? html`<p class="portal-note" role="status">${this.note}</p>` : nothing}
        ${this.signedInNow ? this.renderSignedIn() : this.renderSignIn()}
      </div>`;
  }

  // --- sign in ---------------------------------------------------------------------------

  renderSignIn() {
    const mode = this.mode;
    const server = () => normalizeServer(this.field("server"));
    const fields = html`
      <lit-material-text-field id="server" variant="outlined" label="Sync server" .value=${this.account.server || DEFAULT_SERVER}></lit-material-text-field>
      <lit-material-text-field id="email" variant="outlined" label="Email" type="email" .value=${this.account.email || ""}></lit-material-text-field>`;
    const remember = (s) => {
      this.account = { server: s, email: this.field("email") };
      save(this.account);
    };
    const others = html`<div class="portal-form portal-others">
      ${mode === "code"
        ? html`<lit-material-button variant="text" @click=${() => (this.mode = "password")}>Use a password instead</lit-material-button>`
        : html`<lit-material-button variant="text" @click=${() => (this.mode = "code")}>Use a code instead</lit-material-button>`}
      <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("passkey", async () => {
        const s = server();
        const reply = await passkeySignIn(new Api(s), this.field("email"));
        this.signedIn(s, reply);
      })}>Sign in with a passkey</lit-material-button>
      ${this.methods?.google ? this.providerButton("google", "Continue with Google") : nothing}
      ${this.methods?.github ? this.providerButton("github", "Continue with GitHub") : nothing}
      ${this.sso?.enabled ? this.providerButton("sso", `Sign in with ${this.sso.name || "SSO"}`) : nothing}
    </div>`;
    const lead = html`<p class="page-lead">Use the account you sync with: the same address in hitop (Sync page, or <code>hitop sync login</code>). Your collections stay end-to-end encrypted; this portal only manages organizations, people, usage and tokens.</p>`;
    if (mode === "signup") {
      return html`<section aria-label="Sign in" class="portal-card">
        <h1>Confirm your address</h1>
        <p class="page-lead">We mailed a confirmation code to ${this.account.email}. Paste it (or the whole link) here to finish making your account.</p>
        <div class="portal-form">
          <lit-material-text-field id="signup-code" variant="outlined" label="Code from the confirmation mail"></lit-material-text-field>
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("signin", async () => {
            const s = this.account.server || DEFAULT_SERVER;
            const reply = await new Api(s).call("POST", "/v1/auth/password/verify", { token: this.field("signup-code").split("#").pop() });
            this.mode = "password";
            this.signedIn(s, reply);
            this.note = "Account ready. Sign in with the same password in hitop: your first device seals its keys with it, so the password unlocks them on your next devices.";
          })}>Confirm</lit-material-button>
          <lit-material-button variant="text" @click=${() => (this.mode = "password")}>Back</lit-material-button>
        </div>
      </section>`;
    }
    if (mode === "forgot") {
      return html`<section aria-label="Sign in" class="portal-card">
        <h1>Reset your password</h1>
        <p class="portal-warning" aria-label="Reset warning">A reset signs you in again, but it can't unlock your end-to-end encrypted workspaces on a new device: your old password sealed their keys, and nobody, the server included, can open them without it. Keep your recovery phrase or a device that is still signed in. Remembered it? Go back and sign in.</p>
        <div class="portal-form">${fields}
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("forgot", async () => {
            const s = server();
            await new Api(s).call("POST", "/v1/auth/password/forgot", { email: this.field("email") });
            remember(s);
            this.resetSent = true;
            this.note = "If the address has an account, a reset code is on its way.";
          })}>Send reset code</lit-material-button>
        </div>
        ${this.resetSent
          ? html`<div class="portal-form">
              <lit-material-text-field id="reset-code" variant="outlined" label="Code from the reset mail"></lit-material-text-field>
              <lit-material-text-field id="reset-password" variant="outlined" label="New password" type="password"></lit-material-text-field>
              <lit-material-text-field id="reset-password2" variant="outlined" label="Repeat the new password" type="password"></lit-material-text-field>
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("signin", async () => {
                const next = this.raw("reset-password");
                if (next !== this.raw("reset-password2")) throw new Error("The two new passwords differ");
                const s = this.account.server || DEFAULT_SERVER;
                const reply = await passwordReset(new Api(s), this.field("reset-code").split("#").pop(), next);
                this.mode = "password";
                this.resetSent = false;
                this.signedIn(s, reply);
                this.note = reply.bundleLost
                  ? "Password reset. The key bundle your old password sealed is gone: on a new device, use your recovery phrase or a device that still has your workspaces."
                  : "Password reset.";
              })}>Reset password</lit-material-button>
            </div>`
          : nothing}
        <lit-material-button variant="text" @click=${() => (this.mode = "password")}>Back to sign in</lit-material-button>
      </section>`;
    }
    if (mode === "code") {
      return html`<section aria-label="Sign in" class="portal-card">
        <h1>Sign in to HITOP Cloud</h1>
        ${lead}
        <div class="portal-form">${fields}
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("code", async () => {
            const s = server();
            await new Api(s).call("POST", "/v1/auth/email", { email: this.field("email") });
            remember(s);
            this.awaitingCode = true;
            this.note = "Check your mail for the sign-in code.";
          })}>Send code</lit-material-button>
        </div>
        ${this.awaitingCode
          ? html`<div class="portal-form">
              <lit-material-text-field id="code" variant="outlined" label="Code from the mail"></lit-material-text-field>
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("signin", async () => {
                // A pasted link works too: the code is after the '#'.
                const code = this.field("code").split("#").pop();
                const s = this.account.server || DEFAULT_SERVER;
                this.signedIn(s, await new Api(s).call("POST", "/v1/auth/email/verify", { token: code }));
              })}>Sign in</lit-material-button>
            </div>`
          : nothing}
        ${others}
      </section>`;
    }
    return html`<section aria-label="Sign in" class="portal-card">
      <h1>Sign in to HITOP Cloud</h1>
      ${lead}
      <div class="portal-form">${fields}
        <lit-material-text-field id="password" variant="outlined" label="Password" type="password"></lit-material-text-field>
        <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("signin", async () => {
          const s = server();
          this.signedIn(s, await passwordSignIn(new Api(s), this.field("email"), this.raw("password")));
        })}>Sign in</lit-material-button>
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("signup", async () => {
          const s = server();
          await passwordSignUp(new Api(s), this.field("email"), this.raw("password"));
          remember(s);
          this.mode = "signup";
          this.note = "Check your mail for the confirmation code.";
        })}>Create account</lit-material-button>
      </div>
      <lit-material-button variant="text" @click=${() => {
        this.mode = "forgot";
        this.resetSent = false;
      }}>Forgot password?</lit-material-button>
      ${others}
    </section>`;
  }

  providerButton(kind, label) {
    return html`<lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run(kind, async () => {
      const s = normalizeServer(this.field("server"));
      this.account = { server: s };
      save(this.account);
      location.assign(await ssoStart(new Api(s), this.field("email"), kind));
    })}>${label}</lit-material-button>`;
  }

  raw(id) {
    return String(this.querySelector(`#${id}`)?.value ?? "");
  }

  renderSignedIn() {
    const parts = this.route.replace(/^#\/?/, "").split("/").filter(Boolean);
    let page;
    if (parts[0] === "orgs" && this.data.org) page = parts[2] === "plan" ? this.renderPlan() : this.renderOrg();
    else if (parts[0] === "tokens" && this.data.tokens) page = this.renderTokens();
    else if (parts[0] === "account" && this.data.me) page = this.renderAccount();
    else if (this.data.orgs) page = this.renderOrgs();
    else page = html`<p>Loading…</p>`;
    return html`
      <p class="portal-whoami">Signed in as <strong>${this.account.email}</strong> on ${this.account.server}
        <lit-material-button variant="text" @click=${() => this.run("signout", async () => {
          await this.api.call("POST", "/v1/auth/logout").catch(() => {});
          this.account = { server: this.account.server, email: this.account.email };
          save(this.account);
          this.data = {};
        })}>Sign out</lit-material-button></p>
      ${page}`;
  }

  // --- organizations ---------------------------------------------------------------------

  renderOrgs() {
    return html`
      <section aria-label="Organizations">
        <h1>Organizations</h1>
        <p class="page-lead">A plan, its seats and limits belong to an organization. Your personal one holds the workspaces you share without naming an organization (<code>hitop sync init --org &lt;name&gt;</code> picks another).</p>
        <table class="portal-table">
          <thead><tr><th>Name</th><th>Plan</th><th>Your role</th><th>Members</th><th>Workspaces</th></tr></thead>
          <tbody>
            ${this.data.orgs.map((o) => html`<tr>
              <td><a href="#/orgs/${o.id}">${o.personal ? "Personal" : o.name}</a></td>
              <td>${o.plan}</td><td>${o.role}</td><td>${o.members}</td><td>${o.workspaces}</td>
            </tr>`)}
          </tbody>
        </table>
        <h2>Create an organization</h2>
        <div class="portal-form">
          <lit-material-text-field id="org-name" variant="outlined" label="Organization name"></lit-material-text-field>
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("create-org", async () => {
            const org = await this.api.call("POST", "/v1/orgs", { name: this.field("org-name") });
            location.hash = `#/orgs/${org.id}`;
          })}>Create</lit-material-button>
        </div>
      </section>`;
  }

  renderUsage(u) {
    return html`
      <table class="portal-table" aria-label="Usage">
        <thead><tr><th>Limit</th><th>Used</th><th>Plan allows</th></tr></thead>
        <tbody>
          ${METRICS.map(([key, label, fmt]) => {
            const max = u.limits[key];
            const used = u.usage[key];
            const full = max !== null && max !== undefined && used >= max;
            return html`<tr data-metric=${key} class=${full ? "portal-full" : ""}>
              <td>${label}</td><td>${fmt(used)}</td><td>${max === null || max === undefined ? "no limit" : fmt(max)}</td>
            </tr>`;
          })}
        </tbody>
      </table>
      ${u.usage.guests ? html`<p class="hint">${u.usage.guests} guest viewer(s); ${u.limits.guestViewers} of them free on this plan.</p>` : nothing}
      ${u.enforced ? nothing : html`<p class="hint">This server enforces no plan limits (self-hosted).</p>`}`;
  }

  renderOrg() {
    const { org, members, usage } = this.data;
    const admin = org.role === "owner" || org.role === "admin";
    const owner = org.role === "owner";
    const base = `/v1/orgs/${encodeURIComponent(org.id)}`;
    const reload = () => this.go(this.route);
    return html`
      <section aria-label="Organization">
        <h1>${org.personal ? "Personal" : org.name}</h1>
        <p class="page-lead">Plan <strong>${org.plan}</strong>${org.seats ? html`, ${org.seats} seat(s)` : nothing}. You are ${org.role}. <a href="#/orgs/${org.id}/plan">Plan details</a></p>
        <h2>Usage</h2>
        ${this.renderUsage(usage)}
        <h2>Members</h2>
        ${org.personal
          ? html`<p class="hint">A personal organization has only you. People you share a workspace with count here (Free: 3 people). For a team, create an organization.</p>`
          : nothing}
        <table class="portal-table" aria-label="Members">
          <thead><tr><th>Email</th><th>Role</th><th></th></tr></thead>
          <tbody>
            ${members.members.map((m) => html`<tr>
              <td>${m.email}</td>
              <td>${admin && !org.personal
                ? html`<select aria-label="Role of ${m.email}" .value=${m.role} @change=${(e) => this.run("role", async () => {
                    await this.api.call("PUT", `${base}/members/${m.userId}`, { role: e.target.value });
                    await reload();
                  })}>${["member", "admin", "owner"].map((r) => html`<option value=${r} ?selected=${r === m.role}>${r}</option>`)}</select>`
                : m.role}</td>
              <td>${!org.personal && (admin || m.email === this.account.email)
                ? html`<lit-material-button variant="text" @click=${() => this.run("remove", async () => {
                    await this.api.call("DELETE", `${base}/members/${m.userId}`);
                    if (m.email === this.account.email) location.hash = "#/";
                    else await reload();
                  })}>${m.email === this.account.email ? "Leave" : "Remove"}</lit-material-button>`
                : nothing}</td>
            </tr>`)}
            ${members.invites.map((i) => html`<tr>
              <td>${i.email}</td><td>${i.role} (invited)</td>
              <td>${admin ? html`<lit-material-button variant="text" @click=${() => this.run("uninvite", async () => {
                await this.api.call("DELETE", `${base}/invites/${encodeURIComponent(i.email)}`);
                await reload();
              })}>Cancel invite</lit-material-button>` : nothing}</td>
            </tr>`)}
          </tbody>
        </table>
        ${admin && !org.personal
          ? html`<h2>Invite someone</h2>
            <p class="hint">An account joins at once; an address without one is invited (mailed) and joins when it first signs in. Either takes a seat. Workspaces are still shared from hitop: an organization role never opens a workspace.</p>
            <div class="portal-form">
              <lit-material-text-field id="member-email" variant="outlined" label="Email to invite" type="email"></lit-material-text-field>
              <select id="member-role" aria-label="Role">${["member", "admin", ...(owner ? ["owner"] : [])].map((r) => html`<option value=${r}>${r}</option>`)}</select>
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("invite", async () => {
                const r = await this.api.call("POST", `${base}/members`, { email: this.field("member-email"), role: this.querySelector("#member-role").value });
                await reload();
                this.note = r.status === "invited" ? `Invited ${r.email}${r.mailed ? " (mailed)" : ""}.` : `Added ${r.email}.`;
              })}>Invite</lit-material-button>
            </div>
            <h2>Settings</h2>
            <div class="portal-form">
              <lit-material-text-field id="org-rename" variant="outlined" label="Name" .value=${org.name}></lit-material-text-field>
              <lit-material-button variant="outlined" @click=${() => this.run("rename", async () => {
                await this.api.call("PUT", base, { name: this.field("org-rename") });
                await reload();
                this.note = "Renamed.";
              })}>Rename</lit-material-button>
              ${owner
                ? html`<lit-material-button variant="outlined" @click=${() => this.run("delete-org", async () => {
                    if (!confirm(`Delete the organization ${org.name}? It must have no workspaces.`)) return;
                    await this.api.call("DELETE", base);
                    location.hash = "#/";
                  })}>Delete organization</lit-material-button>`
                : nothing}
            </div>`
          : nothing}
      </section>`;
  }

  renderPlan() {
    const { org, usage } = this.data;
    return html`
      <section aria-label="Plan">
        <h1>Plan: ${org.plan}</h1>
        <p class="page-lead">${PLAN_NOTES[usage.planInForce] || ""}${usage.grandfathered ? " Your limits were raised to what you used before plans existed." : ""}</p>
        ${usage.planInForce !== usage.plan ? html`<p class="portal-error" role="alert">The ${usage.plan} subscription is ${usage.status}: Free limits apply until it is paid. Nothing was deleted.</p>` : nothing}
        ${this.renderUsage(usage)}
        <p>Paid plans (Pro, Team, Business) open soon; prices are not final. Over a limit, hitop refuses new workspaces, people, devices or uploads that grow storage. Reading and pulling never stop, and nothing is deleted.</p>
        <p><a href="#/orgs/${org.id}">Back to ${org.personal ? "Personal" : org.name}</a></p>
      </section>`;
  }

  // --- access tokens and the account -------------------------------------------------------

  renderTokens() {
    return html`
      <section aria-label="Access tokens">
        <h1>Access tokens</h1>
        <p class="page-lead">Personal access tokens call the sync API as you (scripts, integrations). Read tokens only read. A token is not a key: it can't decrypt anything.</p>
        ${this.newToken
          ? html`<div class="portal-card" role="status"><p>Copy the token now; it isn't shown again.</p><pre aria-label="New token">${this.newToken}</pre></div>`
          : nothing}
        <table class="portal-table" aria-label="Tokens">
          <thead><tr><th>Name</th><th>Scopes</th><th>Created</th><th>Expires</th><th>Last used</th><th></th></tr></thead>
          <tbody>
            ${this.data.tokens.map((t) => html`<tr>
              <td>${t.name}</td><td>${t.scopes.join(", ")}</td><td>${date(t.createdAt)}</td><td>${t.expired ? "expired" : date(t.expiresAt)}</td><td>${date(t.lastUsedAt)}</td>
              <td><lit-material-button variant="text" @click=${() => this.run("revoke", async () => {
                await this.api.call("DELETE", `/v1/me/tokens/${t.id}`);
                this.data = { tokens: (await this.api.get("/v1/me/tokens")).tokens };
                this.note = `Revoked ${t.name}.`;
              })}>Revoke</lit-material-button></td>
            </tr>`)}
          </tbody>
        </table>
        <h2>New token</h2>
        <div class="portal-form">
          <lit-material-text-field id="token-name" variant="outlined" label="Token name"></lit-material-text-field>
          <label><input type="checkbox" id="token-write" /> Write (changes too)</label>
          <select id="token-days" aria-label="Expires in">${[30, 90, 365].map((d) => html`<option value=${d} ?selected=${d === 90}>${d} days</option>`)}</select>
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("token", async () => {
            const scopes = this.querySelector("#token-write").checked ? ["read", "write"] : ["read"];
            const t = await this.api.call("POST", "/v1/me/tokens", { name: this.field("token-name"), scopes, expiresInDays: Number(this.querySelector("#token-days").value) });
            this.data = { tokens: (await this.api.get("/v1/me/tokens")).tokens };
            this.newToken = t.token;
          })}>Create token</lit-material-button>
        </div>
      </section>`;
  }

  renderAccount() {
    const me = this.data.me;
    return html`
      <section aria-label="Account">
        <h1>Account</h1>
        <p class="page-lead">${me.email}: ${me.keys.length} device key(s), ${me.passkeys.length} passkey(s).</p>
        <h2>Password</h2>
        <p class="hint" aria-label="Password status">${me.password?.set
          ? `Your account has a password.${me.bundles?.password ? " It also unlocks your device's keys on a new device; changing it here re-seals them." : ""}`
          : "No password yet. Add one to sign in with it; sign in with it in hitop once, and it also unlocks your encrypted workspaces on a new device."}</p>
        <div class="portal-form">
          ${me.password?.set ? html`<lit-material-text-field id="current-password" variant="outlined" label="Current password" type="password"></lit-material-text-field>` : nothing}
          <lit-material-text-field id="new-password" variant="outlined" label="New password" type="password"></lit-material-text-field>
          <lit-material-text-field id="new-password2" variant="outlined" label="Repeat the new password" type="password"></lit-material-text-field>
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("password", async () => {
            const next = this.raw("new-password");
            if (next !== this.raw("new-password2")) throw new Error("The two new passwords differ");
            await changePassword(this.api, me, this.raw("current-password"), next);
            this.data = { me: await this.api.get("/v1/me") };
            this.note = me.password?.set ? "Password changed. Your other devices were signed out; they sign in with the new one." : "Password set.";
          })}>${me.password?.set ? "Change password" : "Set password"}</lit-material-button>
        </div>
        <h2>Passkeys</h2>
        <ul>${me.passkeys.map((p) => html`<li>${p.name}</li>`)}</ul>
        <div class="portal-form">
          <lit-material-text-field id="passkey-name" variant="outlined" label="Passkey name"></lit-material-text-field>
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("passkey-add", async () => {
            await registerPasskey(this.api, this.field("passkey-name") || "portal");
            this.data = { me: await this.api.get("/v1/me") };
            this.note = "Passkey added: sign in with it next time.";
          })}>Add a passkey</lit-material-button>
        </div>
      </section>`;
  }
}

customElements.define("hitop-cloud", HitopCloud);
