// Organization settings as tabs: General (name), SSO (domains, provider, a guided setup), SCIM
// (tokens, what a group gives), Audit (signed export, anchors), Admin (workspaces, devices) and
// the Danger zone (delete, typing the name). Each tab is its own route.
import "@lit-material/tabs";
import "@lit-material/text-field";
import "@lit-material/switch";
import "@lit-material/select";
import "@lit-material/button";
import "@lit-material/icon-button";
import "@lit-material/alert";
import "@lit-material/status";
import "@lit-material/skeleton";
import "@lit-material/data-table";
import "@lit-material/clipboard";
import "@lit-material/timestamp";
import "@lit-material/stepper";
import "@lit-material/empty-state";
import { confirm } from "@lit-material/confirm";
import { navigate } from "@lit-material/router";
import { TaskController, taskCache } from "@lit-material/task";
import { LitElement, css, html, nothing } from "lit";
import { act, errorAlert, loadError } from "../actions.js";
import { bytes, day, download, iso, orgName, shortId } from "../format.js";
import { removeIcon } from "../icons.js";
import { enc, isAdmin, isOwner, orgTask, refreshOrg } from "../org-data.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

const TABS = [
  ["general", "General"],
  ["sso", "SSO"],
  ["scim", "SCIM"],
  ["audit", "Audit"],
  ["admin", "Admin"],
  ["danger", "Danger zone"],
];

