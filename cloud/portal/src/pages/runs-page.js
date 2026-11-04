// Runs of one workspace (HITOP Cloud C4): the pass rate over time, the run list, one run's
// status row, and the alert settings (owners). Reports are sealed end to end: they open in hitop.
import "@lit-material/tabs";
import "@lit-material/data-table";
import "@lit-material/status";
import "@lit-material/timestamp";
import "@lit-material/skeleton";
import "@lit-material/empty-state";
import "@lit-material/chart";
import "@lit-material/switch";
import "@lit-material/text-field";
import "@lit-material/button";
import "@lit-material/alert";
import { TaskController, taskCache } from "@lit-material/task";
import { LitElement, css, html, nothing } from "lit";
import { act, errorAlert, loadError } from "../actions.js";
import { iso, shortId, statusKind } from "../format.js";
import { enc } from "../org-data.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

export class RunsPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      lit-material-tabs {
        margin-bottom: var(--space-300);
      }
      .series {
        display: flex;
        flex-wrap: wrap;
        gap: var(--space-75) var(--space-200);
        margin: 0 0 var(--space-300);
      }
      .chart {
        max-width: 480px;
        margin-bottom: var(--space-300);
      }
      .run {
        margin-bottom: var(--space-300);
      }
      .failed-row {
        color: var(--md-sys-color-error);
      }
    `,
  ];

  static properties = {
    ws: {},
    run: {},
    tab: { state: true },
    busy: { state: true },
    error: { state: true },
  };

  constructor() {
    super();
    this.tab = 0;
    this.busy = false;
    this.error = null;
    this.data = new TaskController(this, {
      task: async ([ws], signal) => {
        const api = session.api;
        const [runs, info] = await Promise.all([api.get(`/v1/workspaces/${enc(ws)}/runs?limit=200`, { signal }), api.get(`/v1/workspaces/${enc(ws)}`, { signal })]);
        const owner = info.role === "owner";
        const alerts = owner ? await api.get(`/v1/workspaces/${enc(ws)}/alerts`, { signal }).catch(() => null) : null;
        const deliveries = owner ? (await api.get(`/v1/workspaces/${enc(ws)}/alert-deliveries`, { signal }).catch(() => ({ deliveries: [] }))).deliveries : [];
        return { runs, info, alerts, deliveries };
      },
      args: () => [this.ws],
      key: ([ws]) => ["runs", ws],
    });
  }

  render() {
    return html`
      <div class="title-row">
        <div><h1>Runs of <span class="mono">${shortId(this.ws)}</span></h1><p class="lead">Status rows of the runs your CI and cron upload. The full reports are sealed end to end: open one in hitop (<code>hitop sync runs --id &lt;run&gt;</code>) or on the app's Sync page. HITOP never runs your collections on its machines.</p></div>
      </div>
      ${this.data.render({
        pending: () => html`${[1, 2, 3].map(() => html`<lit-material-skeleton height="44px"></lit-material-skeleton>`)}`,
        error: loadError,
        complete: (d) => this.body(d),
      })}
    `;
  }

  body({ runs, info, alerts, deliveries }) {
    const owner = info.role === "owner";
    return html`
      <p class="hint">Kept ${runs.historyDays ? `${runs.historyDays} days` : "without a time limit"}${runs.sealedHistory ? "" : html`; this plan keeps <strong>status rows only</strong> (sealed reports from Pro on${info.org ? html`, <a href="/orgs/${enc(info.org.id || info.org)}/plan">see the plan</a>` : nothing})`}. You are ${info.role}.</p>
      ${owner
        ? html`<lit-material-tabs aria-label="Workspace sections" .selected=${this.tab} @change=${(e) => (this.tab = e.target.selected)}>
            <lit-material-tab>Runs</lit-material-tab>
            <lit-material-tab>Alerts</lit-material-tab>
          </lit-material-tabs>`
        : nothing}
      ${this.tab === 1 && owner ? this.alerts(alerts, deliveries) : this.runs(runs)}
    `;
  }

  runs(runs) {
    const one = this.run ? runs.runs.find((r) => r.id === this.run) : null;
    const rates = [...runs.runs]
      .filter((r) => r.status !== "sealed" && r.requests > 0)
      .sort((a, b) => a.createdAt - b.createdAt)
      .slice(-40)
      .map((r) => Math.round(((r.requests - r.failed) / r.requests) * 100));
    return html`
      ${runs.series.length
        ? html`<div class="series" aria-label="Last status">
            ${runs.series.map((x) => html`<lit-material-status status=${statusKind(x.status)}>${x.collection || "collection"}${x.environment ? ` (${x.environment})` : ""}: ${x.status}</lit-material-status>`)}
          </div>`
        : nothing}
      ${rates.length > 1 ? html`<div class="chart"><lit-material-sparkline label="Pass rate of the last ${rates.length} runs, percent" .values=${rates} show-last></lit-material-sparkline></div>` : nothing}
      ${one
        ? html`<div class="card run" aria-label="Run ${one.id}">
            <h2>Run <span class="mono">${one.id}</span></h2>
            <p><lit-material-status status=${statusKind(one.status)}>${one.status}</lit-material-status> at <lit-material-timestamp date=${iso(one.createdAt)} time-format="short"></lit-material-timestamp>, ${one.requests - one.failed}/${one.requests} requests passed, ${one.durationMs} ms${one.runner ? ", from a runner credential" : ""}.</p>
            ${one.failedNames?.length ? html`<p>Failed: ${one.failedNames.join(", ")}</p>` : nothing}
            <p class="hint">${one.hasReport ? html`The full report is sealed: open it in hitop with <code>hitop sync runs --id ${one.id}</code> or on the app's Sync page (Run history).` : "This run has a status row only."}</p>
          </div>`
        : nothing}
      ${runs.runs.length
        ? html`<div class="scroll"><lit-material-data-table aria-label="Runs">
            <lit-material-data-table-row header>
              <lit-material-data-table-cell header flex>Time</lit-material-data-table-cell>
              <lit-material-data-table-cell header>Status</lit-material-data-table-cell>
              <lit-material-data-table-cell header>Collection</lit-material-data-table-cell>
              <lit-material-data-table-cell header numeric>Passed</lit-material-data-table-cell>
              <lit-material-data-table-cell header numeric>Duration</lit-material-data-table-cell>
              <lit-material-data-table-cell header>Report</lit-material-data-table-cell>
            </lit-material-data-table-row>
            ${runs.runs.map(
              (r) => html`<lit-material-data-table-row data-row-id=${r.id} class=${r.status === "failed" ? "failed-row" : ""}>
                <lit-material-data-table-cell flex><a href="/workspaces/${enc(this.ws)}/runs/${enc(r.id)}"><lit-material-timestamp date=${iso(r.createdAt)} time-format="short"></lit-material-timestamp></a></lit-material-data-table-cell>
                <lit-material-data-table-cell><lit-material-status status=${statusKind(r.status)}>${r.status}</lit-material-status></lit-material-data-table-cell>
                <lit-material-data-table-cell>${r.collection}${r.environment ? ` (${r.environment})` : ""}</lit-material-data-table-cell>
                <lit-material-data-table-cell numeric class="mono">${r.status === "sealed" ? "–" : `${r.requests - r.failed}/${r.requests}`}</lit-material-data-table-cell>
                <lit-material-data-table-cell numeric class="mono">${r.status === "sealed" ? "–" : `${r.durationMs} ms`}</lit-material-data-table-cell>
                <lit-material-data-table-cell>${r.hasReport ? "sealed" : "status only"}</lit-material-data-table-cell>
              </lit-material-data-table-row>`,
            )}
          </lit-material-data-table></div>`
        : html`<lit-material-empty-state headline="No runs yet">Upload one from your CI: <code>hitop run --sync-report</code>, or schedule it with <code>hitop schedule --sync</code>.</lit-material-empty-state>`}
    `;
  }

  alerts(a, deliveries) {
    if (!a) return html`<p class="hint">Alerts are set by the workspace's owners.</p>`;
    const s = a.settings;
    const channels = a.plan?.alertChannels ?? ["email"];
    const urls = (kind) => s.webhooks.filter((w) => w.kind === kind).map((w) => w.url).join("\n");
    return html`
      <section>
        <h2>Alerts</h2>
        <p class="hint">When a collection and environment change status (failed, passes again) or miss a run: the status and a link, never a response, body or secret value. This plan: ${channels.join(", ")}.</p>
        ${errorAlert(this.error)}
        <form class="grid" @submit=${(e) => this.saveAlerts(e, channels)}>
          <lit-material-text-field name="email" variant="outlined" label="Mail to members (comma separated)" .value=${s.email.join(", ")}></lit-material-text-field>
          ${channels.includes("webhook") ? html`<lit-material-text-field name="webhook" variant="outlined" label="Webhook URLs (https, one per line)" .value=${urls("webhook")}></lit-material-text-field>` : nothing}
          ${channels.includes("slack") ? html`<lit-material-text-field name="slack" variant="outlined" label="Slack incoming webhook URLs" .value=${urls("slack")}></lit-material-text-field>` : nothing}
          <lit-material-text-field name="missed" variant="outlined" type="number" min="0" label="Missed-run alert after minutes (0 = off)" .value=${String(s.missedAfterMinutes ?? 0)}></lit-material-text-field>
          <lit-material-switch name="failedNames" label="Name failed requests in status rows and alerts" aria-label="Name failed requests in status rows and alerts" ?checked=${Boolean(s.failedNames)}></lit-material-switch>
          <lit-material-switch name="statusRows" label="Keep status rows" aria-label="Keep status rows" supporting-text="Off: history only, no alerts" ?checked=${s.statusRows !== false}></lit-material-switch>
          <div class="actions"><lit-material-button type="submit" ?disabled=${this.busy}>Save alerts</lit-material-button></div>
        </form>
      </section>
      ${deliveries.length
        ? html`<section>
            <h2>Alerts sent</h2>
            <div class="scroll"><lit-material-data-table aria-label="Alerts sent">
              <lit-material-data-table-row header>
                <lit-material-data-table-cell header flex>Time</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Event</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Channel</lit-material-data-table-cell>
                <lit-material-data-table-cell header>Outcome</lit-material-data-table-cell>
              </lit-material-data-table-row>
              ${deliveries.map(
                (d) => html`<lit-material-data-table-row>
                  <lit-material-data-table-cell flex><lit-material-timestamp date=${iso(d.createdAt)} time-format="short"></lit-material-timestamp></lit-material-data-table-cell>
                  <lit-material-data-table-cell>${d.event}</lit-material-data-table-cell>
                  <lit-material-data-table-cell>${d.channel.split(":")[0]}</lit-material-data-table-cell>
                  <lit-material-data-table-cell><lit-material-status status=${d.outcome === "sent" ? "ok" : statusKind(d.outcome)}>${d.outcome}</lit-material-status></lit-material-data-table-cell>
                </lit-material-data-table-row>`,
              )}
            </lit-material-data-table></div>
          </section>`
        : nothing}
    `;
  }

  async saveAlerts(event, channels) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const lines = (name) => String(data.get(name) || "").split(/[\n,]/).map((x) => x.trim()).filter(Boolean);
    const r = await act(
      this,
      () =>
        session.api.call("PUT", `/v1/workspaces/${enc(this.ws)}/alerts`, {
          email: lines("email"),
          webhooks: [...(channels.includes("webhook") ? lines("webhook") : []).map((url) => ({ kind: "webhook", url })), ...(channels.includes("slack") ? lines("slack") : []).map((url) => ({ kind: "slack", url }))],
          missedAfterMinutes: Number(data.get("missed") || 0),
          failedNames: data.get("failedNames") !== null,
          statusRows: data.get("statusRows") !== null,
        }),
      { success: "Alert settings saved" },
    );
    if (r.ok) taskCache.invalidate(["runs", this.ws]);
  }
}

customElements.define("runs-page", RunsPage);
