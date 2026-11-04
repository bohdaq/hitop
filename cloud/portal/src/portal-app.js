// <hitop-cloud>: the HITOP Cloud portal. Sign in (email + password, Google, GitHub, mail code,
// passkey or single sign-on, all through the sync server), organizations, members and roles, usage against the plan's limits, the plan
// (buy Pro, Team or Business through Stripe, seats, the Team trial, the billing portal), access tokens
// and passkeys. Light DOM, hash routes:
//   #/                 organizations          #/orgs/<id>        an organization
//   #/orgs/<id>/plan   its plan               #/tokens           access tokens
//   #/account          the account            #sso=<code>        back from single sign-on
//   #/runs             workspaces' run history  #/workspaces/<ws>/runs[/<run>]  one workspace's
//                      (HITOP Cloud C4: status rows and alert settings; the sealed reports open in hitop)
//   #/orgs/<id>/admin  admin console (C5: every workspace, member and device of the org)
//   #/orgs/<id>/sso    single sign-on and domains (C5)   #/orgs/<id>/audit  audit export, anchors (C5)
//   #/orgs/<id>/scim   SCIM provisioning (C6: tokens, what each directory group gives)
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

// The owner's prices (spec/CLOUD_SERVICE.md Q4), in US dollars before tax; Stripe's checkout
// (Managed Payments: Stripe is the merchant of record) adds tax and may show the buyer's currency.
const PRICES = {
  pro: { annual: "$6 a month, billed yearly", monthly: "$8 a month" },
  team: { annual: "$10 per seat a month, billed yearly", monthly: "$12 per seat a month" },
  business: { annual: "$20 per seat a month, billed yearly", monthly: "$25 per seat a month" },
};

