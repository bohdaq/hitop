// The Share dialog's Monitor tab: HITOP Cloud runs the collection on a schedule (sync-server
// monitors.rs) and mails when it starts failing and when it passes again. Library collections.
import { LitElement, html, nothing } from "lit";
import { icon } from "../icons.js";

const EVERY = [
  [5, "Every 5 minutes"],
  [15, "Every 15 minutes"],
  [30, "Every 30 minutes"],
  [60, "Every hour"],
  [360, "Every 6 hours"],
  [1440, "Every day"],
];

class HitopMonitor extends LitElement {
  static properties = { store: { attribute: false }, collection: { attribute: false }, monitor: { state: true }, busy: { state: true }, error: { state: true }, note: { state: true } };

  createRenderRoot() {
    return this;
  }

  get api() {
    return this.store.library.client;
  }

  get path() {
    return `/v1/library/collection/${encodeURIComponent(this.collection.id)}/monitor`;
  }

  connectedCallback() {
    super.connectedCallback();
    this.load();
  }

  async load() {
    try {
      const r = await this.api.call("GET", this.path, undefined, { allow: [404] });
      this.monitor = r.status === 404 ? null : r.json;
    } catch (e) {
      this.error = e.message || String(e);
    }
  }

  async save() {
    const env = this.querySelector("#monitor-env")?.value ?? "";
    const every = Number(this.querySelector("#monitor-every")?.value || 60);
    const emails = String(this.querySelector("#monitor-emails")?.value ?? "").split(/[\s,]+/).filter(Boolean);
    await this.act("PUT", { environment: env === "none" ? "" : env, everyMinutes: every, ...(emails.length ? { emails } : {}) });
    if (!this.error) this.note = "Saved. The first run starts within a minute.";
  }

  async act(method, body, path = this.path) {
    this.busy = true;
    this.error = null;
    this.note = null;
    try {
      if (method === "PUT") await this.store.library.flush();
      const r = await this.api.call(method, path, body, { allow: [402, 429] });
      if (r.status === 402 || r.status === 429) this.error = r.json?.message ?? "Not now.";
      else if (path.endsWith("/run")) this.note = "Queued: it runs within a minute.";
      else this.monitor = method === "DELETE" ? null : r.json;
    } catch (e) {
      this.error = e.message || String(e);
    } finally {
      this.busy = false;
    }
  }

  render() {
    const m = this.monitor;
    const envs = this.store.visibleEnvironments.filter((e) => !e.team && !this.store.folderEnvs.has(e.id));
    const when = (t) => new Date(t * 1000).toLocaleString();
    const account = this.api.account();
    return html`<div class="share-file">
      <p>HITOP Cloud runs this collection on a schedule with an environment of yours and mails when it starts failing and when it passes again. Secret values are used from your library; what it keeps of a run (names, statuses, failed tests) hides them.</p>
      ${this.error ? html`<p class="error-text" role="alert">${this.error}</p>` : nothing}
      ${this.note ? html`<p class="sync-note" role="status">${this.note}</p>` : nothing}
      ${m === undefined
        ? html`<div class="skeleton block"></div>`
        : html`<div class="sync-form">
              <lit-material-select id="monitor-env" variant="outlined" label="Environment" .value=${m?.environment || "none"}>
                <lit-material-select-option value="none">No environment</lit-material-select-option>
                ${envs.map((e) => html`<lit-material-select-option value=${e.name}>${e.name}</lit-material-select-option>`)}
              </lit-material-select>
              <lit-material-select id="monitor-every" variant="outlined" label="Runs" .value=${String(m?.everyMinutes ?? 60)}>
                ${EVERY.map(([v, l]) => html`<lit-material-select-option value=${String(v)}>${l}</lit-material-select-option>`)}
              </lit-material-select>
              <lit-material-text-field id="monitor-emails" variant="outlined" label="Alert emails (comma separated)" .value=${(m?.emails ?? [account?.email ?? ""]).join(", ")}></lit-material-text-field>
            </div>
            <div class="page-actions">
              <lit-material-button variant="filled" ?disabled=${this.busy} @click=${() => this.save()}>${icon("play")} ${m ? "Save" : "Turn on the monitor"}</lit-material-button>
              ${m
                ? html`<lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.act("POST", undefined, `${this.path}/run`)}>Run now</lit-material-button>
                    <lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.act("DELETE")}>Turn off</lit-material-button>`
                : nothing}
            </div>
            ${m?.runs?.length
              ? html`<ul class="sync-list" aria-label="Monitor runs">
                  ${m.runs.slice(0, 10).map((r) => html`<li class="sync-row">
                    <span><span class=${`status-chip ${r.passed ? "ok" : "bad"}`}>${r.passed ? "Passed" : "Failed"}</span> ${when(r.at)}</span>
                    <span class="hint">${r.requests - r.failed}/${r.requests} requests · ${r.durationMs} ms${r.failures?.length ? ` · ${r.failures.map((f) => `${f.name}${f.status ? ` (${f.status})` : ""}${f.error ? `: ${f.error}` : ""}`).join("; ")}` : ""}</span>
                  </li>`)}
                </ul>`
              : m
                ? html`<p class="hint">No runs yet. Next run: ${when(m.nextAt)}.</p>`
                : nothing}`}
    </div>`;
  }
}

customElements.define("hitop-monitor", HitopMonitor);
