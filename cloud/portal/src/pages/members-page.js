// Members of an organization: roles in the row, pending invites, an invite dialog that says
// before sending when the plan is full, leave or remove with a confirm.
import "@lit-material/data-table";
import "@lit-material/select";
import "@lit-material/dialog";
import "@lit-material/text-field";
import "@lit-material/button";
import "@lit-material/icon-button";
import "@lit-material/empty-state";
import "@lit-material/skeleton";
import "@lit-material/alert";
import "@lit-material/status";
import { confirm } from "@lit-material/confirm";
import { navigate } from "@lit-material/router";
import { LitElement, css, html, nothing } from "lit";
import { act, describeError, errorAlert, loadError, sessionEnded } from "../actions.js";
import { notify } from "../notify.js";
import { orgName } from "../format.js";
import { removeIcon } from "../icons.js";
import { enc, isAdmin, isOwner, orgTask, refreshOrg } from "../org-data.js";
import { session } from "../session.js";
import { pastDueBanner } from "./org-page.js";
import { pageStyles } from "./page-styles.js";

export class MembersPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      lit-material-data-table lit-material-select {
        --lit-material-select-min-width: 120px;
      }
      .pending {
        margin-inline-start: var(--space-75);
      }
      .seats {
        margin-bottom: var(--space-200);
      }
    `,
  ];

  static properties = { orgId: {}, busy: { state: true }, error: { state: true }, dialogError: { state: true } };

  constructor() {
    super();
    this.busy = false;
    this.error = null;
    this.dialogError = null;
    this.data = orgTask(this, () => this.orgId);
  }

  willUpdate() {
    if (this.orgId) session.setOrg(this.orgId);
  }

  get dialog() {
    return this.renderRoot.querySelector("lit-material-dialog");
  }

  render() {
    return this.data.render({
      pending: () => html`<div class="title-row"><h1>Members</h1></div>${[1, 2, 3].map(() => html`<lit-material-skeleton height="44px"></lit-material-skeleton>`)}`,
      error: loadError,
      complete: (d) => this.body(d),
    });
  }

  body({ org, members, usage, billing }) {
    const admin = isAdmin(org) && !org.personal;
    const seats = usage.limits.seats;
    const full = seats !== null && seats !== undefined && usage.usage.seats >= seats;
    return html`
      <div class="title-row">
        <div><h1>Members</h1><p class="lead">${orgName(org)}: ${members.members.length} ${members.members.length === 1 ? "person" : "people"}${members.invites.length ? `, ${members.invites.length} invited` : ""}${seats !== null && seats !== undefined ? ` of ${seats} seats` : ""}. ${org.personal ? "A personal organization has only you; people you share a workspace with count as seats (Free: 3 people). For a team, create an organization." : "An organization role never opens a workspace: workspaces are still shared from hitop."}</p></div>
        ${admin ? html`<lit-material-button @click=${this.openInvite}>Invite</lit-material-button>` : nothing}
      </div>
      ${pastDueBanner(billing)}
      ${errorAlert(this.error)}
      ${full && admin
        ? html`<lit-material-alert class="seats" variant="info">Every seat is taken (${usage.usage.seats} of ${seats}). Invites are refused until there is room.<lit-material-button slot="action" variant="text" href="/orgs/${enc(org.id)}/plan">See plans</lit-material-button></lit-material-alert>`
        : nothing}
      ${this.table(org, members, admin)}
      ${admin ? this.inviteDialog(org, full, seats) : nothing}
    `;
  }

  table(org, members, admin) {
    const me = session.email;
    const roles = ["member", "admin", ...(isOwner(org) ? ["owner"] : [])];
    return html`<lit-material-data-table aria-label="Members">
      <lit-material-data-table-row header>
        <lit-material-data-table-cell header flex>Email</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Role</lit-material-data-table-cell>
        <lit-material-data-table-cell header><span class="sr-only">Actions</span></lit-material-data-table-cell>
      </lit-material-data-table-row>
      ${members.members.map(
        (m) => html`<lit-material-data-table-row data-row-id=${m.userId}>
          <lit-material-data-table-cell flex><span class="who"><span>${m.email}${m.email === me ? html` <span class="hint">(you)</span>` : nothing}</span></span></lit-material-data-table-cell>
          <lit-material-data-table-cell>
            ${admin
              ? html`<lit-material-select variant="filled" label="Role" .value=${m.role} ?disabled=${this.busy} @change=${(e) => this.changeRole(org, m, e.target.value)}>
                  ${[...new Set([...roles, m.role])].map((r) => html`<lit-material-select-option value=${r}>${r}</lit-material-select-option>`)}
                </lit-material-select>`
              : m.role}
          </lit-material-data-table-cell>
          <lit-material-data-table-cell>
            ${!org.personal && (admin || m.email === me)
              ? html`<lit-material-icon-button aria-label=${m.email === me ? "Leave the organization" : `Remove ${m.email}`} ?disabled=${this.busy} @click=${() => this.removeMember(org, m)}><span slot="icon">${removeIcon}</span></lit-material-icon-button>`
              : nothing}
          </lit-material-data-table-cell>
        </lit-material-data-table-row>`,
      )}
      ${members.invites.map(
        (i) => html`<lit-material-data-table-row data-row-id=${`invite:${i.email}`}>
          <lit-material-data-table-cell flex><span class="who"><span>${i.email} <lit-material-status class="pending" status="info">invited</lit-material-status></span></span></lit-material-data-table-cell>
          <lit-material-data-table-cell>${i.role}</lit-material-data-table-cell>
          <lit-material-data-table-cell>
            ${admin ? html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.uninvite(org, i)}>Cancel invite</lit-material-button>` : nothing}
          </lit-material-data-table-cell>
        </lit-material-data-table-row>`,
      )}
    </lit-material-data-table>`;
  }

  inviteDialog(org, full, seats) {
    const roles = ["member", "admin", ...(isOwner(org) ? ["owner"] : [])];
    return html`<lit-material-dialog @close=${() => (this.dialogError = null)}>
      <span slot="headline">Invite someone</span>
      <form id="invite-form" class="dialog-form" @submit=${(e) => this.sendInvite(e, org)}>
        ${errorAlert(this.dialogError)}
        ${full
          ? html`<lit-material-alert variant="warning">All ${seats} seats are taken. Add a seat first.<lit-material-button slot="action" variant="text" href="/orgs/${enc(org.id)}/plan">See plans</lit-material-button></lit-material-alert>`
          : nothing}
        <lit-material-text-field name="email" type="email" variant="outlined" label="Email to invite" required autocomplete="off"></lit-material-text-field>
        <lit-material-select name="role" variant="outlined" label="Role" value="member">
          ${roles.map((r) => html`<lit-material-select-option value=${r}>${r}</lit-material-select-option>`)}
        </lit-material-select>
        <p class="hint">An account joins at once; an address without one is invited by mail and joins when it first signs in. Either takes a seat.</p>
      </form>
      <lit-material-button slot="actions" variant="text" @click=${() => this.dialog.close()}>Cancel</lit-material-button>
      <lit-material-button slot="actions" type="submit" form="invite-form" ?disabled=${this.busy}>${this.busy ? "Sending…" : "Send invite"}</lit-material-button>
    </lit-material-dialog>`;
  }

  openInvite = () => {
    this.dialogError = null;
    this.dialog.show();
  };

  async sendInvite(event, org) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const email = String(data.get("email") || "").trim();
    const role = String(data.get("role") || "member");
    this.dialogError = null;
    this.busy = true;
    try {
      const r = await session.api.call("POST", `/v1/orgs/${enc(org.id)}/members`, { email, role });
      this.dialog.close();
      form.reset();
      refreshOrg(org.id);
      notify(r.status === "invited" ? `Invite sent to ${r.email}${r.mailed ? "" : " (not mailed on this server)"}` : `Added ${r.email}`);
    } catch (e) {
      if (!sessionEnded(e)) this.dialogError = describeError(e);
    } finally {
      this.busy = false;
    }
  }

  async changeRole(org, m, role) {
    if (role === m.role) return;
    const r = await act(this, () => session.api.call("PUT", `/v1/orgs/${enc(org.id)}/members/${enc(m.userId)}`, { role }), { success: `${m.email} is now ${role === "admin" || role === "owner" ? "an" : "a"} ${role}` });
    refreshOrg(org.id);
    return r;
  }

  async removeMember(org, m) {
    const me = m.email === session.email;
    const ok = await confirm({
      headline: me ? `Leave ${orgName(org)}?` : `Remove ${m.email}?`,
      message: me ? "You lose its workspaces' organization access right away. An owner can invite you again." : "They lose access right away. You can invite them again later.",
      confirmLabel: me ? "Leave" : "Remove",
      destructive: true,
    });
    if (!ok) return;
    const r = await act(this, () => session.api.call("DELETE", `/v1/orgs/${enc(org.id)}/members/${enc(m.userId)}`), { success: me ? `You left ${orgName(org)}` : `${m.email} removed` });
    if (!r.ok) return;
    refreshOrg(org.id);
    if (me) {
      session.setOrg("");
      navigate("/orgs");
    }
  }

  async uninvite(org, i) {
    const r = await act(this, () => session.api.call("DELETE", `/v1/orgs/${enc(org.id)}/invites/${enc(i.email)}`), { success: `Invite to ${i.email} cancelled` });
    if (r.ok) refreshOrg(org.id);
  }
}

customElements.define("members-page", MembersPage);
