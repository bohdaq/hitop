// Plan and billing of an organization: the plan in force and its status, usage meters, the plan
// picker (Stripe Checkout, Managed Payments), seats, the billing portal and the Team trial.
import "@lit-material/pricing";
import "@lit-material/progress";
import "@lit-material/text-field";
import "@lit-material/button";
import "@lit-material/alert";
import "@lit-material/status";
import "@lit-material/skeleton";
import { LitElement, css, html, nothing } from "lit";
import { act, errorAlert, loadError } from "../actions.js";
import { day, orgName, PLAN_FEATURES, PLAN_NAMES, PLAN_NOTES, planLabel, PRICES, STATUS_NOTES, statusKind } from "../format.js";
import { enc, isAdmin, orgTask, refreshOrg } from "../org-data.js";
import { session } from "../session.js";
import { pastDueBanner, usageMeters } from "./org-page.js";
import { pageStyles } from "./page-styles.js";

export class PlanPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      .status-line {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--space-100);
      }
      .seats-field {
        max-width: 12rem;
      }
      lit-material-plan-picker {
        margin-top: var(--space-200);
      }
      .notes {
        margin-top: var(--space-200);
      }
    `,
  ];

  static properties = { orgId: {}, busy: { state: true }, error: { state: true }, leaving: { state: true }, seats: { state: true } };

  constructor() {
    super();
    this.busy = false;
    this.error = null;
    // Set while the browser loads Stripe's page: a second click would open a second checkout.
    this.leaving = false;
    this.seats = 0;
    this.data = orgTask(this, () => this.orgId);
    this.onShow = () => (this.leaving = false);
  }

  connectedCallback() {
    super.connectedCallback();
    window.addEventListener("pageshow", this.onShow);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener("pageshow", this.onShow);
  }

  willUpdate() {
    if (this.orgId) session.setOrg(this.orgId);
  }

  render() {
    return this.data.render({
      pending: () => html`<div class="title-row"><h1>Plan and billing</h1></div>${[1, 2, 3].map(() => html`<lit-material-skeleton height="40px"></lit-material-skeleton>`)}`,
      error: loadError,
      complete: (d) => this.body(d),
    });
  }

  body({ org, usage, billing }) {
    const admin = isAdmin(org);
    const status = billing ? billing.status : usage.status;
    const label = planLabel(org.plan, status);
    return html`
      <div class="title-row">
        <div>
          <h1>Plan and billing</h1>
          <p class="lead">${orgName(org)} is on <strong>${label}</strong>. ${PLAN_NOTES[usage.planInForce] || ""}${usage.grandfathered ? " Your limits were raised to what you used before plans existed." : ""}</p>
        </div>
      </div>
      ${pastDueBanner(billing)}
      ${usage.planInForce !== usage.plan ? html`<lit-material-alert class="banner" variant="error">The ${usage.plan} plan is ${STATUS_NOTES[status] || `${status}: Free limits`}. Nothing was deleted.</lit-material-alert>` : nothing}
      ${errorAlert(this.error)}
      ${billing && org.plan !== "free"
        ? html`<section class="card" aria-label="Subscription">
            <div class="status-line">
              <strong>${PLAN_NAMES[org.plan] || org.plan}</strong>
              <lit-material-status status=${statusKind(status)}>${STATUS_NOTES[status] || status}</lit-material-status>
              ${billing.seats && ["team", "business"].includes(org.plan) ? html`<span>${billing.seats} seat(s) bought, ${billing.seatsUsed} in use</span>` : nothing}
              ${billing.periodEnd ? html`<span>${status === "trialing" ? "the trial ends" : status === "cancelled" ? "ends" : "renews"} ${day(billing.periodEnd)}</span>` : nothing}
            </div>
            ${billing.subscribed && admin ? this.manage(org, billing) : nothing}
          </section>`
        : nothing}
      <section>
        <h2>Usage</h2>
        ${usageMeters(usage, null)}
      </section>
      ${billing?.enabled ? this.plans(org, billing, admin) : html`<section><p class="hint">This server sells no plans. Over a limit, hitop refuses new workspaces, people, devices or uploads that grow storage. Reading and pulling never stop, and nothing is deleted.</p></section>`}
    `;
  }

  manage(org, billing) {
    const base = `/v1/orgs/${enc(org.id)}/billing`;
    const seatsNow = ["team", "business"].includes(org.plan) && ["active", "on_trial", "past_due"].includes(billing.status);
    return html`
      <p class="hint">Payment method, invoices, monthly or yearly, cancel: in Stripe's billing portal. Stripe is the merchant of record (receipts come from Link, "Sold through Link"), so you can also manage the order at link.com.</p>
      <div class="row">
        <lit-material-button ?disabled=${this.busy || this.leaving} @click=${() => this.go(() => session.api.get(`${base}/portal`))}>Manage billing</lit-material-button>
      </div>
      ${seatsNow
        ? html`<form class="row" @submit=${(e) => this.changeSeats(e, org, billing)}>
            <lit-material-text-field class="seats-field" name="seats" type="number" variant="outlined" label="Seats" min=${Math.max(1, billing.seatsUsed)} max="1000" .value=${String(billing.seats)} required></lit-material-text-field>
            <lit-material-button variant="outlined" type="submit" ?disabled=${this.busy}>Change seats</lit-material-button>
            <p class="hint">Every person in the organization or its workspaces is a seat (viewers too); 5 guest viewers are free. More seats are charged at once, prorated; fewer take effect now, with no credit, and the lower price starts at the next renewal.</p>
          </form>`
        : nothing}
    `;
  }

  plans(org, billing, admin) {
    if (billing.subscribed) return nothing;
    if (!admin) return html`<section><p class="hint">An owner or admin of the organization buys its plan.</p></section>`;
    const ids = org.personal ? ["pro"] : ["team", "business"];
    const offered = (plan) => billing.offers.filter((o) => o.plan === plan);
    const seatsUsed = Math.max(1, billing.seatsUsed || 1);
    const seats = this.seats || seatsUsed;
    const plans = [
      { id: "free", name: "Free", price: { amount: "$0" }, features: PLAN_FEATURES.free, current: (org.plan || "free") === "free", action: "" },
      ...ids
        .filter((plan) => plan !== "business" || offered(plan).length)
        .map((plan) => {
          const offers = offered(plan);
          const price = {};
          if (offers.some((o) => o.period === "monthly")) price.monthly = PRICES[plan].monthly;
          if (offers.some((o) => o.period === "annual")) price.yearly = PRICES[plan].yearly;
          const onSale = offers.length && billing.checkout;
          return {
            id: plan,
            name: PLAN_NAMES[plan],
            price: Object.keys(price).length ? price : { amount: "–" },
            features: PLAN_FEATURES[plan],
            current: org.plan === plan,
            recommended: plan === (org.personal ? "pro" : "team"),
            action: this.leaving ? "Opening checkout…" : `Buy ${PLAN_NAMES[plan]}`,
            disabled: !onSale || this.busy || this.leaving,
            hint: !offers.length ? `${PLAN_NAMES[plan]} isn't on sale on this server yet.` : !billing.checkout ? "Checkout isn't open on this server yet." : plan === "pro" ? "" : `for ${seats} seat${seats === 1 ? "" : "s"}`,
          };
        }),
    ];
    return html`<section aria-label="Plans">
      <h2>Plans</h2>
      ${billing.testMode ? html`<p class="hint">Test mode: purchases here are Stripe test purchases (no real money).</p>` : nothing}
      ${org.personal
        ? html`<p class="hint">Pro is for one person. For a team, create an organization and buy Team there.</p>`
        : html`<form class="row" @submit=${(e) => e.preventDefault()}>
            <lit-material-text-field class="seats-field" name="seats" type="number" variant="outlined" label="Seats" min=${seatsUsed} max="1000" .value=${String(seats)} @input=${(e) => (this.seats = Math.max(seatsUsed, Number(e.target.value) || seatsUsed))}></lit-material-text-field>
            <p class="hint">Every person in the organization or its workspaces takes a seat; 5 guest viewers are free.</p>
          </form>`}
      <lit-material-plan-picker .plans=${plans} period="yearly" period-note="Yearly: two months free" @lit-material-plan-select=${(e) => this.checkout(org, e.detail.plan.id, e.detail.period, seats)}></lit-material-plan-picker>
      <p class="hint notes">Prices in US dollars before tax. The checkout is Stripe's page, "Sold through Link": Stripe is the merchant of record, adds tax where it applies and takes the payment (HITOP never sees the card). <a href="https://hitop.rws8.tech/pricing.html">Pricing</a> · <a href="https://hitop.rws8.tech/pricing.html#refunds">Refunds</a></p>
      ${billing.trial?.available && org.role === "owner"
        ? html`<div class="row">
            <lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.trial(org, billing)}>Start the ${billing.trial.days}-day Team trial</lit-material-button>
            <span class="hint">${billing.trial.seats} seats, no card. Afterwards Free limits apply unless you buy Team; nothing is deleted.</span>
          </div>`
        : nothing}
    </section>`;
  }

  async go(fn) {
    const r = await act(this, fn);
    if (!r.ok) return;
    this.leaving = true;
    location.assign(r.value.url);
  }

  checkout(org, plan, period, seats) {
    if (plan === "free") return;
    const body = { plan, period: period === "yearly" ? "annual" : "monthly" };
    if (plan !== "pro") body.seats = seats;
    return this.go(() => session.api.call("POST", `/v1/orgs/${enc(org.id)}/billing/checkout`, body));
  }

  async changeSeats(event, org) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const seats = Number(new FormData(form).get("seats"));
    const r = await act(this, () => session.api.call("POST", `/v1/orgs/${enc(org.id)}/billing/seats`, { seats }), { success: (v) => `${v.seats} seat(s)` });
    if (r.ok) refreshOrg(org.id);
  }

  async trial(org, billing) {
    const r = await act(this, () => session.api.call("POST", `/v1/orgs/${enc(org.id)}/billing/trial`, {}), { success: `Team trial started: ${billing.trial.days} days, ${billing.trial.seats} seats, no card` });
    if (r.ok) refreshOrg(org.id);
  }
}

customElements.define("plan-page", PlanPage);
