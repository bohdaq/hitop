// Workspaces: every workspace you are in, with your role, its organization and its last run.
import "@lit-material/data-table";
import "@lit-material/status";
import "@lit-material/timestamp";
import "@lit-material/skeleton";
import "@lit-material/empty-state";
import "@lit-material/clipboard";
import { TaskController } from "@lit-material/task";
import { LitElement, css, html, nothing } from "lit";
import { loadError } from "../actions.js";
import { iso, orgName, shortId, statusKind } from "../format.js";
import { enc } from "../org-data.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

export class WorkspacesPage extends LitElement {
  static styles = [pageStyles, css``];

  constructor() {
    super();
    this.workspaces = new TaskController(this, {
      task: async (_a, signal) => {
        const api = session.api;
        const { workspaces } = await api.get("/v1/workspaces", { signal });
        const last = await Promise.all(workspaces.slice(0, 20).map((w) => api.get(`/v1/workspaces/${enc(w.id)}/runs?limit=1`, { signal }).then((r) => r.runs?.[0] || null, () => null)));
        return workspaces.map((w, i) => ({ ...w, last: last[i] || null }));
      },
      args: () => [session.account.session],
      key: ([t]) => ["workspaces-last", t],
    });
  }

  render() {
    return html`
      <div class="title-row">
        <div><h1>Workspaces</h1><p class="lead">A shared collection is a workspace: end-to-end encrypted, so this page shows who is in it and its run status rows, never its contents. Runs your CI uploads (<code>hitop run --sync-report</code>) are listed per workspace.</p></div>
      </div>
      ${this.workspaces.render({
        pending: () => html`${[1, 2, 3].map(() => html`<lit-material-skeleton height="44px"></lit-material-skeleton>`)}`,
        error: loadError,
        complete: (list) => (list.length ? this.table(list) : this.empty()),
      })}
    `;
  }

  empty() {
    return html`<lit-material-empty-state headline="No workspaces yet">
      Share a collection from hitop first: sign in in its folder, then share it. It shows up here within seconds.
      <lit-material-clipboard-copy slot="actions" label="Share command" value=${"hitop sync login && hitop sync init"}></lit-material-clipboard-copy>
    </lit-material-empty-state>`;
  }

  table(list) {
    return html`<div class="scroll"><lit-material-data-table aria-label="Workspaces">
      <lit-material-data-table-row header>
        <lit-material-data-table-cell header flex>Workspace</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Your role</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Organization</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Last run</lit-material-data-table-cell>
      </lit-material-data-table-row>
      ${list.map(
        (w) => html`<lit-material-data-table-row data-row-id=${w.id}>
          <lit-material-data-table-cell flex><a class="mono" href="/workspaces/${enc(w.id)}/runs">${shortId(w.id)}</a>${w.rotationRequested ? html` <lit-material-status status="warning">rotation requested</lit-material-status>` : nothing}</lit-material-data-table-cell>
          <lit-material-data-table-cell>${w.role}</lit-material-data-table-cell>
          <lit-material-data-table-cell>${w.org ? orgName(w.org) : ""}</lit-material-data-table-cell>
          <lit-material-data-table-cell>
            ${w.last
              ? html`<lit-material-status status=${statusKind(w.last.status)}>${w.last.status}</lit-material-status> <lit-material-timestamp date=${iso(w.last.createdAt)} relative></lit-material-timestamp>`
              : html`<span class="hint">no runs</span>`}
          </lit-material-data-table-cell>
        </lit-material-data-table-row>`,
      )}
    </lit-material-data-table></div>`;
  }
}

customElements.define("workspaces-page", WorkspacesPage);
