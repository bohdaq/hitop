// Home: the getting-started checklist until it's done, then what matters at a glance: the
// organizations and workspaces, failing checks and the pass rate of recent runs.
import "@lit-material/stepper/checklist.js";
import "@lit-material/stat";
import "@lit-material/status";
import "@lit-material/clipboard";
import "@lit-material/chart";
import "@lit-material/button";
import "@lit-material/skeleton";
import "@lit-material/empty-state";
import { TaskController } from "@lit-material/task";
import { LitElement, css, html, nothing } from "lit";
import { loadError } from "../actions.js";
import { bytes, orgName, shortId } from "../format.js";
import { enc } from "../org-data.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

export class HomePage extends LitElement {
  static styles = [
    pageStyles,
    css`
      .stats {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: var(--space-200);
      }
      lit-material-checklist {
        margin-bottom: var(--space-400);
      }
      lit-material-checklist p {
        margin: 0 0 var(--space-100);
      }
      .failing li {
        margin: var(--space-50) 0;
      }
      ul {
        margin: 0;
        padding-inline-start: var(--space-300);
      }
    `,
  ];

  constructor() {
    super();
    this.orgs = new TaskController(this, {
      task: (_a, signal) => session.api.get("/v1/orgs", { signal }).then((r) => r.orgs),
      args: () => [session.account.session],
      key: ([t]) => ["orgs", t],
    });
    // Polls while the first workspace is awaited (it appears within seconds of `hitop sync init`).
    this.workspaces = new TaskController(this, {
      task: (_a, signal) => session.api.get("/v1/workspaces", { signal }).then((r) => r.workspaces),
      args: () => [session.account.session],
      key: ([t]) => ["workspaces", t],
      refetchInterval: 15_000,
      refetchOnFocus: true,
    });
    // Recent runs of up to six workspaces: failing checks and the pass rate.
    this.runs = new TaskController(this, {
      // The ids as one string: a fresh array every render would re-run the task without end.
      task: async ([ids], signal) => {
        const api = session.api;
        const all = await Promise.all((ids ? ids.split(",") : []).map((ws) => api.get(`/v1/workspaces/${enc(ws)}/runs?limit=30`, { signal }).then((r) => ({ ws, ...r }), () => null)));
        return all.filter(Boolean);
      },
      args: () => [(this.workspaces.value || []).slice(0, 6).map((w) => w.id).join(",")],
      key: ([ids]) => ["home-runs", ids],
    });
  }

  get currentOrg() {
    const orgs = this.orgs.value || [];
    return orgs.find((o) => o.id === session.orgId) || orgs.find((o) => o.personal) || orgs[0] || null;
  }

  render() {
    return html`
      <div class="title-row"><div><h1>Home</h1><p class="lead">${session.email}${session.hosted ? "" : ` on ${session.server}`}</p></div></div>
      ${this.orgs.render({
        pending: () => html`${[1, 2].map(() => html`<lit-material-skeleton height="56px"></lit-material-skeleton>`)}`,
        error: loadError,
        complete: (orgs) => this.body(orgs),
      })}
    `;
  }

