// The Organization area (HITOP Next 7b, owner decision 2026-11-09: inside the app, not the
// portal): General, Members, Plan & billing, Access tokens & MCP, Runs & monitors, Security &
// audit, for the organization picked in the top bar. Same sync-server endpoints the portal uses
// (cloud/portal/src/pages/*.js), through the app's own session (sync.js SyncClient).
//
// Purchases: the web app opens Stripe Checkout and the billing portal. The desktop build never
// shows a purchase link (App Store rule, as before: hitop-app.js PORTAL_URL); it shows the plan
// and usage and says where to change them.
import { LitElement, html, nothing } from "lit";
import { styleMap } from "lit/directives/style-map.js";
import { confirm } from "@lit-material/confirm";
import { icon } from "../icons.js";
import { IS_TAURI, platform } from "../platform/index.js";
import { syncClient } from "./sync-view.js";
import { limitInfo, meters, planCards, PLAN_NAMES, sectionsFor } from "../org.js";

const enc = encodeURIComponent;

export class HitopOrg extends LitElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    app: { attribute: false },
    section: { state: true },
    data: { state: true },
    tokens: { state: true },
    security: { state: true },
    busy: { state: true },
    error: { state: true },
    created: { state: true },
    seats: { state: true },
  };

  constructor() {
    super();
    this.section = "general";
    this.data = null;
    this.loadedFor = null;
  }

  createRenderRoot() {
    return this;
  }

  get client() {
    return syncClient(this.store);
  }

  get orgId() {
    return this.store.currentOrg;
  }

  updated() {
    if (this.orgId && this.loadedFor !== this.orgId) this.load();
  }

  async load() {
    const id = this.orgId;
    this.loadedFor = id;
    this.error = null;
    const base = `/v1/orgs/${enc(id)}`;
    try {
      const [org, members, usage, billing] = await Promise.all([
        this.client.api("GET", base),
        this.client.api("GET", `${base}/members`),
        this.client.api("GET", `${base}/usage`),
        this.client.api("GET", `${base}/billing`).catch(() => null),
      ]);
      if (this.loadedFor === id) this.data = { org, members, usage, billing };
      // The title bar's usage warning (U49) follows what this page just read.
      this.store?.setOrgUsage?.(id, usage);
    } catch (e) {
      this.error = e.message;
    }
    if (this.section === "tokens") this.loadTokens();
    if (this.section === "security") this.loadSecurity();
  }

  async loadTokens() {
    try {
      this.tokens = (await this.client.api("GET", "/v1/me/tokens")).tokens ?? [];
    } catch (e) {
      this.tokens = [];
      this.error = e.message;
    }
  }

  async loadSecurity() {
    const base = `/v1/orgs/${enc(this.orgId)}`;
    const get = (p) => this.client.api("GET", `${base}${p}`).catch((e) => ({ refused: e }));
    const [sso, domains] = await Promise.all([get("/sso"), get("/domains")]);
    this.security = { sso, domains: domains.domains ?? [], refused: domains.refused ?? sso.refused ?? null };
  }

  pick(section) {
    this.section = section;
    this.error = null;
    if (section === "tokens" && !this.tokens) this.loadTokens();
    if (section === "security" && !this.security) this.loadSecurity();
  }

  /** Runs a change; a plan limit (402) opens the limit dialog, anything else shows here. */
  async act(fn, success) {
    this.busy = true;
    this.error = null;
    try {
      const out = await fn();
      if (success) this.app.toast(typeof success === "function" ? success(out) : success);
      await this.load();
      return out;
    } catch (e) {
      const limit = limitInfo(e);
      if (limit) this.app.openLimit(limit, this.data?.org);
      else this.error = e.message;
      return null;
    } finally {
      this.busy = false;
    }
  }

  // --- sections --------------------------------------------------------------------------

  general({ org, usage }) {
    const owner = org.role === "owner";
    return html`<section class="org-card" aria-label="General">
        <h2>General</h2>
        <dl class="org-facts">
          <dt>Name</dt><dd>${org.name}${org.personal ? html` <span class="hint">(your personal organization)</span>` : nothing}</dd>
          <dt>Your role</dt><dd>${org.role}</dd>
          <dt>Plan</dt><dd>${PLAN_NAMES[usage.planInForce] ?? usage.planInForce}${usage.planInForce !== usage.plan ? html` <span class="hint">(${PLAN_NAMES[usage.plan] ?? usage.plan} ${usage.status})</span>` : nothing}</dd>
          <dt>Members</dt><dd class="tabular">${org.members}</dd>
          <dt>Shared collections</dt><dd class="tabular">${org.workspaces}</dd>
        </dl>
        ${owner && !org.personal
          ? html`<div class="org-row">
              <lit-material-text-field id="org-name" variant="outlined" label="Rename" .value=${org.name}></lit-material-text-field>
              <lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => {
                const name = this.querySelector("#org-name")?.value?.trim();
                if (name && name !== org.name) this.act(() => this.client.api("PUT", `/v1/orgs/${enc(org.id)}`, { name }), `Renamed to ${name}`).then(() => this.app.reloadOrgs?.());
              }}>Rename</lit-material-button>
            </div>
            <div class="org-danger">
              <lit-material-button variant="outlined" class="destructive" ?disabled=${this.busy} @click=${() => this.deleteOrg(org)}>${icon("trash")} Delete ${org.name}</lit-material-button>
              <p class="hint">Only an organization without shared collections can be deleted. Nothing in anyone's personal space changes.</p>
            </div>`
          : nothing}
      </section>`;
  }

  async deleteOrg(org) {
    const ok = await confirm({ headline: `Delete ${org.name}?`, message: "Members lose access to it right away. This can't be undone.", confirmLabel: "Delete", destructive: true });
    if (!ok) return;
    const done = await this.act(() => this.client.api("DELETE", `/v1/orgs/${enc(org.id)}`), `${org.name} deleted`);
    if (done !== null) {
      this.store.setCurrentOrg(this.store.orgs.find((o) => o.personal)?.id ?? null);
      this.app.reloadOrgs?.();
    }
  }

  members({ org, members, usage }) {
    const admin = org.role === "owner" || org.role === "admin";
    const seats = usage.limits?.seats;
    const used = usage.usage?.seats ?? members.members.length;
    const roles = ["member", "admin", ...(org.role === "owner" ? ["owner"] : [])];
    return html`<section class="org-card" aria-label="Members">
      <div class="org-head">
        <h2>Members</h2>
        <span class="hint tabular">${members.members.length} ${members.members.length === 1 ? "person" : "people"}${seats != null ? ` · ${used} of ${seats} seats` : ""}${members.invites.length ? ` · ${members.invites.length} invited` : ""}</span>
        ${admin && !org.personal ? html`<lit-material-button variant="filled" @click=${() => this.app.openDialog("org-invite", { seats: this.data?.usage })}>${icon("plus")} Invite</lit-material-button>` : nothing}
      </div>
      <div class="org-table" role="table" aria-label="People">
        <div role="row" class="org-tr org-th"><span role="columnheader">Person</span><span role="columnheader">Role</span><span role="columnheader">Since</span><span role="columnheader"><span class="visually-hidden">Actions</span></span></div>
        ${members.members.map((m) => {
          const me = m.email === this.client.account?.()?.email;
          return html`<div role="row" class="org-tr">
            <span role="cell" class="org-person"><span class="org-avatar" aria-hidden="true">${m.email.slice(0, 2).toUpperCase()}</span>${m.email}${me ? html` <span class="hint">(you)</span>` : nothing}</span>
            <span role="cell">${admin && !me && (m.role !== "owner" || org.role === "owner")
              ? html`<lit-material-select variant="outlined" label=${`Role of ${m.email}`} .value=${m.role} ?disabled=${this.busy} @change=${(e) => this.changeRole(org, m, e.target.value)}>
                  ${[...new Set([...roles, m.role])].map((r) => html`<lit-material-select-option value=${r}>${r === "owner" ? "owner (transfer)" : r}</lit-material-select-option>`)}
                </lit-material-select>`
              : m.role}</span>
            <span role="cell" class="hint tabular">${m.since ? new Date(m.since * 1000).toLocaleDateString() : ""}</span>
            <span role="cell">${(admin && !me && m.role !== "owner") || (me && m.role !== "owner")
              ? html`<lit-material-button variant="text" class="destructive" ?disabled=${this.busy} @click=${() => this.removeMember(org, m, me)}>${me ? "Leave" : "Remove"}</lit-material-button>`
              : nothing}</span>
          </div>`;
        })}
        ${members.invites.map(
          (i) => html`<div role="row" class="org-tr pending">
            <span role="cell" class="org-person"><span class="org-avatar" aria-hidden="true">@</span>${i.email} <span class="hint">invited</span></span>
            <span role="cell">${i.role}</span>
            <span role="cell" class="hint tabular">${i.createdAt ? new Date(i.createdAt * 1000).toLocaleDateString() : ""}</span>
            <span role="cell">${admin
              ? html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.act(() => this.client.api("POST", `/v1/orgs/${enc(org.id)}/members`, { email: i.email, role: i.role }), `Invite to ${i.email} sent again`)}>Resend</lit-material-button>
                  <lit-material-button variant="text" class="destructive" ?disabled=${this.busy} @click=${() => this.act(() => this.client.api("DELETE", `/v1/orgs/${enc(org.id)}/invites/${enc(i.email)}`), `Invite to ${i.email} revoked`)}>Revoke</lit-material-button>`
              : nothing}</span>
          </div>`,
        )}
      </div>
      <p class="hint">Guest viewers (free on Team, read only) and CI runners are added from a collection's Share dialog, on the People tab.${usage.usage?.guestViewers != null && usage.limits ? ` Guests now: ${usage.usage.guestViewers}.` : ""}</p>
    </section>`;
  }

  async changeRole(org, m, role) {
    if (role === m.role) return;
    if (role === "owner") {
      const ok = await confirm({ headline: `Make ${m.email} the owner?`, message: "They can then manage billing and delete the organization. You stay an admin.", confirmLabel: "Make owner" });
      if (!ok) return this.requestUpdate();
    }
    await this.act(() => this.client.api("PUT", `/v1/orgs/${enc(org.id)}/members/${enc(m.userId)}`, { role }), `${m.email} is now ${role === "member" ? "a" : "an"} ${role}`);
  }

  async removeMember(org, m, me) {
    const ok = await confirm({ headline: me ? `Leave ${org.name}?` : `Remove ${m.email}?`, message: me ? "You lose access right away. An admin can invite you again." : "They lose access right away and free a seat. Keys of shared collections are rotated on the next save.", confirmLabel: me ? "Leave" : "Remove", destructive: true });
    if (ok) await this.act(() => this.client.api("DELETE", `/v1/orgs/${enc(org.id)}/members/${enc(m.userId)}`), me ? `You left ${org.name}` : `${m.email} removed`);
  }

  billing({ org, usage, billing }) {
    const admin = org.role === "owner" || org.role === "admin";
    const cards = planCards(org, billing, usage.planInForce);
    const lapsed = cards.find((c) => c.lapsed && c.buy);
    const canBuy = !IS_TAURI && billing?.enabled && admin && !billing.subscribed;
    const perSeat = ["team", "business"].includes(org.plan);
    return html`<section class="org-card" aria-label="Plan and billing">
        <div class="org-head">
          <h2>Plan &amp; billing</h2>
          <span class="status-chip info">${PLAN_NAMES[usage.planInForce] ?? usage.planInForce}${billing?.seats && perSeat ? ` · ${billing.seats} seats` : ""}</span>
        </div>
        ${usage.planInForce !== usage.plan
          ? html`<lit-material-alert variant="warning">${PLAN_NAMES[usage.plan] ?? usage.plan} is ${usage.status}: Free limits apply until it's paid again. Nothing is deleted; over-limit collections become read-only.
              ${lapsed && canBuy ? html`<lit-material-button slot="action" variant="filled" ?disabled=${this.busy} @click=${() => this.openBilling("checkout", org, lapsed)}>${lapsed.action}</lit-material-button>` : nothing}
              ${billing?.subscribed && admin && !IS_TAURI ? html`<lit-material-button slot="action" variant="filled" ?disabled=${this.busy} @click=${() => this.openBilling("portal", org)}>Update payment</lit-material-button>` : nothing}</lit-material-alert>`
          : nothing}
        ${billing?.trial?.endsAt ? html`<p class="hint">The trial ends ${new Date(billing.trial.endsAt * 1000).toLocaleDateString()}. Nothing is deleted if you don't pick a plan.</p>` : nothing}
        <h3>Usage</h3>
        <div class="org-meters">
          ${meters(usage).map(
            (m) => html`<div class="org-meter">
              <div class="org-meter-top"><span>${m.label}</span><span class="tabular">${m.text}</span></div>
              <div role="meter" aria-label=${m.label} aria-valuenow=${m.used} aria-valuemin="0" aria-valuemax=${m.max ?? m.used} class=${`org-meter-bar ${m.level}`}><span style=${styleMap({ width: `${m.percent}%` })}></span></div>
              ${m.note ? html`<span class=${`hint ${m.level}`}>${m.note}</span>` : nothing}
            </div>`,
          )}
        </div>
        ${IS_TAURI
          ? html`<p class="hint">Plans and payment are changed on the web: open HITOP in a browser, then Organization → Plan &amp; billing.</p>`
          : !billing?.enabled
            ? html`<p class="hint">This server doesn't sell plans.</p>`
            : !admin
              ? html`<p class="hint">Only owners and admins change the plan.</p>`
              : billing.subscribed
                ? html`<div class="org-row">
                    ${perSeat
                      ? html`<lit-material-text-field id="org-seats" type="number" min="1" variant="outlined" label="Seats" .value=${String(this.seats ?? billing.seats ?? 1)} @input=${(e) => (this.seats = Number(e.target.value))}></lit-material-text-field>
                          <lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.act(() => this.client.api("POST", `/v1/orgs/${enc(org.id)}/billing/seats`, { seats: Number(this.querySelector("#org-seats")?.value) }), (r) => `Seats: ${r?.seats ?? ""}`)}>Change seats</lit-material-button>`
                      : nothing}
                    <lit-material-button variant="filled" ?disabled=${this.busy} @click=${() => this.openBilling("portal", org)}>${icon("arrow-square-out")} Invoices, payment, cancel</lit-material-button>
                  </div>`
                : html`<h3>Plans</h3>
                    <div class="org-plans">
                      ${cards.map(
                        (c) => html`<div class=${`org-plan ${c.current ? "current" : ""}`}>
                          <span class="org-plan-name">${c.name}</span>
                          <span class="org-plan-price tabular">${c.price}</span>
                          <ul>${c.items.map((x) => html`<li>${x}</li>`)}</ul>
                          ${c.buy
                            ? html`<lit-material-button variant=${c.recommended || c.lapsed ? "filled" : "outlined"} ?disabled=${this.busy} @click=${() => this.openBilling("checkout", org, c)}>${c.action}</lit-material-button>`
                            : html`<span class="hint">${c.current ? "Current plan" : c.note ?? ""}</span>`}
                        </div>`,
                      )}
                    </div>
                    <p class="hint">Payment on Stripe's secure checkout: HITOP never sees the card. Tax is added there. Cancel any time.</p>`}
      </section>`;
  }

  /** Stripe Checkout or the billing portal, in this browser tab's place (web only). */
  async openBilling(kind, org, card) {
    if (IS_TAURI) return;
    const base = `/v1/orgs/${enc(org.id)}/billing`;
    const r = await this.act(() => (kind === "portal" ? this.client.api("GET", `${base}/portal`) : this.client.api("POST", `${base}/checkout`, { plan: card.plan, period: card.period, ...(card.perSeat ? { seats: Math.max(1, this.data.usage.usage?.seats ?? 1) } : {}) })));
    if (r?.url && /^https:\/\//.test(r.url)) window.open(r.url, "_blank", "noopener");
  }

  tokensSection() {
    const server = (this.client.account?.()?.server ?? "https://sync.hitop.rws8.tech").replace(/\/+$/, "");
    return html`<section class="org-card" aria-label="Access tokens and MCP">
      <div class="org-head"><h2>Access tokens &amp; MCP</h2></div>
      <p class="hint">For scripts and AI agents. A token acts as you; secret values are never returned.</p>
      ${this.created ? html`<lit-material-alert variant="success"><span slot="title">“${this.created.name}” created. Copy it now: it isn't shown again.</span><code class="org-token">${this.created.token}</code></lit-material-alert>` : nothing}
      <div class="org-row">
        <lit-material-text-field id="token-name" variant="outlined" label="Token name" placeholder="Claude Code · laptop"></lit-material-text-field>
        <lit-material-select id="token-scope" variant="outlined" label="Scope" value="read">
          <lit-material-select-option value="read">Read</lit-material-select-option>
          <lit-material-select-option value="write">Read and write</lit-material-select-option>
        </lit-material-select>
        <lit-material-button variant="filled" ?disabled=${this.busy} @click=${() => this.createToken()}>New token</lit-material-button>
      </div>
      <div class="org-table" role="table" aria-label="Tokens">
        <div role="row" class="org-tr org-th"><span role="columnheader">Name</span><span role="columnheader">Scopes</span><span role="columnheader">Last used</span><span role="columnheader"><span class="visually-hidden">Actions</span></span></div>
        ${(this.tokens ?? []).map(
          (t) => html`<div role="row" class="org-tr">
            <span role="cell">${t.name}</span><span role="cell">${(t.scopes ?? []).join(", ")}</span>
            <span role="cell" class="hint">${t.lastUsedAt ? new Date(t.lastUsedAt * 1000).toLocaleDateString() : "never"}</span>
            <span role="cell"><lit-material-button variant="text" class="destructive" ?disabled=${this.busy} @click=${() => this.act(() => this.client.api("DELETE", `/v1/me/tokens/${enc(t.id)}`), `Revoked ${t.name}`).then(() => this.loadTokens())}>Revoke</lit-material-button></span>
          </div>`,
        )}
        ${this.tokens && !this.tokens.length ? html`<div role="row" class="org-tr"><span role="cell" class="hint">No tokens yet.</span></div>` : nothing}
      </div>
      <h3>Connect an agent</h3>
      <code class="run-command">claude mcp add --transport http hitop ${server}/mcp --header "Authorization: Bearer &lt;token&gt;"</code>
      <p class="hint">Local alternative that also runs requests on your machine: <code>claude mcp add hitop -- hitop mcp ./your-folder</code>.</p>
    </section>`;
  }

  async createToken() {
    const name = this.querySelector("#token-name")?.value?.trim();
    if (!name) return (this.error = "Give the token a name.");
    const scope = this.querySelector("#token-scope")?.value ?? "read";
    const scopes = scope === "write" ? ["read", "write"] : ["read"];
    const t = await this.act(() => this.client.api("POST", "/v1/me/tokens", { name, scopes, expiresInDays: 90 }));
    if (t) {
      this.created = { name, token: t.token };
      this.loadTokens();
    }
  }

  runs({ usage }) {
    return html`<section class="org-card" aria-label="Runs and monitors">
      <div class="org-head"><h2>Runs &amp; monitors</h2></div>
      <dl class="org-facts">
        <dt>Runs reported today</dt><dd class="tabular">${usage.usage?.runsToday ?? 0}${usage.runs?.runsPerDay != null ? ` of ${usage.runs.runsPerDay}` : ""}</dd>
        ${usage.runs ? html`<dt>Run history kept</dt><dd class="tabular">${usage.runs.historyDays} days</dd><dt>Alerts</dt><dd>${(usage.runs.alertChannels ?? []).join(", ")}</dd>` : nothing}
      </dl>
      <p class="hint">CI runs (<code>hitop run</code> with a runner credential) report to their shared collection; a collection's last run shows on its page with the full report. Monitors run a Cloud collection on a schedule and mail on failure and recovery: open the collection, then Share → Monitor.</p>
      <lit-material-button variant="outlined" @click=${() => this.app.go("collections")}>${icon("folders")} Collections</lit-material-button>
    </section>`;
  }

  securitySection({ org }) {
    const sec = this.security;
    if (!sec) return html`<section class="org-card" aria-label="Security and audit"><div class="skeleton block" aria-busy="true"></div></section>`;
    const limit = sec.refused ? limitInfo(sec.refused) : null;
    const base = `/v1/orgs/${enc(org.id)}`;
    return html`<section class="org-card" aria-label="Security and audit">
      <div class="org-head"><h2>Security &amp; audit</h2><span class="hint">Business</span></div>
      ${limit
        ? html`<lit-material-alert variant="info">${limit.message} Single sign-on, domain claims and the audit export come with Business.</lit-material-alert>`
        : sec.refused
          ? html`<p class="hint">${sec.refused.message}</p>`
          : html`
              <h3>Single sign-on</h3>
              ${sec.sso?.available === false
                ? html`<p class="hint">${sec.sso.reason}</p>`
                : html`<p>${sec.sso?.configured ? html`On · <code>${sec.sso.config?.issuer}</code>` : "Not set up."} ${sec.sso?.redirectUri ? html`<span class="hint">Redirect URI: <code>${sec.sso.redirectUri}</code></span>` : nothing}</p>`}
              <h3>Domains</h3>
              <div class="org-table" role="table" aria-label="Domains">
                ${sec.domains.map(
                  (d) => html`<div role="row" class="org-tr">
                    <span role="cell"><code>${d.domain}</code></span>
                    <span role="cell">${d.verified ? html`<span class="ok-text">Verified</span>` : html`<span class="warn-text">Waiting for DNS</span>`}</span>
                    <span role="cell" class="hint">${d.verified ? "" : html`TXT <code>${d.record}</code> = <code>${d.value}</code>`}</span>
                    <span role="cell">${d.verified ? nothing : html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.act(() => this.client.api("POST", `${base}/domains/${enc(d.domain)}/verify`)).then(() => this.loadSecurity())}>Check again</lit-material-button>`}</span>
                  </div>`,
                )}
              </div>
              <div class="org-row">
                <lit-material-text-field id="org-domain" variant="outlined" label="Add a domain" placeholder="acme.dev"></lit-material-text-field>
                <lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => {
                  const domain = this.querySelector("#org-domain")?.value?.trim();
                  if (domain) this.act(() => this.client.api("POST", `${base}/domains`, { domain })).then(() => this.loadSecurity());
                }}>Add</lit-material-button>
              </div>
              <h3>Audit log</h3>
              <lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.exportAudit(org)}>${icon("file-arrow-down")} Export JSONL</lit-material-button>
            `}
    </section>`;
  }

  async reloadOrgs() {
    this.busy = true;
    try {
      const { loadOrgs } = await import("../orgs.js");
      await loadOrgs(this.store, this.client);
    } finally {
      this.busy = false;
    }
  }

  async exportAudit(org) {
    const out = await this.act(() => this.client.call("GET", `/v1/orgs/${enc(org.id)}/audit/export`));
    if (!out) return;
    const text = out.text ?? "";
    await platform().saveFile(`${org.id}-audit.jsonl`, text, "application/x-ndjson");
  }

  render() {
    const s = this.store;
    if (!s.orgs?.length && this.client.account()) {
      return html`<div class="page"><lit-material-empty-state headline="No organizations loaded">
        Signed in as ${this.client.account().email}. HITOP Cloud didn't answer with your organizations.
        <lit-material-button slot="actions" variant="filled" ?disabled=${this.busy} @click=${() => this.reloadOrgs()}>Try again</lit-material-button>
      </lit-material-empty-state></div>`;
    }
    if (!s.orgs?.length) {
      return html`<div class="page"><lit-material-empty-state headline="Sign in to manage an organization">
        Organizations share collections and environments on HITOP Cloud.
        <lit-material-button slot="actions" variant="filled" @click=${() => this.app.openSignIn()}>Sign in</lit-material-button>
      </lit-material-empty-state></div>`;
    }
    const d = this.data;
    const sections = sectionsFor(d?.org);
    const body = !d
      ? html`<div class="skeleton block" aria-busy="true"></div>`
      : this.section === "members"
        ? this.members(d)
        : this.section === "billing"
          ? this.billing(d)
          : this.section === "tokens"
            ? this.tokensSection()
            : this.section === "runs"
              ? this.runs(d)
              : this.section === "security"
                ? this.securitySection(d)
                : this.general(d);
    return html`<div class="page org-page">
      <nav class="org-nav" aria-label="Organization settings">
        <span class="org-nav-title">${d?.org?.name ?? "Organization"}</span>
        ${sections.map(([id, label]) => html`<button type="button" class="org-nav-item" aria-current=${this.section === id ? "page" : "false"} @click=${() => this.pick(id)}>${label}</button>`)}
      </nav>
      <div class="org-main">
        ${this.error ? html`<lit-material-alert variant="error" class="refusal">${this.error}</lit-material-alert>` : nothing}
        ${body}
      </div>
    </div>`;
  }
}

customElements.define("hitop-org", HitopOrg);
