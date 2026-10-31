// The collections overview, one collection (requests and its last run), and
// a collection's variables.
import { LitElement, html, nothing } from "lit";
import { guard } from "lit/directives/guard.js";
import { icon } from "../icons.js";
import { testList } from "./request-view.js";

class LightElement extends LitElement {
  createRenderRoot() {
    return this;
  }
}

const count = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;

function runResults(results, running) {
  if (!results.length && !running) return nothing;
  return html`
    <section class="run" aria-label="Run results" aria-live="polite">
      <h2>${running ? "Running" : "Last run"}</h2>
      <ol class="run-list">
        ${results.map(
          (r) => html`
            <li>
              <span class=${`status-chip ${r.success ? "ok" : "bad"}`}>${r.statusCode}</span>
              <span class="method">${r.method}</span>
              <span class="run-name">${r.requestName}</span>
              ${r.duration != null ? html`<span class="run-time">${r.duration} ms</span>` : nothing}
              ${r.tests?.length ? html`<span class="run-tests">${r.tests.filter((x) => x.pass).length}/${r.tests.length} tests</span>` : nothing}
              ${!r.success ? html`<p class="run-detail">${r.response}</p>` : nothing}
              ${r.tests?.some((x) => !x.pass) ? html`<div class="run-detail-tests">${testList(r.tests)}</div>` : nothing}
            </li>`,
        )}
        ${running ? html`<li class="skeleton row" aria-label="Sending the next request"></li>` : nothing}
      </ol>
    </section>`;
}

export class HitopCollections extends LightElement {
  static properties = { store: { attribute: false }, version: { attribute: false }, app: { attribute: false } };

  drop(e, index) {
    e.preventDefault();
    const from = Number(e.dataTransfer.getData("text/plain"));
    if (!Number.isNaN(from)) this.store.reorderCollections(from, index);
  }

  render() {
    const s = this.store;
    const run = s.run;
    const summary = run.summary;
    return html`
      <div class="page">
        <header class="page-header">
          <h1>Collections</h1>
          <div class="page-actions">
            <lit-material-button variant="outlined" ?disabled=${!s.collections.length || run.running} @click=${() => s.runAll()}>
              ${icon("play")} ${run.running ? `Running ${run.completed + 1} of ${s.collections.filter((c) => c.requests?.length).length}` : "Run all"}
            </lit-material-button>
            <lit-material-button variant="filled" @click=${() => this.app.openDialog("add-collection")}>${icon("plus")} New collection</lit-material-button>
          </div>
        </header>
        ${summary
          ? html`<lit-material-alert variant=${summary.failedCount ? "warning" : "success"}>
              <span slot="title">${summary.failedCount ? "The run stopped at a failed request" : "Every request succeeded"}</span>
              ${count(summary.totalCollections, "collection")}, ${count(summary.totalRequests, "request")}: ${summary.successCount} succeeded,
              ${summary.failedCount} failed${summary.assertions ? `; ${summary.assertions - summary.assertionsFailed} of ${summary.assertions} tests passed` : ""}, in ${summary.duration} ms.
            </lit-material-alert>`
          : nothing}
        ${s.collections.length
          ? html`<ul class="cards">
              ${s.collections.map(
                (c, i) => html`
                  <li
                    class="card"
                    draggable="true"
                    @dragstart=${(e) => e.dataTransfer.setData("text/plain", String(i))}
                    @dragover=${(e) => e.preventDefault()}
                    @drop=${(e) => this.drop(e, i)}
                  >
                    <span class="drag-handle" aria-hidden="true">${icon("dots-six-vertical")}</span>
                    <h2><button type="button" class="card-link" @click=${() => this.app.go("collection", c.id)}>${c.name}</button></h2>
                    <p>${count(c.requests?.length ?? 0, "request")} · ${count(Object.keys(c.variables || {}).length, "variable")}</p>
                    ${run.running && run.collectionId === c.id ? html`<p class="running">Running…</p>` : nothing}
                  </li>`,
              )}
            </ul>
            <p class="hint">Drag cards to change the order "Run all" uses.</p>`
          : html`<lit-material-empty-state headline="No collections yet">
              Group related requests in a collection to run them together and share variables.
              <lit-material-button slot="actions" variant="filled" @click=${() => this.app.openDialog("add-collection")}>New collection</lit-material-button>
            </lit-material-empty-state>`}
      </div>`;
  }
}