  body(orgs) {
    const workspaces = this.workspaces.value || [];
    const org = this.currentOrg;
    const team = orgs.filter((o) => !o.personal);
    const invited = team.some((o) => o.members > 1);
    const paid = orgs.some((o) => o.plan && o.plan !== "free");
    const membersHref = team.length ? `/orgs/${enc((team.find((o) => o.id === session.orgId) || team[0]).id)}/members` : "/orgs?new=1";
    const runs = this.runs.value || [];
    const series = runs.flatMap((r) => (r.series || []).map((s) => ({ ...s, ws: r.ws })));
    const failing = series.filter((s) => s.status === "failed");
    const recent = runs
      .flatMap((r) => (r.runs || []).filter((x) => x.status !== "sealed" && x.requests > 0).map((x) => ({ at: x.createdAt, rate: (x.requests - x.failed) / x.requests })))
      .sort((a, b) => a.at - b.at)
      .slice(-30);
    const rate = recent.length ? Math.round((recent.reduce((s, r) => s + r.rate, 0) / recent.length) * 100) : null;
    return html`
      <lit-material-checklist heading="Get started" dismissible storage-key="hitop-cloud-getting-started">
        <lit-material-checklist-item label="Open the web app" ?done=${workspaces.length > 0}>
          <p>Requests, collections and environments run in your browser, nothing to install. The desktop app and the CLI open the same collections.</p>
          <p><lit-material-button href="https://hitop.rws8.tech/app/" rel="noopener">Open the web app</lit-material-button></p>
          <p class="hint">Prefer a terminal or CI? <a href="https://hitop.rws8.tech/" rel="noopener">Downloads</a>, or:</p>
          <lit-material-clipboard-copy label="Install command" value="curl -fsSL https://hitop.rws8.tech/install.sh | sh"></lit-material-clipboard-copy>
        </lit-material-checklist-item>
        <lit-material-checklist-item label="Share a workspace" ?done=${workspaces.length > 0}>
          <p>In the web app, open a collection and share it from its Sync page (sign in with this account). It is end-to-end encrypted and shows up here within seconds. From a terminal, in the collection's folder:</p>
          <lit-material-clipboard-copy label="Share command" value=${"hitop sync login && hitop sync init"}></lit-material-clipboard-copy>
        </lit-material-checklist-item>
        <lit-material-checklist-item label="Invite a teammate" ?done=${invited}>
          <p>${team.length ? "People you invite join the organization and take a seat; workspaces are still shared from hitop." : "A plan, its seats and limits belong to an organization. Create one for your team first."}</p>
          <lit-material-button href=${membersHref}>${team.length ? "Invite people" : "New organization"}</lit-material-button>
        </lit-material-checklist-item>
        <lit-material-checklist-item label="Choose a plan" ?done=${paid}>
          <p>Free works for one person and small shares. Teams need seats.</p>
          <lit-material-button variant="outlined" href=${org ? `/orgs/${enc(org.id)}/plan` : "/orgs"}>See plans</lit-material-button>
        </lit-material-checklist-item>
      </lit-material-checklist>
      <section>
        <h2>At a glance</h2>
        <div class="stats">
          <lit-material-stat label="Organizations" value=${String(orgs.length)} caption=${org ? `current: ${orgName(org)}` : ""}></lit-material-stat>
          <lit-material-stat label="Workspaces" value=${String(workspaces.length)} caption=${workspaces.length ? `${workspaces.filter((w) => w.role === "owner").length} owned by you` : "share one from hitop"}></lit-material-stat>
          <lit-material-stat label="Failing checks" value=${String(failing.length)} caption=${series.length ? `of ${series.length} collections` : "no runs uploaded yet"}></lit-material-stat>
          <lit-material-stat label="Pass rate" value=${rate === null ? "–" : `${rate}%`} caption=${recent.length ? `last ${recent.length} runs` : "no runs yet"}>
            ${recent.length > 1 ? html`<lit-material-sparkline slot="caption" label="Pass rate of recent runs" .values=${recent.map((r) => Math.round(r.rate * 100))}></lit-material-sparkline>` : nothing}
          </lit-material-stat>
        </div>
      </section>
      ${failing.length
        ? html`<section class="failing">
            <h2>Failing now</h2>
            <ul>
              ${failing.map((s) => html`<li><lit-material-status status="error">${s.collection || "collection"}${s.environment ? ` (${s.environment})` : ""}</lit-material-status> in <a href="/workspaces/${enc(s.ws)}/runs">${shortId(s.ws)}</a></li>`)}
            </ul>
          </section>`
        : nothing}
      ${org && this.usage(org)}
    `;
  }

  usage(org) {
    return html`<section>
      <h2>${orgName(org)}</h2>
      <p class="hint">Plan <strong>${org.plan || "free"}</strong>, ${org.members} ${org.members === 1 ? "person" : "people"}, ${org.workspaces} ${org.workspaces === 1 ? "workspace" : "workspaces"}${org.storageBytes ? `, ${bytes(org.storageBytes)}` : ""}. <a href="/orgs/${enc(org.id)}">Usage and limits</a></p>
    </section>`;
  }
}

customElements.define("home-page", HomePage);