export class SettingsPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      lit-material-tabs {
        margin-bottom: var(--space-300);
      }
      .card {
        max-width: 40rem;
      }
      lit-material-stepper {
        min-width: max-content;
        margin-bottom: var(--space-300);
      }
      .ws-card h3 {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--space-100);
      }
      .ws-card + .ws-card {
        margin-top: var(--space-200);
      }
      lit-material-new-secret {
        margin-bottom: var(--space-200);
      }
      .group + .group {
        margin-top: var(--space-200);
      }
    `,
  ];

  static properties = { orgId: {}, tab: {}, busy: { state: true }, error: { state: true }, draftName: { state: true }, newToken: { state: true }, tested: { state: true } };

  constructor() {
    super();
    this.busy = false;
    this.error = null;
    this.draftName = null;
    this.newToken = null;
    this.tested = false;
    this.data = orgTask(this, () => this.orgId);
    // The tab's own data (402 on a plan without it: the refusal shows with its See plans link).
    this.extra = new TaskController(this, {
      task: async ([id, tab], signal) => {
        const api = session.api;
        const base = `/v1/orgs/${enc(id)}`;
        if (tab === "sso") return api.get(`${base}/sso`, { signal });
        if (tab === "audit") return api.get(`${base}/audit/anchors`, { signal });
        if (tab === "admin") return api.get(`${base}/admin`, { signal });
        if (tab === "scim") {
          const scim = await api.get(`${base}/scim`, { signal });
          const admin = await api.get(`${base}/admin`, { signal }).catch(() => ({ workspaces: [] }));
          return { ...scim, workspaceIds: admin.workspaces.map((w) => w.id) };
        }
        return null;
      },
      args: () => [this.orgId, this.tab],
      key: ([id, tab]) => ["org-settings", id, tab],
    });
  }

  willUpdate(changed) {
    if (this.orgId) session.setOrg(this.orgId);
    if (changed.has("tab")) {
      this.error = null;
      this.newToken = null;
    }
  }

  refresh() {
    refreshOrg(this.orgId);
    taskCache.invalidate(["org-settings", this.orgId]);
  }

  render() {
    const index = Math.max(0, TABS.findIndex(([t]) => t === this.tab));
    return html`
      <div class="title-row"><div><h1>Settings</h1><p class="lead">${this.data.value ? orgName(this.data.value.org) : ""}</p></div></div>
      <lit-material-tabs scrollable aria-label="Settings sections" .selected=${index} @change=${(e) => navigate(`/orgs/${enc(this.orgId)}/settings/${TABS[e.target.selected][0]}`)}>
        ${TABS.map(([, label]) => html`<lit-material-tab>${label}</lit-material-tab>`)}
      </lit-material-tabs>
      ${this.data.render({
        pending: () => html`${[1, 2].map(() => html`<lit-material-skeleton height="40px"></lit-material-skeleton>`)}`,
        error: loadError,
        complete: (d) => this.body(d),
      })}
    `;
  }

  body({ org }) {
    if (!isAdmin(org) || org.personal) {
      return html`<p class="hint">${org.personal ? "A personal organization has no settings: it is yours, with your workspaces. Create an organization for a team." : "Only owners and admins change an organization's settings."}</p>`;
    }
    if (this.tab === "general") return this.general(org);
    if (this.tab === "danger") return this.danger(org);
    return this.extra.render({
      pending: () => html`${[1, 2, 3].map(() => html`<lit-material-skeleton height="40px"></lit-material-skeleton>`)}`,
      error: loadError,
      complete: (x) => ({ sso: () => this.sso(org, x), scim: () => this.scim(org, x), audit: () => this.audit(org, x), admin: () => this.admin(org, x) })[this.tab]?.() ?? nothing,
    });
  }

  // --- General ---------------------------------------------------------------------------

  general(org) {
    const draft = this.draftName ?? org.name;
    const dirty = draft.trim() !== org.name && draft.trim() !== "";
    return html`<section class="card">
      <h2>Organization</h2>
      ${errorAlert(this.error)}
      <lit-material-text-field label="Name" variant="outlined" .value=${draft} @input=${(e) => (this.draftName = e.target.value)} ?disabled=${!isOwner(org) && !isAdmin(org)}></lit-material-text-field>
      <p class="hint">Named in invites, the CLI (<code>hitop sync init --org ${org.name}</code>) and receipts.</p>
      <div class="actions">
        <lit-material-button variant="text" ?disabled=${!dirty} @click=${() => (this.draftName = null)}>Discard</lit-material-button>
        <lit-material-button ?disabled=${!dirty || this.busy} @click=${() => this.rename(org, draft.trim())}>Save</lit-material-button>
      </div>
    </section>`;
  }

  async rename(org, name) {
    const r = await act(this, () => session.api.call("PUT", `/v1/orgs/${enc(org.id)}`, { name }), { success: "Renamed" });
    if (r.ok) {
      this.draftName = null;
      this.refresh();
    }
  }

  // --- Danger zone -----------------------------------------------------------------------

  danger(org) {
    return html`<section class="card danger">
      <h2>Delete this organization</h2>
      ${errorAlert(this.error)}
      <p class="hint">Removes every member and invite and ends its plan. It must have no workspaces (move or delete them in hitop first). This can't be undone.</p>
      <div class="actions"><lit-material-button ?disabled=${!isOwner(org) || this.busy} @click=${() => this.deleteOrg(org)}>Delete organization</lit-material-button></div>
      ${isOwner(org) ? nothing : html`<p class="hint">Only an owner deletes an organization.</p>`}
    </section>`;
  }

  async deleteOrg(org) {
    const ok = await confirm({
      headline: `Delete ${org.name}?`,
      message: "Every member and invite goes with it and its plan ends. Type the organization's name to confirm.",
      confirmLabel: "Delete",
      destructive: true,
      requireText: org.name,
    });
    if (!ok) return;
    const r = await act(this, () => session.api.call("DELETE", `/v1/orgs/${enc(org.id)}`), { success: `${org.name} deleted` });
    if (!r.ok) return;
    taskCache.invalidate(["orgs"]);
    session.setOrg("");
    navigate("/orgs");
  }

  // --- Single sign-on (C5) -----------------------------------------------------------------

  sso(org, sso) {
    const owner = isOwner(org);
    if (!sso.available) {
      return html`<section class="card">
        <h2>Single sign-on</h2>
        <p class="hint" aria-label="Not available">${sso.reason}. A self-hosted server turns it on with a host key (<code>HITOP_SYNC_HOST_KEY_FILE</code>, see the sync server's README).</p>
      </section>`;
    }
    const c = sso.config || {};
    const verified = sso.domains.filter((d) => d.verified).length;
    const steps = [
      ["Claim a domain", sso.domains.length > 0],
      ["Publish the TXT record and verify", verified > 0],
      ["Enter the provider", Boolean(sso.configured)],
      ["Test the sign-in", this.tested],
      ["Turn it on", Boolean(sso.configured && c.enabled !== false && sso.allowed)],
    ];
    const active = Math.max(0, steps.findIndex(([, done]) => !done));
    return html`
      <p class="lead">Your people sign in with your identity provider (OpenID Connect: Entra ID, Google Workspace, Okta, Keycloak...). It signs in only addresses of domains you verified here. Like every sign-in it gives access, never keys: workspaces stay end-to-end encrypted.</p>
      ${sso.allowed ? nothing : html`<lit-material-alert class="banner" variant="warning">Single sign-on comes with Business.<lit-material-button slot="action" variant="text" href="/orgs/${enc(org.id)}/plan">See plans</lit-material-button></lit-material-alert>`}
      ${errorAlert(this.error)}
      <div class="scroll"><lit-material-stepper orientation="horizontal" .selected=${active} aria-label="Setup steps">
        ${steps.map(([label, done]) => html`<lit-material-step ?completed=${done}><span slot="label">${label}</span></lit-material-step>`)}
      </lit-material-stepper></div>
      <section>
        <h2>Domains</h2>
        <p class="hint">Prove you own a domain with a DNS TXT record${sso.dnsVerification ? "" : " (this server can't look them up yet: HITOP_SYNC_DNS_URL isn't set)"}.</p>
        ${sso.domains.length
          ? html`<div class="scroll"><lit-material-data-table aria-label="Domains">
              <lit-material-data-table-row header>
                <lit-material-data-table-cell header flex>Domain</lit-material-data-table-cell>
                <lit-material-data-table-cell header>TXT record</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Status</lit-material-data-table-cell>
                <lit-material-data-table-cell header><span class="sr-only">Actions</span></lit-material-data-table-cell>
              </lit-material-data-table-row>
              ${sso.domains.map(
                (d) => html`<lit-material-data-table-row data-row-id=${d.domain}>
                  <lit-material-data-table-cell flex>${d.domain}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>
                    <div class="grid">
                      <lit-material-clipboard-copy label="Record name of ${d.domain}" value=${d.record}></lit-material-clipboard-copy>
                      <lit-material-clipboard-copy label="Record value of ${d.domain}" value=${d.value}></lit-material-clipboard-copy>
                    </div>
                  </lit-material-data-table-cell>
                  <lit-material-data-table-cell>${d.verified ? html`<lit-material-status status="ok">verified ${day(d.verifiedAt)}</lit-material-status>` : html`<lit-material-status status="warning">not verified</lit-material-status>`}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>
                    ${d.verified ? nothing : html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.verifyDomain(org, d)}>Verify</lit-material-button>`}
                    <lit-material-icon-button aria-label="Remove ${d.domain}" ?disabled=${this.busy} @click=${() => this.removeDomain(org, d)}><span slot="icon">${removeIcon}</span></lit-material-icon-button>
                  </lit-material-data-table-cell>
                </lit-material-data-table-row>`,
              )}
            </lit-material-data-table></div>`
          : html`<lit-material-empty-state size="compact" headline="No domains yet" level="3">Claim the domain of your work addresses, publish the TXT record it gives you, then verify.</lit-material-empty-state>`}
        <form class="row" @submit=${(e) => this.claimDomain(e, org)}>
          <lit-material-text-field name="domain" variant="outlined" label="Domain to claim" required pattern="[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"></lit-material-text-field>
          <lit-material-button variant="outlined" type="submit" ?disabled=${this.busy}>Claim domain</lit-material-button>
        </form>
      </section>
      <section>
        <h2>Identity provider</h2>
        <p class="hint">Register a web application (confidential client, authorization code with PKCE) with this redirect URI. ${verified ? "" : "Verify a domain first: nobody can sign in through it until then."}${owner ? "" : " Only owners change these settings."}</p>
        <lit-material-clipboard-copy label="Redirect URI" value=${sso.redirectUri}></lit-material-clipboard-copy>
        <form class="grid card" aria-label="SSO settings" @submit=${(e) => this.saveSso(e, org)}>
          <lit-material-text-field name="issuer" variant="outlined" label="Issuer URL" type="url" .value=${c.issuer || ""} ?disabled=${!owner} required></lit-material-text-field>
          <lit-material-text-field name="clientId" variant="outlined" label="Client ID" .value=${c.clientId || ""} ?disabled=${!owner} required></lit-material-text-field>
          <lit-material-text-field name="clientSecret" variant="outlined" type="password" label="Client secret" autocomplete="off" placeholder=${c.hasSecret ? "stored (leave empty to keep it)" : ""} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field name="name" variant="outlined" label="Button name" .value=${c.name || "SSO"} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field name="scopes" variant="outlined" label="Scopes" .value=${c.scopes || "openid email profile"} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field name="groupsClaim" variant="outlined" label="Groups claim" .value=${c.groupsClaim || "groups"} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-text-field name="requiredGroups" variant="outlined" label="Required groups (comma separated, empty = any)" .value=${(c.requiredGroups || []).join(", ")} ?disabled=${!owner}></lit-material-text-field>
          <lit-material-switch name="required" label="SSO required" aria-label="SSO required" supporting-text="Your domains' addresses sign in only through it (no mail codes, passwords, passkeys, Google or GitHub)" ?checked=${Boolean(c.required)} ?disabled=${!owner}></lit-material-switch>
          <lit-material-switch name="assumeVerified" label="Trust addresses in your verified domains" aria-label="Trust addresses in your verified domains" supporting-text="For a provider that doesn't send email_verified (Entra ID)" ?checked=${Boolean(c.assumeVerified)} ?disabled=${!owner}></lit-material-switch>
          <lit-material-switch name="enabled" label="On" aria-label="On" ?checked=${c.enabled !== false} ?disabled=${!owner}></lit-material-switch>
          <div class="actions">
            ${sso.configured && owner ? html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.removeSso(org)}>Remove single sign-on</lit-material-button>` : nothing}
            ${sso.configured ? html`<lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.testSso(org)}>Test</lit-material-button>` : nothing}
            ${owner ? html`<lit-material-button type="submit" ?disabled=${this.busy}>Save</lit-material-button>` : nothing}
          </div>
        </form>
        <p class="hint">People sign in with <code>hitop sync login --sso --email you@your-domain</code>, or "Sign in with SSO" and their work address in the apps and here.</p>
      </section>
    `;
  }

  async claimDomain(event, org) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const domain = String(new FormData(form).get("domain") || "").trim().toLowerCase();
    const r = await act(this, () => session.api.call("POST", `/v1/orgs/${enc(org.id)}/domains`, { domain }), { success: (d) => `Publish the TXT record ${d.record} with the value ${d.value}, then Verify` });
    if (r.ok) {
      form.reset();
      this.refresh();
    }
  }

  async verifyDomain(org, d) {
    const r = await act(this, async () => {
      const v = await session.api.call("POST", `/v1/orgs/${enc(org.id)}/domains/${enc(d.domain)}/verify`);
      if (v.problem) throw new Error(v.problem);
      return v;
    }, { success: `${d.domain} is verified` });
    this.refresh();
    return r;
  }

  async removeDomain(org, d) {
    const ok = await confirm({ headline: `Remove ${d.domain}?`, message: "Its addresses can no longer sign in through single sign-on.", confirmLabel: "Remove", destructive: true });
    if (!ok) return;
    const r = await act(this, () => session.api.call("DELETE", `/v1/orgs/${enc(org.id)}/domains/${enc(d.domain)}`), { success: `${d.domain} removed` });
    if (r.ok) this.refresh();
  }

  async saveSso(event, org) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const text = (name) => String(data.get(name) || "").trim();
    const body = {
      issuer: text("issuer"),
      clientId: text("clientId"),
      name: text("name"),
      scopes: text("scopes"),
      groupsClaim: text("groupsClaim"),
      requiredGroups: text("requiredGroups").split(",").map((g) => g.trim()).filter(Boolean),
      required: data.get("required") !== null,
      assumeVerified: data.get("assumeVerified") !== null,
      enabled: data.get("enabled") !== null,
    };
    const secret = String(data.get("clientSecret") || "");
    if (secret) body.clientSecret = secret;
    const r = await act(this, () => session.api.call("PUT", `/v1/orgs/${enc(org.id)}/sso`, body), { success: "Single sign-on saved" });
    if (r.ok) this.refresh();
  }

  async testSso(org) {
    const r = await act(this, async () => {
      const t = await session.api.call("POST", `/v1/orgs/${enc(org.id)}/sso/test`);
      if (!t.ok) throw new Error(`The provider check failed: ${t.error}`);
      return t;
    }, { success: "The provider answers: discovery and keys fetched" });
    this.tested = r.ok;
  }

  async removeSso(org) {
    const ok = await confirm({ headline: "Remove single sign-on?", message: "People sign in as before (mail code, password, passkey); accounts stay.", confirmLabel: "Remove", destructive: true });
    if (!ok) return;
    const r = await act(this, () => session.api.call("DELETE", `/v1/orgs/${enc(org.id)}/sso`), { success: "Single sign-on removed" });
    if (r.ok) this.refresh();
  }

  // --- SCIM (C6) -------------------------------------------------------------------------

  scim(org, scim) {
    const owner = isOwner(org);
    const workspaces = scim.workspaceIds || [];
    return html`
      <p class="lead">Your identity provider (Okta, Entra ID, Keycloak...) adds and removes the organization's members and their groups. A group gives an organization role and workspace roles, set here. SCIM gives access, never keys: a workspace's owner still adds a new person's device (<code>hitop sync members</code>, then <code>hitop sync add</code>), and someone removed is out at once while the owners rotate the key.</p>
      ${errorAlert(this.error)}
      <section>
        <h2>Connection</h2>
        <p class="hint">Bearer token: one of the tokens below. userName must be the person's email address. ${scim.enabled ? `${scim.activeUsers} active of ${scim.users} provisioned users${scim.pendingWorkspaceAdds ? `, ${scim.pendingWorkspaceAdds} waiting for a workspace owner to add them` : ""}.` : "Off: no token yet."}</p>
        <lit-material-clipboard-copy label="SCIM base URL" value=${scim.baseUrl}></lit-material-clipboard-copy>
      </section>
      <section>
        <h2>Tokens</h2>
        ${this.newToken
          ? html`<lit-material-new-secret label="New SCIM token" value=${this.newToken}>
              <lit-material-button slot="actions" variant="text" @click=${() => (this.newToken = null)}>Done</lit-material-button>
            </lit-material-new-secret>`
          : nothing}
        ${scim.tokens.length
          ? html`<div class="scroll"><lit-material-data-table aria-label="SCIM tokens">
              <lit-material-data-table-row header>
                <lit-material-data-table-cell header flex>Name</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Created</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Last used</lit-material-data-table-cell>
                <lit-material-data-table-cell header><span class="sr-only">Actions</span></lit-material-data-table-cell>
              </lit-material-data-table-row>
              ${scim.tokens.map(
                (t) => html`<lit-material-data-table-row data-row-id=${t.id}>
                  <lit-material-data-table-cell flex>${t.name}</lit-material-data-table-cell>
                  <lit-material-data-table-cell><lit-material-timestamp date=${iso(t.createdAt)}></lit-material-timestamp></lit-material-data-table-cell>
                  <lit-material-data-table-cell>${t.lastUsedAt ? html`<lit-material-timestamp date=${iso(t.lastUsedAt)} relative></lit-material-timestamp>` : "never"}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>${owner ? html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.revokeScim(org, t)}>Revoke</lit-material-button>` : nothing}</lit-material-data-table-cell>
                </lit-material-data-table-row>`,
              )}
            </lit-material-data-table></div>`
          : html`<lit-material-empty-state size="compact" headline="No SCIM token yet" level="3">Create one and paste it into your identity provider's SCIM connection.</lit-material-empty-state>`}
        ${owner
          ? html`<form class="row" @submit=${(e) => this.createScim(e, org)}>
              <lit-material-text-field name="name" variant="outlined" label="Token name (your identity provider)" required></lit-material-text-field>
              <lit-material-button type="submit" ?disabled=${this.busy}>Create SCIM token</lit-material-button>
            </form>`
          : html`<p class="hint">Only owners create or revoke SCIM tokens.</p>`}
      </section>
      <section>
        <h2>Groups</h2>
        <p class="hint">Groups appear when your identity provider pushes them. Owners are never set by SCIM; when any group gives an organization role, members without one are members.</p>
        ${scim.groups.length ? nothing : html`<lit-material-empty-state size="compact" headline="No groups yet" level="3">Push a group from your identity provider and it shows here.</lit-material-empty-state>`}
        ${scim.groups.map((g) => this.group(org, g, workspaces))}
      </section>
    `;
  }

  group(org, g, workspaces) {
    const save = (change) => this.saveGroup(org, g, change);
    return html`<div class="card group" aria-label="Group ${g.displayName}">
      <h3>${g.displayName} <span class="hint">${g.members} member(s)</span></h3>
      <lit-material-select variant="outlined" label="Organization role" .value=${g.orgRole || ""} ?disabled=${this.busy} @change=${(e) => save({ orgRole: e.target.value })}>
        ${[["", "no organization role"], ["member", "member"], ["admin", "admin"]].map(([v, l]) => html`<lit-material-select-option value=${v}>${l}</lit-material-select-option>`)}
      </lit-material-select>
      ${g.workspaces.length
        ? html`<lit-material-data-table aria-label="Workspaces of ${g.displayName}">
            <lit-material-data-table-row header>
              <lit-material-data-table-cell header flex>Workspace</lit-material-data-table-cell>
              <lit-material-data-table-cell header>Role</lit-material-data-table-cell>
              <lit-material-data-table-cell header><span class="sr-only">Actions</span></lit-material-data-table-cell>
            </lit-material-data-table-row>
            ${g.workspaces.map(
              (w) => html`<lit-material-data-table-row data-row-id=${w.workspace}>
                <lit-material-data-table-cell flex class="mono">${shortId(w.workspace)}</lit-material-data-table-cell>
                <lit-material-data-table-cell>${w.role}</lit-material-data-table-cell>
                <lit-material-data-table-cell><lit-material-button variant="text" ?disabled=${this.busy} @click=${() => save({ workspaces: g.workspaces.filter((x) => x.workspace !== w.workspace) })}>Remove</lit-material-button></lit-material-data-table-cell>
              </lit-material-data-table-row>`,
            )}
          </lit-material-data-table>`
        : html`<p class="hint">No workspace yet.</p>`}
      <form class="row" @submit=${(e) => this.giveWorkspace(e, org, g)}>
        <lit-material-select name="workspace" variant="outlined" label="Workspace" ?disabled=${!workspaces.length} required>
          ${workspaces.map((w) => html`<lit-material-select-option value=${w}>${shortId(w)}</lit-material-select-option>`)}
        </lit-material-select>
        <lit-material-select name="role" variant="outlined" label="Workspace role" value="viewer">
          ${["viewer", "editor"].map((r) => html`<lit-material-select-option value=${r}>${r}</lit-material-select-option>`)}
        </lit-material-select>
        <lit-material-button variant="outlined" type="submit" ?disabled=${this.busy || !workspaces.length}>Give workspace</lit-material-button>
      </form>
    </div>`;
  }

  async saveGroup(org, g, change) {
    const body = { orgRole: g.orgRole, workspaces: g.workspaces.map((w) => ({ workspace: w.workspace, role: w.role })), ...change };
    const r = await act(this, () => session.api.call("PUT", `/v1/orgs/${enc(org.id)}/scim/groups/${enc(g.id)}`, body), { success: `Saved what ${g.displayName} gives; applied to its members` });
    this.refresh();
    return r;
  }

  giveWorkspace(event, org, g) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const workspace = String(data.get("workspace") || "");
    const role = String(data.get("role") || "viewer");
    if (!workspace) return;
    return this.saveGroup(org, g, { workspaces: [...g.workspaces.filter((x) => x.workspace !== workspace), { workspace, role }] });
  }

  async createScim(event, org) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const name = String(new FormData(form).get("name") || "").trim();
    const r = await act(this, () => session.api.call("POST", `/v1/orgs/${enc(org.id)}/scim/tokens`, { name }), { success: `SCIM token ${name} created` });
    if (!r.ok) return;
    form.reset();
    this.newToken = r.value.token;
    this.refresh();
  }

  async revokeScim(org, t) {
    const ok = await confirm({ headline: `Revoke the SCIM token "${t.name}"?`, message: "Your identity provider stops provisioning until it has a new one.", confirmLabel: "Revoke", destructive: true });
    if (!ok) return;
    const r = await act(this, () => session.api.call("DELETE", `/v1/orgs/${enc(org.id)}/scim/tokens/${enc(t.id)}`), { success: `Revoked ${t.name}` });
    if (r.ok) this.refresh();
  }

  // --- Audit (C5) ------------------------------------------------------------------------

  audit(org, audit) {
    const owner = isOwner(org);
    const base = `/v1/orgs/${enc(org.id)}/audit`;
    return html`
      <p class="lead">The audit log of the organization and its workspaces (sign-ins aside: who added, removed or invited whom, roles, key rotations, SSO and admin changes). Never contents, keys or tokens.</p>
      ${errorAlert(this.error)}
      <section>
        <h2>Signed export</h2>
        <p class="hint">JSON lines: every chain's rows and head, signed by the server (Ed25519, key <code>${audit.keyId}</code>). Keep it next to the anchors you were mailed: <code>hitop sync audit --verify --anchor anchor.json</code> recomputes the chains and checks each anchored head is still there, so a rewritten log shows.</p>
        <div class="row">
          <lit-material-button ?disabled=${this.busy} @click=${() => act(this, async () => download(`hitop-audit-${org.id}-${new Date().toISOString().slice(0, 10)}.jsonl`, await session.api.blob(`${base}/export`)))}>Download signed export</lit-material-button>
          <lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => act(this, async () => {
            const a = await session.api.call("POST", `${base}/anchors`);
            download(`hitop-anchor-${org.id}-${a.day}-now.json`, JSON.stringify(a, null, 2));
          })}>Anchor now</lit-material-button>
        </div>
      </section>
      <section>
        <h2>Daily anchors</h2>
        <p class="hint">Once a day the server signs the heads of the organization's chains${audit.anchorsOn ? "" : " (turned off on this server)"}; when they moved, it mails the anchor to the owners. Keep those mails: <code>hitop sync audit --verify --anchor anchor.json</code> checks the log against one.</p>
        <div class="grid card" aria-label="Anchor settings">
          <lit-material-switch id="audit-mail" label="Mail anchors to the owners" aria-label="Mail anchors to the owners" ?checked=${audit.mailAnchors} ?disabled=${!owner || this.busy} @change=${(e) => this.auditSetting(org, "mailAnchors", e.target.checked)}></lit-material-switch>
          <lit-material-switch id="audit-public" label="Publish anchors" aria-label="Publish anchors" supporting-text="Anyone can fetch them at ${audit.publicUrl}<day>.json; they show how many workspaces the organization has and their ids" ?checked=${audit.publicAnchors} ?disabled=${!owner || this.busy} @change=${(e) => this.auditSetting(org, "publicAnchors", e.target.checked)}></lit-material-switch>
        </div>
        ${audit.anchors.length
          ? html`<div class="scroll"><lit-material-data-table aria-label="Anchors">
              <lit-material-data-table-row header>
                <lit-material-data-table-cell header flex>Day</lit-material-data-table-cell>
                <lit-material-data-table-cell header numeric>Chains</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Mailed</lit-material-data-table-cell>
                <lit-material-data-table-cell header><span class="sr-only">Actions</span></lit-material-data-table-cell>
              </lit-material-data-table-row>
              ${audit.anchors.map(
                (x) => html`<lit-material-data-table-row data-row-id=${x.day}>
                  <lit-material-data-table-cell flex class="mono">${x.day}</lit-material-data-table-cell>
                  <lit-material-data-table-cell numeric class="mono">${x.anchor?.chains?.length ?? 0}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>${x.mailedAt ? day(x.mailedAt) : "no"}</lit-material-data-table-cell>
                  <lit-material-data-table-cell><lit-material-button variant="text" @click=${() => download(`hitop-anchor-${org.id}-${x.day}.json`, JSON.stringify(x.anchor, null, 2))}>Download</lit-material-button></lit-material-data-table-cell>
                </lit-material-data-table-row>`,
              )}
            </lit-material-data-table></div>`
          : html`<p class="hint" aria-label="No anchors">No daily anchor yet: the first one comes within the hour.</p>`}
      </section>
    `;
  }

  async auditSetting(org, key, value) {
    const r = await act(this, () => session.api.call("PUT", `/v1/orgs/${enc(org.id)}/audit/settings`, { [key]: value }), { success: "Audit settings saved" });
    this.refresh();
    return r;
  }

  // --- Admin console (C5) ------------------------------------------------------------------

  admin(org, a) {
    const base = `/v1/orgs/${enc(org.id)}/admin`;
    return html`
      <p class="lead">Every workspace of the organization, its people and their devices. An organization role never opens a workspace: this page shows who has access, never what is inside, and the server holds no key to show it.</p>
      ${errorAlert(this.error)}
      <section>
        <h2>Workspaces</h2>
        ${a.workspaces.length ? nothing : html`<lit-material-empty-state size="compact" headline="No workspaces in this organization yet" level="3">Share one into it: <code>hitop sync init --org ${org.name}</code>.</lit-material-empty-state>`}
        ${a.workspaces.map((w) => {
          const people = w.members.filter((m) => !m.runner);
          return html`<div class="card ws-card" aria-label="Workspace ${shortId(w.id)}">
            <h3><span class="mono">${shortId(w.id)}</span> ${w.rotationRequested ? html`<lit-material-status status="warning" aria-label="Rotation requested">key rotation requested by ${w.rotationRequested.by}</lit-material-status>` : nothing}</h3>
            <p class="hint">Key epoch ${w.epoch}, ${bytes(w.bytes)}, created ${day(w.createdAt)}.</p>
            <lit-material-data-table aria-label="People of ${shortId(w.id)}">
              <lit-material-data-table-row header>
                <lit-material-data-table-cell header flex>Email</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Role</lit-material-data-table-cell>
              </lit-material-data-table-row>
              ${w.members.map((m) => html`<lit-material-data-table-row><lit-material-data-table-cell flex>${m.email}</lit-material-data-table-cell><lit-material-data-table-cell>${m.runner ? "runner" : m.role}</lit-material-data-table-cell></lit-material-data-table-row>`)}
            </lit-material-data-table>
            ${a.actions.transferOwner
              ? html`<form class="row" @submit=${(e) => this.transfer(e, base, w, people)}>
                  <lit-material-select name="owner" variant="outlined" label="New owner" required>
                    ${people.map((m) => html`<lit-material-select-option value=${m.userId}>${m.email}</lit-material-select-option>`)}
                  </lit-material-select>
                  <lit-material-switch name="keep" label="Keep the other owners" aria-label="Keep the other owners"></lit-material-switch>
                  <lit-material-button variant="outlined" type="submit" ?disabled=${this.busy}>Transfer ownership</lit-material-button>
                </form>`
              : nothing}
            ${a.actions.forceRotation
              ? html`<div class="row"><lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.rotation(base, w)}>Ask for a key rotation</lit-material-button></div>`
              : nothing}
          </div>`;
        })}
      </section>
      <section>
        <h2>Devices</h2>
        <p class="hint">Device keys of the organization's members. Removing one (a lost or stolen laptop) takes it out of this organization's workspaces only: its copies of their keys are deleted and each of those workspaces asks its owners for a rotation. The device stays on the person's account, with their personal and other organizations' workspaces. Their other devices keep working.</p>
        ${a.devices.length
          ? html`<div class="scroll"><lit-material-data-table aria-label="Devices">
              <lit-material-data-table-row header>
                <lit-material-data-table-cell header flex>Email</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Device</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Added</lit-material-data-table-cell>
                <lit-material-data-table-cell header numeric>Workspaces here</lit-material-data-table-cell>
                <lit-material-data-table-cell header><span class="sr-only">Actions</span></lit-material-data-table-cell>
              </lit-material-data-table-row>
              ${a.devices.map(
                (d) => html`<lit-material-data-table-row data-row-id=${d.keyId}>
                  <lit-material-data-table-cell flex>${d.email}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>${d.name}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>${day(d.createdAt)}</lit-material-data-table-cell>
                  <lit-material-data-table-cell numeric class="mono">${d.orgWorkspaces}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>${a.actions.removeDevice ? html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.removeDevice(base, d)}>Remove</lit-material-button>` : nothing}</lit-material-data-table-cell>
                </lit-material-data-table-row>`,
              )}
            </lit-material-data-table></div>`
          : html`<p class="hint">No device keys yet.</p>`}
        ${a.actions.forceRotation ? nothing : html`<lit-material-alert variant="info">Key rotation requests and removing devices come with Business.<lit-material-button slot="action" variant="text" href="/orgs/${enc(org.id)}/plan">See plans</lit-material-button></lit-material-alert>`}
      </section>
    `;
  }

  async transfer(event, base, w, people) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const userId = String(data.get("owner") || "");
    const who = people.find((m) => m.userId === userId)?.email || userId;
    const r = await act(this, () => session.api.call("PUT", `${base}/workspaces/${enc(w.id)}/owner`, { userId, keepOwners: data.get("keep") !== null }), { success: `${who} owns ${shortId(w.id)} now` });
    if (r.ok) this.refresh();
  }

  async rotation(base, w) {
    const r = await act(this, () => session.api.call("POST", `${base}/workspaces/${enc(w.id)}/rotation`), { success: "Rotation requested: the workspace's owners are asked to rotate its key (their hitop does it; the server can't)" });
    if (r.ok) this.refresh();
  }

  async removeDevice(base, d) {
    const ok = await confirm({
      headline: `Remove the device "${d.name}" of ${d.email}?`,
      message: "It is removed from this organization's workspaces: its copies of their keys are deleted and those workspaces ask for a key rotation. Their personal and other organizations' workspaces aren't touched. Their other devices keep working.",
      confirmLabel: "Remove",
      destructive: true,
    });
    if (!ok) return;
    const r = await act(this, () => session.api.call("DELETE", `${base}/devices/${enc(d.keyId)}`), { success: `Removed ${d.name} of ${d.email} from this organization's workspaces` });
    if (r.ok) this.refresh();
  }
}

customElements.define("settings-page", SettingsPage);