// Downloads `body` (a string or a Blob) as `name` (no server round trip: the portal already
// fetched it with the session).
function download(name, body, type = "application/json") {
  const blob = body instanceof Blob ? body : new Blob([body], { type });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

const STATUS_NOTES = {
  trialing: "Team trial",
  on_trial: "trial",
  active: "active",
  past_due: "payment failed, being retried",
  cancelled: "cancelled, runs until the period ends",
  paused: "paused: Free limits until it resumes",
  unpaid: "unpaid: Free limits",
  expired: "ended: Free limits",
  refunded: "refunded: Free limits",
  incomplete: "waiting for the first payment: Free limits",
};

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
    limitHit: { state: true },
  };

  constructor() {
    super();
    this.account = load();
    // ?server=<url> picks another sync server (self-hosted, tests); the hosted one otherwise.
    const asked = new URLSearchParams(location.search).get("server");
    if (asked && asked !== this.account.server) {
      this.account = { server: asked };
      save(this.account);
    }
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
    // The 402 body of the last refused create (its plan link is shown with the error).
    this.limitHit = null;
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
    this.limitHit = null;
    try {
      await fn();
    } catch (e) {
      if (e instanceof ApiError && e.status === 401 && this.account.session) {
        this.account = { server: this.account.server };
        save(this.account);
        this.error = "Your session ended. Sign in again.";
      } else {
        this.error = e.message || String(e);
        if (e instanceof ApiError && e.status === 402 && e.body?.error === "limit") this.limitHit = e.body;
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
        const [org, members, usage, billing] = await Promise.all([
          this.api.get(`/v1/orgs/${encodeURIComponent(id)}`),
          this.api.get(`/v1/orgs/${encodeURIComponent(id)}/members`),
          this.api.get(`/v1/orgs/${encodeURIComponent(id)}/usage`),
          // An older server has no billing: the plan page then shows the plan only.
          this.api.get(`/v1/orgs/${encodeURIComponent(id)}/billing`).catch(() => null),
        ]);
        this.data = { org, members, usage, billing };
        // C5 pages: a refusal (402 on a plan without them) shows with its "See plans" link.
        const sub = { admin: "admin", sso: "sso", audit: "audit/anchors", scim: "scim" }[parts[2]];
        if (sub) {
          const extra = await this.api.get(`/v1/orgs/${encodeURIComponent(id)}/${sub}`).then((v) => ({ v }), (e) => ({ e }));
          this.data = { ...this.data, [parts[2]]: extra.v ?? null };
          if (extra.e) throw extra.e;
          // The SCIM page offers the org's workspaces for a group's roles (from the admin console).
          if (parts[2] === "scim") {
            this.data.adminWorkspaces = (await this.api.get(`/v1/orgs/${encodeURIComponent(id)}/admin`).catch(() => ({ workspaces: [] }))).workspaces.map((w) => w.id);
          }
        }
      } else if (parts[0] === "runs") {
        this.data = { workspaces: (await this.api.get("/v1/workspaces")).workspaces };
      } else if (parts[0] === "workspaces" && parts[1]) {
        const ws = decodeURIComponent(parts[1]);
        const [runs, info] = await Promise.all([this.api.get(`/v1/workspaces/${ws}/runs?limit=200`), this.api.get(`/v1/workspaces/${ws}`)]);
        const alerts = info.role === "owner" ? await this.api.get(`/v1/workspaces/${ws}/alerts`).catch(() => null) : null;
        const deliveries = info.role === "owner" ? (await this.api.get(`/v1/workspaces/${ws}/alert-deliveries`).catch(() => ({ deliveries: [] }))).deliveries : [];
        this.data = { workspace: ws, runs, info, alerts, deliveries, run: parts[3] ? decodeURIComponent(parts[3]) : null };
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
        for (const [href, label] of [["#/", "Organizations"], ["#/runs", "Run history"], ["#/tokens", "Access tokens"], ["#/account", "Account"]]) {
          const a = document.createElement("a");
          a.href = href;
          a.textContent = label;
          if (this.route === href || (href === "#/" && this.route.startsWith("#/orgs")) || (href === "#/runs" && this.route.startsWith("#/workspaces"))) a.setAttribute("aria-current", "page");
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
        ${this.signedInNow ? html`${this.messages()}${this.renderSignedIn()}` : this.renderSignIn()}
      </div>`;
  }

  // --- sign in ---------------------------------------------------------------------------

  renderSignIn() {
    const mode = this.mode;
    // The hosted server (or the one the page was opened with: ?server=<url>).
    const server = () => normalizeServer(this.account.server || DEFAULT_SERVER);
    const email = html`<lit-material-text-field id="email" variant="outlined" label="Email" type="email" autocomplete="email" .value=${this.account.email || ""}></lit-material-text-field>`;
    const remember = (s) => {
      this.account = { server: s, email: this.field("email") };
      save(this.account);
    };
    const busy = Boolean(this.busy);
    const card = (title, lead, body) => html`<section aria-label="Sign in" class="auth">
      <div class="auth-card">
        <img class="auth-logo" src="https://hitop.rws8.tech/favicon.svg" alt="" width="40" height="40" />
        <h1>${title}</h1>
        ${lead ? html`<p class="auth-lead">${lead}</p>` : nothing}
        ${this.messages()}
        ${body}
      </div>
      <p class="auth-foot">The same account as hitop sync: the app's Sync page or <code>hitop sync login</code>.</p>
    </section>`;
    const providers = this.methods?.google || this.methods?.github || this.sso?.enabled
      ? html`<div class="auth-stack">
          ${this.methods?.google ? this.providerButton("google", "Continue with Google") : nothing}
          ${this.methods?.github ? this.providerButton("github", "Continue with GitHub") : nothing}
          ${this.sso?.enabled ? this.providerButton("sso", `Sign in with ${this.sso.name || "SSO"}`) : nothing}
        </div>
        <p class="auth-or"><span>or</span></p>`
      : nothing;
    const others = html`<div class="auth-alt">
      ${mode === "code"
        ? html`<lit-material-button variant="text" @click=${() => (this.mode = "password")}>Use a password instead</lit-material-button>`
        : html`<lit-material-button variant="text" @click=${() => (this.mode = "code")}>Use a code instead</lit-material-button>`}
      <lit-material-button variant="text" ?disabled=${busy} @click=${() => this.run("passkey", async () => {
        const s = server();
        const reply = await passkeySignIn(new Api(s), this.field("email"));
        this.signedIn(s, reply);
      })}>Sign in with a passkey</lit-material-button>
    </div>`;
    if (mode === "signup") {
      return card("Confirm your address", html`We mailed a confirmation code to <strong>${this.account.email}</strong>. Paste it, or the whole link, to finish making your account.`, html`
        <div class="auth-stack">
          <lit-material-text-field id="signup-code" variant="outlined" label="Code from the confirmation mail"></lit-material-text-field>
          <lit-material-button variant="filled" ?disabled=${busy} @click=${() => this.run("signin", async () => {
            const s = this.account.server || DEFAULT_SERVER;
            const reply = await new Api(s).call("POST", "/v1/auth/password/verify", { token: this.field("signup-code").split("#").pop() });
            this.mode = "password";
            this.signedIn(s, reply);
            this.note = "Account ready. Sign in with the same password in hitop: your first device seals its keys with it, so the password unlocks them on your next devices.";
          })}>Confirm</lit-material-button>
          <lit-material-button variant="text" @click=${() => (this.mode = "password")}>Back</lit-material-button>
        </div>`);
    }
    if (mode === "forgot") {
      return card("Reset your password", nothing, html`
        <p class="portal-warning" aria-label="Reset warning">A reset signs you in again, but it can't unlock your end-to-end encrypted workspaces on a new device: your old password sealed their keys, and nobody, the server included, can open them without it. Keep your recovery phrase or a device that is still signed in.</p>
        <div class="auth-stack">
          ${email}
          <lit-material-button variant=${this.resetSent ? "outlined" : "filled"} ?disabled=${busy} @click=${() => this.run("forgot", async () => {
            const s = server();
            await new Api(s).call("POST", "/v1/auth/password/forgot", { email: this.field("email") });
            remember(s);
            this.resetSent = true;
            this.note = "If the address has an account, a reset code is on its way.";
          })}>Send reset code</lit-material-button>
          ${this.resetSent
            ? html`
                <lit-material-text-field id="reset-code" variant="outlined" label="Code from the reset mail"></lit-material-text-field>
                <lit-material-text-field id="reset-password" variant="outlined" label="New password" type="password" autocomplete="new-password"></lit-material-text-field>
                <lit-material-text-field id="reset-password2" variant="outlined" label="Repeat the new password" type="password" autocomplete="new-password"></lit-material-text-field>
                <lit-material-button variant="filled" ?disabled=${busy} @click=${() => this.run("signin", async () => {
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
                })}>Reset password</lit-material-button>`
            : nothing}
          <lit-material-button variant="text" @click=${() => (this.mode = "password")}>Back to sign in</lit-material-button>
        </div>`);
    }
    const lead = "Organizations, seats, billing and access tokens for hitop sync. Your collections stay end-to-end encrypted: this page never sees them.";
    if (mode === "code") {
      return card("Sign in to HITOP Cloud", lead, html`
        ${providers}
        <div class="auth-stack">
          ${email}
          <lit-material-button variant=${this.awaitingCode ? "outlined" : "filled"} ?disabled=${busy} @click=${() => this.run("code", async () => {
            const s = server();
            await new Api(s).call("POST", "/v1/auth/email", { email: this.field("email") });
            remember(s);
            this.awaitingCode = true;
            this.note = "Check your mail for the sign-in code.";
          })}>Send code</lit-material-button>
          ${this.awaitingCode
            ? html`
                <lit-material-text-field id="code" variant="outlined" label="Code from the mail"></lit-material-text-field>
                <lit-material-button variant="filled" ?disabled=${busy} @click=${() => this.run("signin", async () => {
                  // A pasted link works too: the code is after the '#'.
                  const code = this.field("code").split("#").pop();
                  const s = this.account.server || DEFAULT_SERVER;
                  this.signedIn(s, await new Api(s).call("POST", "/v1/auth/email/verify", { token: code }));
                })}>Sign in</lit-material-button>`
            : nothing}
        </div>
        ${others}`);
    }
    return card("Sign in to HITOP Cloud", lead, html`
      ${providers}
      <div class="auth-stack">
        ${email}
        <lit-material-text-field id="password" variant="outlined" label="Password" type="password" autocomplete="current-password"></lit-material-text-field>
        <div class="auth-forgot">
          <lit-material-button variant="text" @click=${() => {
            this.mode = "forgot";
            this.resetSent = false;
          }}>Forgot password?</lit-material-button>
        </div>
        <lit-material-button variant="filled" ?disabled=${busy} @click=${() => this.run("signin", async () => {
          const s = server();
          this.signedIn(s, await passwordSignIn(new Api(s), this.field("email"), this.raw("password")));
        })}>Sign in</lit-material-button>
        <lit-material-button variant="outlined" ?disabled=${busy} @click=${() => this.run("signup", async () => {
          const s = server();
          await passwordSignUp(new Api(s), this.field("email"), this.raw("password"));
          remember(s);
          this.mode = "signup";
          this.note = "Check your mail for the confirmation code.";
        })}>Create account</lit-material-button>
      </div>
      ${others}`);
  }

  // The error and the status line: inside the sign-in card, above the page once signed in.
  messages() {
    return html`
      ${this.error
        ? html`<p class="portal-error" role="alert">${this.error}${this.limitHit?.org
            ? html` <a class="portal-upgrade" href="#/orgs/${this.limitHit.org}/plan">See plans</a>`
            : nothing}</p>`
        : nothing}
      ${this.note ? html`<p class="portal-note" role="status">${this.note}</p>` : nothing}`;
  }

  providerButton(kind, label) {
    return html`<lit-material-button variant="outlined" class="auth-provider" ?disabled=${Boolean(this.busy)} @click=${() => this.run(kind, async () => {
      const s = normalizeServer(this.account.server || DEFAULT_SERVER);
      // Organizations bring their own SSO (C5): the server finds it from the work address.
      if (kind === "sso" && (this.sso?.orgSso || this.methods?.sso?.orgSso) && !this.field("email")) {
        throw new Error("Enter your work address first: it tells the server which organization's single sign-on to use.");
      }
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
    if (parts[0] === "orgs" && this.data.org) {
      const sub = { plan: () => this.renderPlan(), admin: () => this.renderAdmin(), sso: () => this.renderSso(), audit: () => this.renderAudit(), scim: () => this.renderScim() }[parts[2]];
      page = sub ? sub() : this.renderOrg();
    }
    else if (parts[0] === "tokens" && this.data.tokens) page = this.renderTokens();
    else if (parts[0] === "runs" && this.data.workspaces) page = this.renderWorkspaces();
    else if (parts[0] === "workspaces" && this.data.runs) page = this.renderRuns();
    else if (parts[0] === "account" && this.data.me) page = this.renderAccount();
    else if (this.data.orgs) page = this.renderOrgs();
    else page = html`<p>Loading…</p>`;
    return html`
      <p class="portal-whoami">Signed in as <strong>${this.account.email}</strong>${this.account.server && this.account.server !== DEFAULT_SERVER ? html` on ${this.account.server}` : nothing}
        <lit-material-button variant="text" @click=${() => this.run("signout", async () => {
          await this.api.call("POST", "/v1/auth/logout").catch(() => {});
          this.account = { server: this.account.server, email: this.account.email };
          save(this.account);
          this.data = {};
        })}>Sign out</lit-material-button></p>
      ${page}`;
  }

  // --- run history (HITOP Cloud C4) --------------------------------------------------------

  renderWorkspaces() {
    const list = this.data.workspaces;
    return html`
      <section aria-label="Run history">
        <h1>Run history</h1>
        <p class="page-lead">Runs your own CI and cron upload (<code>hitop run --sync-report</code>, <code>hitop schedule --sync</code>): the server keeps a status row per run and the full report sealed end to end. This portal holds no keys, so it shows the status rows; open a report in hitop (<code>hitop sync runs --id &lt;run&gt;</code>, or the app's Sync page). HITOP never runs your collections on its machines.</p>
        ${list.length
          ? html`<table class="portal-table" aria-label="Workspaces">
              <thead><tr><th>Workspace</th><th>Your role</th><th>Organization</th></tr></thead>
              <tbody>${list.map((w) => html`<tr><td><a href="#/workspaces/${w.id}/runs"><code>${w.id.slice(0, 8)}</code></a></td><td>${w.role}</td><td>${w.org ? (w.org.personal ? "Personal" : w.org.name) : ""}</td></tr>`)}</tbody>
            </table>`
          : html`<p class="hint">No workspaces yet: share a collection from hitop first.</p>`}
      </section>`;
  }

  renderRuns() {
    const { workspace: ws, runs, info, alerts, deliveries, run } = this.data;
    const when = (t) => new Date(t * 1000).toISOString().replace("T", " ").replace(/:\d\d\.\d+Z$/, " UTC");
    const one = run ? runs.runs.find((r) => r.id === run) : null;
    return html`
      <section aria-label="Workspace run history">
        <h1>Run history <code>${ws.slice(0, 8)}</code></h1>
        <p class="page-lead">Kept ${runs.historyDays ? `${runs.historyDays} days` : "without a time limit"}${runs.sealedHistory ? "" : html`; this plan keeps <strong>status rows only</strong> (sealed reports from Pro on, <a href="#/">see the plan</a>)`}. You are ${info.role}.</p>
        ${runs.series.length
          ? html`<p aria-label="Last status">${runs.series.map((x) => html`<span class="portal-chip status-${x.status}">${x.collection || "collection"}${x.environment ? ` (${x.environment})` : ""}: ${x.status}</span> `)}</p>`
          : nothing}
        ${one
          ? html`<div class="portal-run" aria-label="Run ${one.id}">
              <h2>Run <code>${one.id}</code></h2>
              <p>${one.status} at ${when(one.createdAt)}, ${one.requests - one.failed}/${one.requests} requests passed, ${one.durationMs} ms${one.runner ? ", from a runner credential" : ""}.</p>
              ${one.failedNames.length ? html`<p>Failed: ${one.failedNames.join(", ")}</p>` : nothing}
              <p class="hint">${one.hasReport ? html`The full report is sealed: open it in hitop with <code>hitop sync runs --id ${one.id}</code> or on the app's Sync page (Run history).` : "This run has a status row only."}</p>
            </div>`
          : nothing}
        <table class="portal-table" aria-label="Runs">
          <thead><tr><th>Time</th><th>Status</th><th>Collection</th><th>Passed</th><th>Duration</th><th>Report</th></tr></thead>
          <tbody>
            ${runs.runs.map((r) => html`<tr class=${r.status === "failed" ? "portal-full" : ""}>
              <td><a href="#/workspaces/${ws}/runs/${r.id}">${when(r.createdAt)}</a></td><td>${r.status}</td>
              <td>${r.collection}${r.environment ? ` (${r.environment})` : ""}</td>
              <td>${r.status === "sealed" ? "-" : `${r.requests - r.failed}/${r.requests}`}</td><td>${r.status === "sealed" ? "-" : `${r.durationMs} ms`}</td>
              <td>${r.hasReport ? "sealed" : "status only"}</td>
            </tr>`)}
          </tbody>
        </table>
        ${runs.runs.length ? nothing : html`<p class="hint">No runs yet.</p>`}
        ${alerts ? this.renderAlerts(ws, alerts, deliveries) : html`<p class="hint">Alerts are set by the workspace's owners.</p>`}
      </section>`;
  }

  renderAlerts(ws, a, deliveries) {
    const s = a.settings;
    const channels = a.plan?.alertChannels ?? ["email"];
    const urls = (kind) => s.webhooks.filter((w) => w.kind === kind).map((w) => w.url).join("\n");
    return html`
      <h2>Alerts</h2>
      <p class="hint">When a collection and environment change status (failed, passes again) or miss a run: the status and a link, never a response, body or secret value. This plan: ${channels.join(", ")}.</p>
      <div class="portal-form portal-stack">
        <lit-material-text-field id="alert-email" variant="outlined" label="Mail to members (comma separated)" .value=${s.email.join(", ")}></lit-material-text-field>
        ${channels.includes("webhook") ? html`<lit-material-text-field id="alert-webhook" variant="outlined" label="Webhook URLs (https, one per line)" .value=${urls("webhook")}></lit-material-text-field>` : nothing}
        ${channels.includes("slack") ? html`<lit-material-text-field id="alert-slack" variant="outlined" label="Slack incoming webhook URLs" .value=${urls("slack")}></lit-material-text-field>` : nothing}
        <lit-material-text-field id="alert-missed" variant="outlined" type="number" label="Missed-run alert after minutes (0 = off)" .value=${String(s.missedAfterMinutes ?? 0)}></lit-material-text-field>
        <label><input type="checkbox" id="alert-names" .checked=${Boolean(s.failedNames)}> Name failed requests in status rows and alerts</label>
        <label><input type="checkbox" id="alert-rows" .checked=${s.statusRows !== false}> Keep status rows (off: history only, no alerts)</label>
        <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("alerts", async () => {
          const lines = (id) => this.raw(id).split(/[\n,]/).map((x) => x.trim()).filter(Boolean);
          await this.api.call("PUT", `/v1/workspaces/${ws}/alerts`, {
            email: lines("alert-email"),
            webhooks: [...lines("alert-webhook").map((url) => ({ kind: "webhook", url })), ...lines("alert-slack").map((url) => ({ kind: "slack", url }))],
            missedAfterMinutes: Number(this.raw("alert-missed") || 0),
            failedNames: this.querySelector("#alert-names")?.checked ?? false,
            statusRows: this.querySelector("#alert-rows")?.checked ?? true,
          });
          await this.go(this.route);
          this.note = "Alert settings saved.";
        })}>Save alerts</lit-material-button>
      </div>
      ${deliveries.length
        ? html`<table class="portal-table" aria-label="Alerts sent">
            <thead><tr><th>Time</th><th>Event</th><th>Channel</th><th>Outcome</th></tr></thead>
            <tbody>${deliveries.map((d) => html`<tr><td>${new Date(d.createdAt * 1000).toISOString().slice(0, 16).replace("T", " ")}</td><td>${d.event}</td><td>${d.channel.split(":")[0]}</td><td>${d.outcome}</td></tr>`)}</tbody>
          </table>`
        : nothing}`;
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
      ${u.usage.runsToday !== undefined
        ? html`<p class="hint" data-metric="runsPerDay">Run uploads today: ${u.usage.runsToday}${u.runs?.runsPerDay ? ` of ${u.runs.runsPerDay}` : ""}; run history kept ${u.runs?.historyDays ? `${u.runs.historyDays} days` : "without a limit"}${u.runs && !u.runs.sealedHistory ? " (status rows only on this plan)" : ""}.</p>`
        : nothing}
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
        <p class="page-lead">Plan <strong>${org.plan}</strong>${org.seats ? html`, ${org.seats} seat(s)` : nothing}. You are ${org.role}. <a href="#/orgs/${org.id}/plan">${this.data.billing?.enabled ? "Plan and billing" : "Plan details"}</a></p>
        ${this.pastDueBanner()}
        ${admin && !org.personal
          ? html`<p aria-label="Organization pages"><a href="#/orgs/${org.id}/admin">Admin console</a> · <a href="#/orgs/${org.id}/sso">Single sign-on</a> · <a href="#/orgs/${org.id}/audit">Audit</a> · <a href="#/orgs/${org.id}/scim">SCIM</a></p>`
          : nothing}
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

  pastDueBanner() {
    const b = this.data.billing;
    if (!b?.pastDue) return nothing;
    return html`<p class="portal-warning" role="alert" aria-label="Payment failed">The last payment failed. Stripe retries it and mails the buyer; update the payment method in the billing portal. Until the retries run out the plan stays as it is; then Free limits apply (nothing is deleted).</p>`;
  }

  renderPlan() {
    const { org, usage, billing } = this.data;
    const admin = org.role === "owner" || org.role === "admin";
    const base = `/v1/orgs/${encodeURIComponent(org.id)}/billing`;
    const date = (secs) => (secs ? new Date(secs * 1000).toISOString().slice(0, 10) : "");
    const status = billing ? billing.status : usage.status;
    return html`
      <section aria-label="Plan">
        <h1>Plan: ${org.plan}</h1>
        <p class="page-lead">${PLAN_NOTES[usage.planInForce] || ""}${usage.grandfathered ? " Your limits were raised to what you used before plans existed." : ""}</p>
        ${this.pastDueBanner()}
        ${usage.planInForce !== usage.plan ? html`<p class="portal-error" role="alert">The ${usage.plan} plan is ${STATUS_NOTES[status] || `${status}: Free limits`}. Nothing was deleted.</p>` : nothing}
        ${billing && org.plan !== "free"
          ? html`<p aria-label="Subscription">Status: <strong>${STATUS_NOTES[status] || status}</strong>${billing.seats && ["team", "business"].includes(org.plan) ? html`, ${billing.seats} seat(s) bought, ${billing.seatsUsed} in use` : nothing}${billing.periodEnd ? html`; ${status === "trialing" ? "the trial ends" : status === "cancelled" ? "ends" : "renews"} ${date(billing.periodEnd)}` : nothing}.</p>`
          : nothing}
        ${this.renderUsage(usage)}
        ${billing?.enabled ? this.renderBilling(org, billing, admin, base) : html`<p>This server sells no plans. Over a limit, hitop refuses new workspaces, people, devices or uploads that grow storage. Reading and pulling never stop, and nothing is deleted.</p>`}
        <p><a href="#/orgs/${org.id}">Back to ${org.personal ? "Personal" : org.name}</a></p>
      </section>`;
  }

  renderBilling(org, billing, admin, base) {
    const reload = () => this.go(this.route);
    const seatsField = (id, value) => html`<label class="portal-seats">Seats <input id=${id} type="number" min=${Math.max(1, billing.seatsUsed)} max="1000" .value=${String(value)} /></label>`;
    const parts = [];
    if (billing.testMode) parts.push(html`<p class="hint">Test mode: purchases here are Stripe test purchases (no real money).</p>`);
    if (billing.subscribed) {
      parts.push(html`<h2>Billing</h2>
        <p class="hint">Payment method, invoices, monthly or yearly, cancel: in Stripe's billing portal. Stripe is the merchant of record (receipts come from Link, "Sold through Link"), so you can also manage the order at link.com.</p>`);
      if (admin) {
        parts.push(html`<div class="portal-form">
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("billing-portal", async () => {
            const { url } = await this.api.get(`${base}/portal`);
            location.assign(url);
          })}>Manage billing</lit-material-button>
        </div>`);
        if (["team", "business"].includes(org.plan) && ["active", "on_trial", "past_due"].includes(billing.status)) {
          parts.push(html`<h2>Seats</h2>
            <p class="hint">Every person in the organization or its workspaces is a seat (viewers too); 5 guest viewers are free. More seats are charged at once, prorated. Fewer seats take effect now, with no credit: the lower price starts at the next renewal.</p>
            <div class="portal-form">${seatsField("seats-change", billing.seats)}
              <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("seats", async () => {
                const seats = Number(this.querySelector("#seats-change").value);
                const r = await this.api.call("POST", `${base}/seats`, { seats });
                await reload();
                this.note = `${r.seats} seat(s).`;
              })}>Change seats</lit-material-button>
            </div>`);
        }
      }
      return parts;
    }
    if (!admin) {
      parts.push(html`<p class="hint">An owner or admin of the organization buys its plan.</p>`);
      return parts;
    }
    const plans = org.personal ? ["pro"] : ["team", "business"];
    const TEXT = {
      pro: "Pro: one person and two guests, 25 workspaces, 2 GB, 10 devices, run history and webhook alerts. For a team, create an organization and buy Team there.",
      team: "Team: per seat, unlimited workspaces, 10 GB + 1 GB per seat, 5 free guest viewers, run history and Slack alerts, the basic admin console.",
      business: "Business: per seat, 50 GB + 2 GB per seat, single sign-on with your own identity provider and domains, signed audit export and daily anchors, the full admin console, a year of run history.",
    };
    const NAME = { pro: "Pro", team: "Team", business: "Business" };
    for (const plan of plans) {
      const offers = billing.offers.filter((o) => o.plan === plan);
      // Business shows once its prices are on sale (C5); Team and Pro always say where they stand.
      if (plan === "business" && !offers.length) continue;
      parts.push(html`<h2>HITOP Cloud ${NAME[plan]}</h2>
        <p class="hint">${TEXT[plan]}
          Prices: ${["annual", "monthly"].filter((p) => offers.some((o) => o.period === p)).map((p) => PRICES[plan][p]).join(" or ")}, plus applicable tax. <a href="https://hitop.rws8.tech/pricing.html">Pricing</a></p>`);
      if (!offers.length) {
        parts.push(html`<p class="hint">${NAME[plan]} isn't on sale on this server yet.</p>`);
      } else if (!billing.checkout) {
        parts.push(html`<p class="hint">Checkout isn't open on this server yet.</p>`);
      } else {
        const seatsId = `seats-buy-${plan}`;
        parts.push(html`<div class="portal-form" aria-label="Buy ${NAME[plan]}">
          ${plan !== "pro" ? seatsField(seatsId, Math.max(1, billing.seatsUsed)) : nothing}
          ${offers.map((o) => html`<lit-material-button variant=${o.period === "annual" ? "filled" : "outlined"} ?disabled=${Boolean(this.busy)} @click=${() => this.run("checkout", async () => {
            const body = { plan: o.plan, period: o.period };
            if (plan !== "pro") body.seats = Number(this.querySelector(`#${seatsId}`).value);
            const { url } = await this.api.call("POST", `${base}/checkout`, body);
            location.assign(url);
          })}>Buy ${NAME[o.plan]}, ${o.period === "annual" ? "yearly" : "monthly"}</lit-material-button>`)}
        </div>`);
      }
    }
    if (billing.offers.length && billing.checkout) {
      parts.push(html`<p class="hint">The checkout is Stripe's page, "Sold through Link": Stripe is the merchant of record, adds tax where it applies and takes the payment (HITOP never sees the card). Refunds: see the <a href="https://hitop.rws8.tech/pricing.html#refunds">pricing page</a>.</p>`);
    }
    if (billing.trial?.available && org.role === "owner") {
      parts.push(html`<div class="portal-form">
        <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("trial", async () => {
          await this.api.call("POST", `${base}/trial`, {});
          await reload();
          this.note = `Team trial started: ${billing.trial.days} days, ${billing.trial.seats} seats, no card. Afterwards Free limits apply unless you buy Team; nothing is deleted.`;
        })}>Start the ${billing.trial.days}-day Team trial</lit-material-button>
      </div>`);
    }
    return parts;
  }

  // --- HITOP Cloud C5: admin console, single sign-on, audit -------------------------------

  orgBack(org) {
    return html`<p><a href="#/orgs/${org.id}">Back to ${org.name}</a></p>`;
  }

  renderAdmin() {
    const { org, admin: a } = this.data;
    if (!a) return html`<section aria-label="Admin console"><h1>Admin console</h1>${this.orgBack(org)}</section>`;
    const base = `/v1/orgs/${encodeURIComponent(org.id)}/admin`;
    const reload = () => this.go(this.route);
    return html`
      <section aria-label="Admin console">
        <h1>Admin console: ${org.name}</h1>
        <p class="page-lead">Every workspace of the organization, its people and their devices. An organization role never opens a workspace: this page shows who has access, never what is inside, and the server holds no key to show it.</p>
        <h2>Workspaces</h2>
        ${a.workspaces.length ? nothing : html`<p class="hint">No workspaces in this organization yet (<code>hitop sync init --org ${org.name}</code>).</p>`}
        ${a.workspaces.map((w) => {
          const people = w.members.filter((m) => !m.runner);
          return html`<div class="portal-card" aria-label="Workspace ${w.id.slice(0, 8)}">
            <h3><code>${w.id.slice(0, 8)}</code> ${w.rotationRequested ? html`<span class="portal-chip status-failed" aria-label="Rotation requested">key rotation requested by ${w.rotationRequested.by}</span>` : nothing}</h3>
            <p class="hint">Key epoch ${w.epoch}, ${bytes(w.bytes)}, created ${date(w.createdAt)}.</p>
            <table class="portal-table" aria-label="People of ${w.id.slice(0, 8)}">
              <thead><tr><th>Email</th><th>Role</th></tr></thead>
              <tbody>${w.members.map((m) => html`<tr><td>${m.email}</td><td>${m.runner ? "runner" : m.role}</td></tr>`)}</tbody>
            </table>
            <div class="portal-form">
              ${a.actions.transferOwner
                ? html`<select id="owner-${w.id}" aria-label="New owner of ${w.id.slice(0, 8)}">${people.map((m) => html`<option value=${m.userId}>${m.email}</option>`)}</select>
                  <label><input type="checkbox" id="keep-${w.id}" /> Keep the other owners</label>
                  <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("owner", async () => {
                    const userId = this.querySelector(`#owner-${w.id}`).value;
                    await this.api.call("PUT", `${base}/workspaces/${w.id}/owner`, { userId, keepOwners: this.querySelector(`#keep-${w.id}`).checked });
                    await reload();
                    this.note = `${people.find((m) => m.userId === userId)?.email} owns ${w.id.slice(0, 8)} now.`;
                  })}>Transfer ownership</lit-material-button>`
                : nothing}
              ${a.actions.forceRotation
                ? html`<lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("rotation", async () => {
                    await this.api.call("POST", `${base}/workspaces/${w.id}/rotation`);
                    await reload();
                    this.note = "Rotation requested: the workspace's owners are asked to rotate its key (their hitop does it; the server can't).";
                  })}>Ask for a key rotation</lit-material-button>`
                : nothing}
            </div>
          </div>`;
        })}
        <h2>Devices</h2>
        <p class="hint">Device keys of the organization's members. Removing one (a lost or stolen laptop) takes it out of this organization's workspaces only: its copies of their keys are deleted and each of those workspaces asks its owners for a rotation. The device stays on the person's account, with their personal and other organizations' workspaces (they remove it there themselves). Their other devices keep working.</p>
        <table class="portal-table" aria-label="Devices">
          <thead><tr><th>Email</th><th>Device</th><th>Added</th><th>Workspaces here</th><th></th></tr></thead>
          <tbody>${a.devices.map((d) => html`<tr>
            <td>${d.email}</td><td>${d.name}</td><td>${date(d.createdAt)}</td><td>${d.orgWorkspaces}</td>
            <td>${a.actions.removeDevice
              ? html`<lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.run("device", async () => {
                  if (!confirm(`Remove the device "${d.name}" of ${d.email}? It is removed from this organization's workspaces: its copies of their keys are deleted and those workspaces ask for a key rotation. Their personal and other organizations' workspaces aren't touched. Their other devices keep working.`)) return;
                  await this.api.call("DELETE", `${base}/devices/${encodeURIComponent(d.keyId)}`);
                  await reload();
                  this.note = `Removed ${d.name} of ${d.email} from this organization's workspaces.`;
                })}>Remove</lit-material-button>`
              : nothing}</td>
          </tr>`)}</tbody>
        </table>
        ${a.actions.forceRotation ? nothing : html`<p class="hint">Key rotation requests and removing devices come with Business. <a class="portal-upgrade" href="#/orgs/${org.id}/plan">See plans</a></p>`}
        ${this.orgBack(org)}
      </section>`;
  }

  renderSso() {
    const { org, sso } = this.data;
    const owner = org.role === "owner";
    const base = `/v1/orgs/${encodeURIComponent(org.id)}`;
    const reload = () => this.go(this.route);
    if (!sso) return html`<section aria-label="Single sign-on"><h1>Single sign-on</h1>${this.orgBack(org)}</section>`;
    if (!sso.available) {
      return html`<section aria-label="Single sign-on"><h1>Single sign-on</h1>
        <p class="page-lead" aria-label="Not available">${sso.reason}. A self-hosted server turns it on with a host key (<code>HITOP_SYNC_HOST_KEY_FILE</code>, see the sync server's README).</p>${this.orgBack(org)}</section>`;
    }
    const c = sso.config || {};
    const verified = sso.domains.filter((d) => d.verified).length;
    return html`
      <section aria-label="Single sign-on">
        <h1>Single sign-on: ${org.name}</h1>
        <p class="page-lead">Your people sign in with your identity provider (OpenID Connect: Entra ID, Google Workspace, Okta, Keycloak...). It signs in only addresses of domains you verified here. Like every sign-in it gives access, never keys: workspaces stay end-to-end encrypted.</p>
        ${sso.allowed ? nothing : html`<p class="portal-warning" role="alert">Single sign-on comes with Business. <a class="portal-upgrade" href="#/orgs/${org.id}/plan">See plans</a></p>`}
        <h2>Domains</h2>
        <p class="hint">Prove you own a domain with a DNS TXT record${sso.dnsVerification ? "" : " (this server can't look them up yet: HITOP_SYNC_DNS_URL isn't set)"}.</p>
        <table class="portal-table" aria-label="Domains">
          <thead><tr><th>Domain</th><th>TXT record</th><th>Value</th><th>Status</th><th></th></tr></thead>
          <tbody>${sso.domains.map((d) => html`<tr>
            <td>${d.domain}</td><td><code>${d.record}</code></td><td><code>${d.value}</code></td>
            <td>${d.verified ? `verified ${date(d.verifiedAt)}` : "not verified"}</td>
            <td>
              ${d.verified ? nothing : html`<lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.run("verify", async () => {
                const r = await this.api.call("POST", `${base}/domains/${encodeURIComponent(d.domain)}/verify`);
                await reload();
                if (r.problem) throw new Error(r.problem);
                this.note = `${d.domain} is verified.`;
              })}>Verify</lit-material-button>`}
              <lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.run("undomain", async () => {
                await this.api.call("DELETE", `${base}/domains/${encodeURIComponent(d.domain)}`);
                await reload();
              })}>Remove</lit-material-button>
            </td>
          </tr>`)}</tbody>
        </table>
        <div class="portal-form">
          <lit-material-text-field id="sso-domain" variant="outlined" label="Domain to claim"></lit-material-text-field>
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("domain", async () => {
            const d = await this.api.call("POST", `${base}/domains`, { domain: this.field("sso-domain") });
            await reload();
            this.note = `Publish the TXT record ${d.record} with the value ${d.value}, then Verify.`;
          })}>Claim domain</lit-material-button>
        </div>
        <h2>Identity provider</h2>
        <p class="hint">Register a web application (confidential client, authorization code with PKCE) with the redirect URI <code aria-label="Redirect URI">${sso.redirectUri}</code>. ${verified ? "" : "Verify a domain first: nobody can sign in through it until then."}${owner ? "" : " Only owners change these settings."}</p>
        <div class="portal-form portal-stack" aria-label="SSO settings">
          <lit-material-text-field id="sso-issuer" variant="outlined" label="Issuer URL" .value=${c.issuer || ""} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field id="sso-client" variant="outlined" label="Client ID" .value=${c.clientId || ""} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field id="sso-secret" variant="outlined" type="password" label="Client secret" placeholder=${c.hasSecret ? "stored (leave empty to keep it)" : ""} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field id="sso-name" variant="outlined" label="Button name" .value=${c.name || "SSO"} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field id="sso-scopes" variant="outlined" label="Scopes" .value=${c.scopes || "openid email profile"} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field id="sso-groups-claim" variant="outlined" label="Groups claim" .value=${c.groupsClaim || "groups"} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field id="sso-groups" variant="outlined" label="Required groups (comma separated, empty = any)" .value=${(c.requiredGroups || []).join(", ")} ?disabled=${!owner}></lit-material-text-field>
          <label><input type="checkbox" id="sso-required" .checked=${Boolean(c.required)} ?disabled=${!owner} /> SSO required: your domains' addresses sign in only through it (no mail codes, passwords, passkeys, Google or GitHub)</label>
          <label><input type="checkbox" id="sso-assume" .checked=${Boolean(c.assumeVerified)} ?disabled=${!owner} /> The provider doesn't send email_verified (Entra ID): trust its addresses in your verified domains</label>
          <label><input type="checkbox" id="sso-enabled" .checked=${c.enabled !== false} ?disabled=${!owner} /> On</label>
          ${owner
            ? html`<lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("sso-save", async () => {
                const body = {
                  issuer: this.field("sso-issuer"),
                  clientId: this.field("sso-client"),
                  name: this.field("sso-name"),
                  scopes: this.field("sso-scopes"),
                  groupsClaim: this.field("sso-groups-claim"),
                  requiredGroups: this.field("sso-groups").split(",").map((g) => g.trim()).filter(Boolean),
                  required: this.querySelector("#sso-required").checked,
                  assumeVerified: this.querySelector("#sso-assume").checked,
                  enabled: this.querySelector("#sso-enabled").checked,
                };
                if (this.raw("sso-secret")) body.clientSecret = this.raw("sso-secret");
                await this.api.call("PUT", `${base}/sso`, body);
                await reload();
                this.note = "Single sign-on saved.";
              })}>Save</lit-material-button>`
            : nothing}
          ${sso.configured
            ? html`<lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("sso-test", async () => {
                const r = await this.api.call("POST", `${base}/sso/test`);
                if (!r.ok) throw new Error(`The provider check failed: ${r.error}`);
                this.note = "The provider answers: discovery and keys fetched.";
              })}>Test</lit-material-button>`
            : nothing}
          ${sso.configured && owner
            ? html`<lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.run("sso-delete", async () => {
                if (!confirm("Remove single sign-on? People sign in as before (mail code, password, passkey); accounts stay.")) return;
                await this.api.call("DELETE", `${base}/sso`);
                await reload();
                this.note = "Single sign-on removed.";
              })}>Remove single sign-on</lit-material-button>`
            : nothing}
        </div>
        <p class="hint">People sign in with <code>hitop sync login --sso --email you@your-domain</code>, or "Sign in with SSO" and their work address in the apps and here.</p>
        ${this.orgBack(org)}
      </section>`;
  }

  // --- HITOP Cloud C6: SCIM provisioning --------------------------------------------------

  renderScim() {
    const { org, scim } = this.data;
    const owner = org.role === "owner";
    const base = `/v1/orgs/${encodeURIComponent(org.id)}/scim`;
    const reload = () => this.go(this.route);
    if (!scim) return html`<section aria-label="SCIM"><h1>SCIM provisioning</h1>${this.orgBack(org)}</section>`;
    const workspaces = this.data.adminWorkspaces || [];
    const saveGroup = (g, change) => this.run("scim-group", async () => {
      const body = { orgRole: g.orgRole, workspaces: g.workspaces.map((w) => ({ workspace: w.workspace, role: w.role })), ...change };
      await this.api.call("PUT", `${base}/groups/${encodeURIComponent(g.id)}`, body);
      await reload();
      this.note = `Saved what ${g.displayName} gives; applied to its members.`;
    });
    return html`
      <section aria-label="SCIM">
        <h1>SCIM provisioning: ${org.name}</h1>
        <p class="page-lead">Your identity provider (Okta, Entra ID, Keycloak...) adds and removes the organization's members and their groups. A group gives an organization role and workspace roles, set here. SCIM gives access, never keys: a workspace's owner still adds a new person's device (<code>hitop sync members</code>, then <code>hitop sync add</code>), and someone removed is out at once while the owners rotate the key.</p>
        <h2>Connection</h2>
        <p class="hint">SCIM base URL <code aria-label="SCIM base URL">${scim.baseUrl}</code>, bearer token below. userName must be the person's email address. ${scim.enabled ? `${scim.activeUsers} active of ${scim.users} provisioned users${scim.pendingWorkspaceAdds ? `, ${scim.pendingWorkspaceAdds} waiting for a workspace owner to add them` : ""}.` : "Off: no token yet."}</p>
        ${this.newToken
          ? html`<div class="portal-card" role="status"><p>Copy the token into your identity provider now; it isn't shown again.</p><pre aria-label="New SCIM token">${this.newToken}</pre></div>`
          : nothing}
        <table class="portal-table" aria-label="SCIM tokens">
          <thead><tr><th>Name</th><th>Created</th><th>Last used</th><th></th></tr></thead>
          <tbody>${scim.tokens.map((t) => html`<tr>
            <td>${t.name}</td><td>${date(t.createdAt)}</td><td>${t.lastUsedAt ? date(t.lastUsedAt) : "never"}</td>
            <td>${owner ? html`<lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => this.run("scim-revoke", async () => {
              if (!confirm(`Revoke the SCIM token "${t.name}"? Your identity provider stops provisioning until it has a new one.`)) return;
              await this.api.call("DELETE", `${base}/tokens/${encodeURIComponent(t.id)}`);
              await reload();
              this.note = `Revoked ${t.name}.`;
            })}>Revoke</lit-material-button>` : nothing}</td>
          </tr>`)}</tbody>
        </table>
        ${owner
          ? html`<div class="portal-form">
              <lit-material-text-field id="scim-token-name" variant="outlined" label="Token name (your identity provider)"></lit-material-text-field>
              <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("scim-token", async () => {
                const t = await this.api.call("POST", `${base}/tokens`, { name: this.field("scim-token-name") });
                await reload();
                this.newToken = t.token;
              })}>Create SCIM token</lit-material-button>
            </div>`
          : html`<p class="hint">Only owners create or revoke SCIM tokens.</p>`}
        <h2>Groups</h2>
        <p class="hint">Groups appear when your identity provider pushes them. Owners are never set by SCIM; when any group gives an organization role, members without one are members.</p>
        ${scim.groups.length ? nothing : html`<p class="hint">No groups yet.</p>`}
        ${scim.groups.map((g) => html`<div class="portal-card" aria-label="Group ${g.displayName}">
          <h3>${g.displayName} <span class="hint">${g.members} member(s)</span></h3>
          <div class="portal-form">
            <select id="scim-org-role-${g.id}" aria-label="Organization role of ${g.displayName}" @change=${(e) => saveGroup(g, { orgRole: e.target.value })}>
              ${[["", "no organization role"], ["member", "member"], ["admin", "admin"]].map(([v, l]) => html`<option value=${v} ?selected=${v === g.orgRole}>${l}</option>`)}
            </select>
          </div>
          <table class="portal-table" aria-label="Workspaces of ${g.displayName}">
            <thead><tr><th>Workspace</th><th>Role</th><th></th></tr></thead>
            <tbody>${g.workspaces.map((w) => html`<tr><td><code>${w.workspace.slice(0, 8)}</code></td><td>${w.role}</td>
              <td><lit-material-button variant="text" ?disabled=${Boolean(this.busy)} @click=${() => saveGroup(g, { workspaces: g.workspaces.filter((x) => x.workspace !== w.workspace) })}>Remove</lit-material-button></td></tr>`)}</tbody>
          </table>
          <div class="portal-form">
            <select id="scim-ws-${g.id}" aria-label="Workspace for ${g.displayName}">${workspaces.map((w) => html`<option value=${w}>${w.slice(0, 8)}</option>`)}</select>
            <select id="scim-ws-role-${g.id}" aria-label="Workspace role for ${g.displayName}">${["viewer", "editor"].map((r) => html`<option value=${r}>${r}</option>`)}</select>
            <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy) || !workspaces.length} @click=${() => {
              const workspace = this.querySelector(`#scim-ws-${g.id}`).value;
              const role = this.querySelector(`#scim-ws-role-${g.id}`).value;
              saveGroup(g, { workspaces: [...g.workspaces.filter((x) => x.workspace !== workspace), { workspace, role }] });
            }}>Give workspace</lit-material-button>
          </div>
        </div>`)}
        ${this.orgBack(org)}
      </section>`;
  }

  renderAudit() {
    const { org, audit } = this.data;
    const owner = org.role === "owner";
    const base = `/v1/orgs/${encodeURIComponent(org.id)}/audit`;
    if (!audit) return html`<section aria-label="Audit"><h1>Audit</h1>${this.orgBack(org)}</section>`;
    const setting = (key, value) => this.run("audit-settings", async () => {
      await this.api.call("PUT", `${base}/settings`, { [key]: value });
      await this.go(this.route);
      this.note = "Audit settings saved.";
    });
    return html`
      <section aria-label="Audit">
        <h1>Audit: ${org.name}</h1>
        <p class="page-lead">The audit log of the organization and its workspaces (sign-ins aside: who added, removed or invited whom, roles, key rotations, SSO and admin changes). Never contents, keys or tokens.</p>
        <h2>Signed export</h2>
        <p class="hint">JSON lines: every chain's rows and head, signed by the server (Ed25519, key <code>${audit.keyId}</code>). Keep it next to the anchors you were mailed: <code>hitop sync audit --verify --anchor anchor.json</code> recomputes the chains and checks each anchored head is still there, so a rewritten log shows.</p>
        <div class="portal-form">
          <lit-material-button variant="filled" ?disabled=${Boolean(this.busy)} @click=${() => this.run("export", async () => {
            const r = await fetch(`${this.api.server}${base}/export`, { headers: { Authorization: `Bearer ${this.account.session}` } });
            if (!r.ok) throw new ApiError(r.status, await r.json().catch(() => null));
            download(`hitop-audit-${org.id}-${new Date().toISOString().slice(0, 10)}.jsonl`, await r.blob());
          })}>Download signed export</lit-material-button>
          <lit-material-button variant="outlined" ?disabled=${Boolean(this.busy)} @click=${() => this.run("anchor", async () => {
            const a = await this.api.call("POST", `${base}/anchors`);
            download(`hitop-anchor-${org.id}-${a.day}-now.json`, JSON.stringify(a, null, 2));
          })}>Anchor now</lit-material-button>
        </div>
        <h2>Daily anchors</h2>
        <p class="hint">Once a day the server signs the heads of the organization's chains${audit.anchorsOn ? "" : " (turned off on this server)"}; when they moved, it mails the anchor to the owners. Keep those mails: <code>hitop sync audit --verify --anchor anchor.json</code> checks the log against one.</p>
        <div class="portal-form portal-stack" aria-label="Anchor settings">
          <label><input type="checkbox" id="audit-mail" .checked=${audit.mailAnchors} ?disabled=${!owner || Boolean(this.busy)} @change=${(e) => setting("mailAnchors", e.target.checked)} /> Mail anchors to the owners</label>
          <label><input type="checkbox" id="audit-public" .checked=${audit.publicAnchors} ?disabled=${!owner || Boolean(this.busy)} @change=${(e) => setting("publicAnchors", e.target.checked)} /> Publish anchors (anyone can fetch them at <code>${audit.publicUrl}&lt;day&gt;.json</code>; they show how many workspaces the organization has and their ids)</label>
        </div>
        ${audit.anchors.length
          ? html`<table class="portal-table" aria-label="Anchors">
              <thead><tr><th>Day</th><th>Chains</th><th>Mailed</th><th></th></tr></thead>
              <tbody>${audit.anchors.map((x) => html`<tr>
                <td>${x.day}</td><td>${x.anchor?.chains?.length ?? 0}</td><td>${x.mailedAt ? date(x.mailedAt) : "no"}</td>
                <td><lit-material-button variant="text" @click=${() => download(`hitop-anchor-${org.id}-${x.day}.json`, JSON.stringify(x.anchor, null, 2))}>Download</lit-material-button></td>
              </tr>`)}</tbody>
            </table>`
          : html`<p class="hint" aria-label="No anchors">No daily anchor yet: the first one comes within the hour.</p>`}
        ${this.orgBack(org)}
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