export class HitopCollection extends LightElement {
  static properties = { store: { attribute: false }, version: { attribute: false }, app: { attribute: false }, collection: { attribute: false } };

  render() {
    const s = this.store;
    const c = this.collection;
    const run = s.run;
    const mine = run.collectionId === c.id;
    const n = c.requests?.length ?? 0;
    return html`
      <div class="page">
        <button type="button" class="back" @click=${() => this.app.go("collections")}>${icon("arrow-left")} Collections</button>
        <header class="page-header">
          <h1>${c.name}</h1>
          <div class="page-actions">
            <lit-material-button variant="filled" ?disabled=${!n || run.running} @click=${() => s.runCollection(c.id)}>
              ${icon("play")} ${mine && run.running ? "Running" : "Run collection"}
            </lit-material-button>
            <lit-material-button variant="outlined" @click=${() => this.app.go("variables", c.id)}>${icon("sliders-horizontal")} Variables</lit-material-button>
            <lit-material-icon-button aria-label="Rename collection" @click=${() => this.app.openDialog("rename-collection", { id: c.id, name: c.name })}>${icon("pencil-simple")}</lit-material-icon-button>
            <lit-material-icon-button aria-label="Duplicate collection" @click=${() => s.duplicateCollection(c.id)}>${icon("copy")}</lit-material-icon-button>
            <lit-material-icon-button aria-label="Delete collection" @click=${() => this.app.deleteCollection(c)}>${icon("trash")}</lit-material-icon-button>
          </div>
        </header>
        <p class="page-lead">Run sends the requests in this order, passes values saved with <code>setContext</code> to the next one, and stops at the first failure.</p>
        ${mine ? runResults(run.results, run.running) : nothing}
        <section aria-label="Requests">
          <h2>${count(n, "request")}</h2>
          ${n
            ? html`<ol class="request-list">
                ${c.requests.map(
                  (r) => html`
                    <li>
                      <button type="button" class="request-row" @click=${() => s.openRequest(r, c.id)}>
                        <span class=${`method-tag m-${r.method}`}>${r.method}</span>
                        <span class="request-name">${r.name}</span>
                        <span class="request-url">${r.url}</span>
                      </button>
                      <lit-material-icon-button aria-label=${`Delete ${r.name}`} @click=${() => this.app.deleteRequest(r, c.id)}>${icon("trash")}</lit-material-icon-button>
                    </li>`,
                )}
              </ol>`
            : html`<lit-material-empty-state size="compact" headline="No requests in this collection">
                Send a request, then save it here with the save button next to Send.
              </lit-material-empty-state>`}
        </section>
      </div>`;
  }
}

export class HitopVariables extends LightElement {
  static properties = { store: { attribute: false }, version: { attribute: false }, collection: { attribute: false } };

  change(e) {
    const editor = e.target;
    if (!editor.valid) return;
    const variables = editor.toObject();
    this.variablesJson = `${this.collection.id}:${JSON.stringify(variables)}`;
    this.store.setVariables(this.collection.id, variables);
  }

  render() {
    const c = this.collection;
    // Rows go to the editor only when the variables changed outside it
    // (another collection, an import): setting `entries` rebuilds its rows.
    const json = `${c.id}:${JSON.stringify(c.variables || {})}`;
    if (json !== this.variablesJson) {
      this.variablesJson = json;
      this.rows = Object.entries(c.variables || {}).map(([key, value]) => ({ key, value: String(value) }));
      this.rowsVersion = (this.rowsVersion ?? 0) + 1;
    }
    return html`
      <div class="page">
        <button type="button" class="back" @click=${() => this.store.show("collection", c.id)}>${icon("arrow-left")} ${c.name}</button>
        <header class="page-header"><h1>Variables</h1></header>
        <p class="page-lead">Use them as <code>\${name}</code> in the URL, headers and body of any request in ${c.name}, and as <code>getVariable('name')</code> in scripts. Changes save as you type.</p>
        <lit-material-key-value-editor
          key-label="Name"
          value-label="Value"
          add-label="Add variable"
          key-pattern="[^}]+"
          key-pattern-message="A name can't contain }"
          .entries=${guard([this.rowsVersion], () => this.rows)}
          @change=${(e) => this.change(e)}
        ></lit-material-key-value-editor>
      </div>`;
  }
}

