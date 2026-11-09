// The Run report (HITOP Next A6, concept screen 2): stat tiles, a timeline of the run's requests on
// one time axis with All / Failed / Slow filters, the first failure with expected vs actual, the
// `hitop run` line that reproduces the run, and JUnit / JSON downloads in `hitop run`'s shapes.
import { LitElement, html, nothing } from "lit";
import { styleMap } from "lit/directives/style-map.js";
import { icon } from "../icons.js";
import { platform } from "../platform/index.js";
import { testList } from "./request-view.js";
import { junitXml, slowThreshold, timeline } from "../run-report.js";

const FILTERS = [
  ["all", "All"],
  ["failed", "Failed"],
  ["slow", "Slow"],
];

const ms = (n) => (n == null ? "—" : `${Math.round(n)} ms`);

export class HitopRunReport extends LitElement {
  static properties = { run: { attribute: false }, store: { attribute: false }, filter: { state: true } };

  constructor() {
    super();
    this.filter = "all";
  }

  createRenderRoot() {
    return this;
  }

  async copyCommand(text) {
    try {
      await navigator.clipboard.writeText(text);
      this.store?.notify("Copied the hitop run command");
    } catch {
      this.store?.notify("Couldn't copy: select the command and copy it");
    }
  }

  async download(kind) {
    const { report } = this.run.report;
    const base = report.collections[0].name.replace(/[^\w.-]+/g, "-").toLowerCase() || "run";
    if (kind === "junit") await platform().saveFile(`${base}-junit.xml`, junitXml(report), "application/xml");
    else await platform().saveFile(`${base}-report.json`, `${JSON.stringify(report, null, 2)}\n`, "application/json");
  }

  tiles(stats) {
    const delta = stats.p95Delta;
    const deltaText = delta == null ? "first run here" : delta === 0 ? "same as last run" : `${delta > 0 ? "+" : "−"}${Math.abs(delta)} ms vs last run`;
    return html`<div class="run-tiles" role="list" aria-label="Run summary">
      <div class="run-tile" role="listitem"><span class="run-tile-label">Requests</span><span class="run-tile-value tabular">${stats.requests.passed}<span class="run-tile-of">/${stats.requests.total}</span></span>
        <span class=${`run-tile-note ${stats.requests.passed < stats.requests.total ? "bad" : ""}`}>${stats.requests.total - stats.requests.passed ? `${stats.requests.total - stats.requests.passed} failed` : "all passed"}</span></div>
      <div class="run-tile" role="listitem"><span class="run-tile-label">Checks</span><span class="run-tile-value tabular">${stats.checks.passed}<span class="run-tile-of">/${stats.checks.total}</span></span>
        <span class="run-tile-note">${stats.checks.total ? `${Math.round((stats.checks.passed / stats.checks.total) * 100)}% pass` : "no checks"}</span></div>
      <div class="run-tile" role="listitem"><span class="run-tile-label">p95 latency</span><span class="run-tile-value tabular">${ms(stats.p95)}</span>
        <span class=${`run-tile-note ${delta > 0 ? "warn" : delta < 0 ? "good" : ""}`}>${deltaText}</span></div>
      <div class="run-tile" role="listitem"><span class="run-tile-label">Wall time</span><span class="run-tile-value tabular">${(stats.wallMs / 1000).toFixed(2)} s</span>
        <span class="run-tile-note">one request at a time</span></div>
    </div>`;
  }

  failure(results) {
    const f = results.find((r) => !r.success);
    if (!f) return nothing;
    const failedChecks = (f.tests || []).filter((t) => !t.pass);
    const responded = typeof f.statusCode === "number";
    return html`<section class="run-failure" aria-label="First failure">
      <p class="run-failure-where"><span class=${`method-tag m-${f.method}`}>${f.method}</span> ${f.file ? html`<code>${f.file}</code>` : nothing}</p>
      <h3>${f.requestName}</h3>
      ${!responded
        ? html`<p>Not answered: ${f.response}</p>`
        : failedChecks.length
          ? html`<div class="run-failure-grid">
              <div><span class="run-tile-label">Expected</span><ul>${failedChecks.map((t) => html`<li>${t.name}</li>`)}</ul></div>
              <div><span class="run-tile-label">Actual</span><ul>${failedChecks.map((t) => html`<li>${t.detail || "failed"}</li>`)}</ul><p class="hint">HTTP ${f.statusCode}</p></div>
            </div>`
          : html`<div class="run-failure-grid">
              <div><span class="run-tile-label">Expected</span><p>a 2xx status</p></div>
              <div><span class="run-tile-label">Actual</span><p>HTTP ${f.statusCode}</p></div>
            </div>`}
    </section>`;
  }

