// One organization at a glance: the plan in force, usage against its limits, and where its
// members, plan and settings are.
import "@lit-material/progress";
import "@lit-material/status";
import "@lit-material/alert";
import "@lit-material/button";
import "@lit-material/skeleton";
import { LitElement, css, html, nothing } from "lit";
import { loadError } from "../actions.js";
import { METRICS, orgName, planLabel, STATUS_NOTES, statusKind } from "../format.js";
import { enc, isAdmin, orgTask } from "../org-data.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

/** The "payment failed" banner (billing.pastDue), shared by the org, plan and members pages. */
export function pastDueBanner(billing) {
  if (!billing?.pastDue) return nothing;
  return html`<lit-material-alert class="banner" variant="warning" aria-label="Payment failed">
    <span slot="title">The last payment failed</span>
    Stripe retries it and mails the buyer; update the payment method in the billing portal. Until the retries run out the plan stays as it is; then Free limits apply (nothing is deleted).
  </lit-material-alert>`;
}

/** The usage meters of an organization (GET /v1/orgs/:org/usage). */
export function usageMeters(u, org) {
  return html`
    <div class="meters" aria-label="Usage">
      ${METRICS.map(({ key, label, format }) => {
        const max = u.limits[key];
        const used = u.usage[key] ?? 0;
        return html`<lit-material-usage-meter data-metric=${key} label=${label} .value=${used} .max=${max === null || max === undefined ? 0 : max} .format=${format || ((n) => String(n))}></lit-material-usage-meter>`;
      })}
    </div>
    ${u.usage.runsToday !== undefined
      ? html`<p class="hint" data-metric="runsPerDay">Run uploads today: ${u.usage.runsToday}${u.runs?.runsPerDay ? ` of ${u.runs.runsPerDay}` : ""}; run history kept ${u.runs?.historyDays ? `${u.runs.historyDays} days` : "without a limit"}${u.runs && !u.runs.sealedHistory ? " (status rows only on this plan)" : ""}.</p>`
      : nothing}
    ${u.usage.guests ? html`<p class="hint">${u.usage.guests} guest viewer(s); ${u.limits.guestViewers} of them free on this plan.</p>` : nothing}
    ${u.enforced ? nothing : html`<p class="hint">This server enforces no plan limits (self-hosted).</p>`}
    ${org && Object.values(u.limits).some((max, i) => max !== null && max !== undefined && (u.usage[METRICS[i]?.key] ?? 0) >= max)
      ? html`<p class="hint">A limit is reached: new people, workspaces, devices or uploads that grow storage are refused until there is room. Reading never stops. <a href="/orgs/${enc(org.id)}/plan">See plans</a></p>`
      : nothing}
  `;
}

export class OrgPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      .plan-line {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--space-100);
      }
      .cards a {
        text-decoration: none;
        color: inherit;
      }
      .cards .card {
        gap: var(--space-75);
        transition: background 150ms var(--ease, ease);
      }
      .cards .card:hover {
        background: var(--md-sys-color-surface-container-low);
      }
      .cards .card strong {
        color: var(--md-sys-color-primary);
      }
    `,
  ];

  static properties = { orgId: {} };

  constructor() {
    super();
    this.data = orgTask(this, () => this.orgId);
  }

  willUpdate() {
    if (this.orgId) session.setOrg(this.orgId);
  }

  render() {
    return this.data.render({
      pending: () => html`<div class="title-row"><h1><lit-material-skeleton width="12rem" height="1em"></lit-material-skeleton></h1></div>${[1, 2, 3].map(() => html`<lit-material-skeleton height="40px"></lit-material-skeleton>`)}`,
      error: loadError,
      complete: (d) => this.body(d),
    });
  }

  body({ org, members, usage, billing }) {
    const status = billing ? billing.status : usage.status;
    const base = `/orgs/${enc(org.id)}`;
    const admin = isAdmin(org);
    return html`
      <div class="title-row">
        <div>
          <h1>${orgName(org)}</h1>
          <p class="lead plan-line" aria-label="Plan">
            Plan <strong>${planLabel(org.plan, status)}</strong>
            ${org.plan && org.plan !== "free" && status ? html`<lit-material-status status=${statusKind(status)}>${STATUS_NOTES[status] || status}</lit-material-status>` : nothing}
            ${org.seats ? html`<span>${org.seats} seat${org.seats === 1 ? "" : "s"}</span>` : nothing}
            <span>You are ${org.role}.</span>
          </p>
        </div>
        ${admin ? html`<lit-material-button href="${base}/plan">${billing?.enabled ? "Plan and billing" : "Plan details"}</lit-material-button>` : nothing}
      </div>
      ${pastDueBanner(billing)}
      <section>
        <h2>Usage</h2>
        ${usageMeters(usage, org)}
      </section>
      <section>
        <h2>Pages</h2>
        <div class="cards">
          <a href="${base}/members"><div class="card"><strong>Members</strong><span>${members.members.length} ${members.members.length === 1 ? "person" : "people"}${members.invites.length ? `, ${members.invites.length} invited` : ""}</span><span class="hint">${org.personal ? "Only you; people you share a workspace with count as seats." : "Roles, invites, who is in."}</span></div></a>
          <a href="${base}/plan"><div class="card"><strong>Plan and billing</strong><span>${planLabel(org.plan, status)}</span><span class="hint">${billing?.enabled ? "Plans, seats, invoices, trial." : "Limits of this plan."}</span></div></a>
          ${admin && !org.personal
            ? html`<a href="${base}/settings"><div class="card"><strong>Settings</strong><span>General, SSO, SCIM, Audit, Admin</span><span class="hint">Owner and admin tools.</span></div></a>`
            : nothing}
        </div>
      </section>
      <section>
        <h2>Workspaces</h2>
        <p class="hint">${org.workspaces ?? usage.usage.workspaces ?? 0} shared in this organization (<code>hitop sync init --org ${org.personal ? "" : org.name}</code>). <a href="/workspaces">All your workspaces</a></p>
      </section>
    `;
  }
}

customElements.define("org-page", OrgPage);
