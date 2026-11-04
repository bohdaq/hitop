// Organizations: every organization you are in, and a new one. A plan, its seats and limits belong
// to an organization; the personal one holds the workspaces shared without naming one.
import "@lit-material/data-table";
import "@lit-material/dialog";
import "@lit-material/text-field";
import "@lit-material/button";
import "@lit-material/skeleton";
import "@lit-material/status";
import { navigate, RouteController } from "@lit-material/router";
import { TaskController, taskCache } from "@lit-material/task";
import { LitElement, css, html } from "lit";
import { act, errorAlert, loadError } from "../actions.js";
import { orgName, planLabel, statusKind } from "../format.js";
import { enc } from "../org-data.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

export class OrgsPage extends LitElement {
  static styles = [pageStyles, css``];

  static properties = { busy: { state: true }, error: { state: true } };

  constructor() {
    super();
    this.busy = false;
    this.error = null;
    this.route = new RouteController(this, () => []);
    this.orgs = new TaskController(this, {
      task: (_a, signal) => session.api.get("/v1/orgs", { signal }).then((r) => r.orgs),
      args: () => [session.account.session],
      key: ([t]) => ["orgs", t],
    });
  }

  firstUpdated() {
    if (new URLSearchParams(location.search).get("new")) this.openDialog();
  }

  openDialog() {
    this.error = null;
    this.renderRoot.querySelector("lit-material-dialog").show();
  }

  render() {
    return html`
      <div class="title-row">
        <div><h1>Organizations</h1><p class="lead">A plan, its seats and limits belong to an organization. Your personal one holds the workspaces you share without naming an organization (<code>hitop sync init --org &lt;name&gt;</code> picks another).</p></div>
        <lit-material-button @click=${this.openDialog}>New organization</lit-material-button>
      </div>
      ${this.orgs.render({
        pending: () => html`${[1, 2].map(() => html`<lit-material-skeleton height="44px"></lit-material-skeleton>`)}`,
        error: loadError,
        complete: (orgs) => this.table(orgs),
      })}
      <lit-material-dialog @close=${() => (this.error = null)}>
        <span slot="headline">New organization</span>
        <form id="org-form" class="dialog-form" @submit=${this.create}>
          ${errorAlert(this.error)}
          <lit-material-text-field name="name" variant="outlined" label="Organization name" required autocomplete="organization"></lit-material-text-field>
          <p class="hint">You own it. Invite people on its Members page; buy seats on its Plan page.</p>
        </form>
        <lit-material-button slot="actions" variant="text" @click=${() => this.renderRoot.querySelector("lit-material-dialog").close()}>Cancel</lit-material-button>
        <lit-material-button slot="actions" type="submit" form="org-form" ?disabled=${this.busy}>${this.busy ? "Creating…" : "Create"}</lit-material-button>
      </lit-material-dialog>
    `;
  }

  table(orgs) {
    return html`<div class="scroll"><lit-material-data-table aria-label="Organizations">
      <lit-material-data-table-row header>
        <lit-material-data-table-cell header flex>Name</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Plan</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Your role</lit-material-data-table-cell>
        <lit-material-data-table-cell header numeric>Members</lit-material-data-table-cell>
        <lit-material-data-table-cell header numeric>Workspaces</lit-material-data-table-cell>
      </lit-material-data-table-row>
      ${orgs.map(
        (o) => html`<lit-material-data-table-row data-row-id=${o.id}>
          <lit-material-data-table-cell flex><a href="/orgs/${enc(o.id)}">${orgName(o)}</a></lit-material-data-table-cell>
          <lit-material-data-table-cell><lit-material-status status=${o.plan && o.plan !== "free" ? statusKind(o.status) : "neutral"}>${planLabel(o.plan, o.status)}</lit-material-status></lit-material-data-table-cell>
          <lit-material-data-table-cell>${o.role}</lit-material-data-table-cell>
          <lit-material-data-table-cell numeric class="mono">${o.members}</lit-material-data-table-cell>
          <lit-material-data-table-cell numeric class="mono">${o.workspaces}</lit-material-data-table-cell>
        </lit-material-data-table-row>`,
      )}
    </lit-material-data-table></div>`;
  }

  create = async (event) => {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const name = String(new FormData(form).get("name") || "").trim();
    const r = await act(this, () => session.api.call("POST", "/v1/orgs", { name }), { success: `${name} created` });
    if (!r.ok) return;
    this.renderRoot.querySelector("lit-material-dialog").close();
    form.reset();
    taskCache.invalidate(["orgs"]);
    session.setOrg(r.value.id);
    navigate(`/orgs/${enc(r.value.id)}`);
  };
}

customElements.define("orgs-page", OrgsPage);