customElements.define("hitop-collections", HitopCollections);
customElements.define("hitop-collection", HitopCollection);
customElements.define("hitop-variables", HitopVariables);

export class HitopEnvironments extends LightElement {
  static properties = { store: { attribute: false }, version: { attribute: false }, app: { attribute: false } };

  change(e, env) {
    const editor = e.target;
    if (!editor.valid) return;
    const variables = editor.toObject();
    this.json = `${env.id}:${JSON.stringify(variables)}`;
    this.store.setEnvironmentVariables(env.id, variables);
  }

  render() {
    const s = this.store;
    const selectedId = s.view.environmentId ?? s.activeEnvironmentId ?? s.environments[0]?.id;
    const env = s.environments.find((e) => e.id === selectedId);
    if (env) {
      // As with collection variables: new rows only when changed elsewhere.
      const json = `${env.id}:${JSON.stringify(env.variables)}`;
      if (json !== this.json) {
        this.json = json;
        this.rows = Object.entries(env.variables).map(([key, value]) => ({ key, value: String(value) }));
        this.rowsVersion = (this.rowsVersion ?? 0) + 1;
      }
    }
    return html`
      <div class="page">
        <header class="page-header">
          <h1>Environments</h1>
          <div class="page-actions">
            <lit-material-button variant="outlined" @click=${() => this.app.openDialog("import-environments")}>${icon("file-arrow-up")} Import</lit-material-button>
            <lit-material-button variant="outlined" ?disabled=${!s.environments.length} @click=${() => this.app.download("hitop-environments.json", s.exportEnvironments())}>${icon("file-arrow-down")} Export</lit-material-button>
            <lit-material-button variant="filled" @click=${() => this.app.openDialog("add-environment")}>${icon("plus")} New environment</lit-material-button>
          </div>
        </header>
        <p class="page-lead">An environment is a set of variables such as Dev, Staging or Prod. The active one, picked in the sidebar, overrides collection variables with the same name in every request.</p>
        ${s.environments.length
          ? html`
              <div class="env-tabs" role="list">
                ${s.environments.map(
                  (e) => html`<button type="button" role="listitem" class=${`env-chip ${e.id === selectedId ? "selected" : ""}`} aria-current=${e.id === selectedId ? "true" : "false"}
                    @click=${() => s.show("environments", null, e.id)}>
                    ${e.name}${e.id === s.activeEnvironmentId ? html` <span class="active-tag">active</span>` : nothing}
                  </button>`,
                )}
              </div>
              ${env
                ? html`
                    <div class="page-actions">
                      ${env.id === s.activeEnvironmentId
                        ? html`<lit-material-button variant="tonal" @click=${() => s.setActiveEnvironment(null)}>Deactivate</lit-material-button>`
                        : html`<lit-material-button variant="tonal" @click=${() => s.setActiveEnvironment(env.id)}>Use this environment</lit-material-button>`}
                      <lit-material-icon-button aria-label=${`Rename ${env.name}`} @click=${() => this.app.openDialog("rename-environment", { id: env.id, name: env.name })}>${icon("pencil-simple")}</lit-material-icon-button>
                      <lit-material-icon-button aria-label=${`Delete ${env.name}`} @click=${() => this.app.deleteEnvironment(env)}>${icon("trash")}</lit-material-icon-button>
                    </div>
                    <lit-material-key-value-editor
                      key-label="Name"
                      value-label="Value"
                      add-label="Add variable"
                      key-pattern="[^}]+"
                      key-pattern-message="A name can't contain }"
                      .entries=${guard([this.rowsVersion], () => this.rows)}
                      @change=${(e) => this.change(e, env)}
                    ></lit-material-key-value-editor>`
                : nothing}`
          : html`<lit-material-empty-state headline="No environments yet">
              Create Dev, Staging and Prod with the same variable names and different values, then switch between them in the sidebar.
              <lit-material-button slot="actions" variant="filled" @click=${() => this.app.openDialog("add-environment")}>New environment</lit-material-button>
            </lit-material-empty-state>`}
      </div>`;
  }
}

customElements.define("hitop-environments", HitopEnvironments);