  render() {
    const run = this.run;
    const results = run.results || [];
    if (!results.length && !run.running) return nothing;
    const slow = slowThreshold(results);
    const rows = timeline(results).filter((r) => (this.filter === "failed" ? !r.success : this.filter === "slow" ? r.ms >= slow : true));
    const done = !run.running && run.report;
    return html`<section class="run run-report" aria-label="Run report" aria-live="polite">
      <header class="run-report-head">
        <h2>${run.running ? "Running" : "Last run"}</h2>
        ${done
          ? html`<span class=${`status-chip ${run.report.report.passed ? "ok" : "bad"}`}>${run.report.report.passed ? "Passed" : `${run.report.report.summary.requests.failed} failed`}</span>
              <span class="run-report-actions">
                <lit-material-button variant="outlined" @click=${() => this.download("junit")}>${icon("file-arrow-down")} JUnit</lit-material-button>
                <lit-material-button variant="outlined" @click=${() => this.download("json")}>${icon("file-arrow-down")} report.json</lit-material-button>
              </span>`
          : nothing}
      </header>
      ${done ? this.tiles(run.report.stats) : nothing}
      <div class="run-report-body">
        <div class="run-timeline">
          <div class="run-filter" role="radiogroup" aria-label="Show">
            ${FILTERS.map(([id, label]) => {
              const n = id === "all" ? results.length : id === "failed" ? results.filter((r) => !r.success).length : results.filter((r) => (r.duration ?? 0) >= slow).length;
              return html`<button type="button" role="radio" aria-checked=${String(this.filter === id)} @click=${() => (this.filter = id)}>${label} <span class="tabular">${n}</span></button>`;
            })}
          </div>
          <ol class="run-list" aria-label="Requests in run order">
            ${rows.map(
              (r) => html`<li class=${`run-row ${r.success ? "" : "failed"}`}>
                <span class=${`status-chip ${r.success ? "ok" : "bad"}`}>${r.statusCode}</span>
                <span class=${`method-tag m-${r.method}`}>${r.method}</span>
                <span class="run-name">${r.requestName}</span>
                <span class="run-time tabular">${ms(r.duration)}</span>
                <span class="run-bar" aria-hidden="true"><span style=${styleMap({ left: `${r.left.toFixed(2)}%`, width: `${r.width.toFixed(2)}%` })} class=${r.success ? (r.ms >= slow ? "slow" : "") : "bad"}></span></span>
                ${r.tests?.length ? html`<span class="run-tests">${r.tests.filter((x) => x.pass).length} of ${r.tests.length} tests passed</span>` : nothing}
                ${!r.success && !r.tests?.some((x) => !x.pass) ? html`<p class="run-detail">${r.response}</p>` : nothing}
                ${r.tests?.some((x) => !x.pass) ? html`<div class="run-detail-tests">${testList(r.tests)}</div>` : nothing}
              </li>`,
            )}
            ${run.running ? html`<li class="skeleton row" aria-label="Sending the next request"></li>` : nothing}
            ${!rows.length && !run.running ? html`<li class="hint">Nothing ${this.filter === "failed" ? "failed" : "was slow"} in this run.</li>` : nothing}
          </ol>
        </div>
        ${done
          ? html`<aside class="run-side">
              ${this.failure(results)}
              <section class="run-cli" aria-label="Same run from your terminal">
                <h3>Same run from your terminal</h3>
                <code class="run-command">${run.report.command}</code>
                <lit-material-button variant="outlined" @click=${() => this.copyCommand(run.report.command)}>${icon("copy")} Copy</lit-material-button>
                <p class="hint">Exit code ${run.report.report.passed ? 0 : 1}; <code>reports/junit.xml</code> annotates failing files in a pull request.</p>
              </section>
            </aside>`
          : nothing}
      </div>
    </section>`;
  }
}

customElements.define("hitop-run-report", HitopRunReport);
