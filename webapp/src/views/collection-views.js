// The collections overview, one collection (requests and its last run), and
// a collection's variables.
import { LitElement, html, nothing } from "lit";
import { guard } from "lit/directives/guard.js";
import { repeat } from "lit/directives/repeat.js";
import { confirm } from "@lit-material/confirm";
import { icon } from "../icons.js";
import { secretNames } from "../features.js";
import { platform } from "../platform/index.js";
import { testList } from "./request-view.js";
import { methodLabel } from "../store.js";
import { syncButton } from "./openapi-dialogs.js";
import { isSyncRef } from "../sync.js";

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
              ${r.tests?.length ? html`<span class="run-tests">${r.tests.filter((x) => x.pass).length} of ${r.tests.length} tests passed</span>` : nothing}
              ${!r.success && !r.tests?.some((x) => !x.pass) ? html`<p class="run-detail">${r.response}</p>` : nothing}
              ${r.tests?.some((x) => !x.pass) ? html`<div class="run-detail-tests">${testList(r.tests)}</div>` : nothing}
            </li>`,
        )}
        ${running ? html`<li class="skeleton row" aria-label="Sending the next request"></li>` : nothing}
      </ol>
    </section>`;
}

export class HitopCollections extends LightElement {
  static properties = { store: { attribute: false }, version: { attribute: false }, app: { attribute: false } };

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
            ${platform().folders?.pick
              ? html`<lit-material-button variant="outlined" @click=${() => s.openFolder()}>${icon("folder-open")} Open folder</lit-material-button>`
              : nothing}
            <lit-material-button variant="outlined" @click=${() => this.querySelector("#zip-input").click()}>${icon("file-zip")} Import zip</lit-material-button>
            <input id="zip-input" type="file" accept=".zip,application/zip" hidden aria-label="Collection zip file"
              @change=${async (e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (file) await s.importZip(file);
              }} />
            <lit-material-button variant="filled" @click=${() => this.app.openDialog("add-collection")}>${icon("plus")} New collection</lit-material-button>
          </div>
        </header>
        <p class="page-lead">A collection folder holds one YAML file per request, for git and for <code>hitop run</code> in CI.
          ${platform().folders?.pick ? "Open one to work on its files directly, or import a .zip of one." : "Import a .zip of one here; to work on a folder directly, use Chrome, Edge or the desktop app."}</p>
        ${summary
          ? html`<lit-material-alert variant=${summary.failedCount ? "warning" : "success"}>
              <span slot="title">${summary.failedCount ? "The run stopped at a failed request" : "Every request succeeded"}</span>
              ${count(summary.totalCollections, "collection")}, ${summary.totalRequests} of ${count(summary.plannedRequests ?? summary.totalRequests, "request")} sent:
              ${summary.successCount} succeeded, ${summary.failedCount} failed${summary.assertions ? `; ${summary.assertions - summary.assertionsFailed} of ${summary.assertions} tests passed` : ""}, in ${summary.duration} ms.
            </lit-material-alert>`
          : nothing}
        ${s.collections.length
          ? html`<lit-material-sortable class="cards" layout="grid" role="list" label="Collections" @sort=${(e) => s.reorderCollections(e.detail.from, e.detail.to)}>
              ${repeat(
                s.collections,
                (c) => c.id,
                (c) => html`
                  <div class="card" role="listitem" data-sort-id=${c.id} data-sort-label=${c.name}>
                    <button type="button" class="drag-handle" data-sort-handle aria-label=${`Reorder ${c.name}`}>${icon("dots-six-vertical")}</button>
                    <h2><button type="button" class="card-link" @click=${() => this.app.go("collection", c.id)}>${c.name}</button></h2>
                    <p>${count(c.requests?.length ?? 0, "request")} · ${count(Object.keys(c.variables || {}).length, "variable")}</p>
                    ${run.running && run.collectionId === c.id ? html`<p class="running">Running…</p>` : nothing}
                  </div>`,
              )}
            </lit-material-sortable>
            <p class="hint">Drag a card by its handle, or focus the handle and press Space, to change the order "Run all" uses.</p>`
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
    const folder = s.folderOf(c.id);
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
            ${syncButton(this.app, c)}
            <lit-material-icon-button aria-label="Rename collection" @click=${() => this.app.openDialog("rename-collection", { id: c.id, name: c.name })}>${icon("pencil-simple")}</lit-material-icon-button>
            <lit-material-icon-button aria-label="Duplicate collection" @click=${() => s.duplicateCollection(c.id)}>${icon("copy")}</lit-material-icon-button>
            <lit-material-icon-button aria-label="Export as zip" @click=${() => s.exportZip(c.id)}>${icon("file-zip")}</lit-material-icon-button>
            <lit-material-icon-button aria-label=${folder ? "Close folder" : "Delete collection"} @click=${() => this.app.deleteCollection(c)}>${icon(folder ? "x" : "trash")}</lit-material-icon-button>
          </div>
        </header>
        ${folder
          ? html`<p class="folder-status" role="status">
              ${icon("folder-open")}
              ${folder.connected
                ? folder.error
                  ? html`<span>Couldn't save to ${folder.label}: ${folder.error}</span>`
                  : html`<span>Saved to the folder ${folder.label} as you edit.</span>
                      ${platform().git && !isSyncRef(folder.ref) ? html`<lit-material-button variant="text" @click=${() => this.app.go("git", c.id)}>Git</lit-material-button>` : nothing}`
                : html`<span>Was opened from the folder ${folder.label}. Reconnect to read it again and keep saving there.</span>
                    <lit-material-button variant="text" @click=${() => s.reconnectFolder(c.id)}>Reconnect folder</lit-material-button>`}
            </p>`
          : platform().folders?.pick
            ? html`<p class="folder-status">
                <lit-material-button variant="text" @click=${() => s.saveToFolder(c.id)}>${icon("folder-open")} Save to folder</lit-material-button>
              </p>`
            : nothing}
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
                        <span class=${`method-tag m-${methodLabel(r)}`}>${methodLabel(r)}</span>
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
  static properties = { store: { attribute: false }, version: { attribute: false }, app: { attribute: false }, reveal: { state: true }, revealGlobals: { state: true } };

  // The globals' two editors, as an environment's: plain values and secrets.
  changeGlobals(e, secret) {
    const editor = e.target;
    if (!editor.valid) return;
    const g = this.store.globals;
    const names = secretNames(g);
    const own = editor.toObject();
    const other = Object.fromEntries(Object.entries(g.variables).filter(([k]) => names.includes(k) !== secret));
    const variables = secret ? { ...other, ...own } : { ...own, ...other };
    const secrets = secret ? Object.keys(own) : names;
    this.globalsJson = `${JSON.stringify(variables)}:${JSON.stringify(secrets)}`;
    this.store.setGlobals(variables, secrets);
  }

  async importGlobals() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        this.store.importGlobals(await file.text());
      } catch (err) {
        this.store.notify(`Couldn't import ${file.name}: ${err.message}`);
      }
    };
    input.click();
  }

  renderGlobals() {
    const s = this.store;
    const g = s.globals;
    const names = secretNames(g);
    const json = `${JSON.stringify(g.variables)}:${JSON.stringify(names)}`;
    if (json !== this.globalsJson) {
      this.globalsJson = json;
      const rows = Object.entries(g.variables).map(([key, value]) => ({ key, value: String(value) }));
      this.globalRows = rows.filter((r) => !names.includes(r.key));
      this.globalSecretRows = rows.filter((r) => names.includes(r.key));
      this.globalsVersion = (this.globalsVersion ?? 0) + 1;
    }
    return html`
      <section class="globals" aria-labelledby="globals-title">
        <header class="section-header">
          <h2 id="globals-title">Globals</h2>
          <div class="page-actions">
            <lit-material-button variant="outlined" @click=${() => this.importGlobals()}>${icon("file-arrow-up")} Import globals</lit-material-button>
            <lit-material-button variant="outlined" ?disabled=${!Object.keys(g.variables).length} @click=${() => this.app.download("hitop-globals.json", s.exportGlobals())}>${icon("file-arrow-down")} Export globals</lit-material-button>
          </div>
        </header>
        <p class="hint">Variables every collection sees. Collection variables and the active environment override them; Postman globals import here.</p>
        <lit-material-key-value-editor
          aria-label="Globals"
          key-label="Name"
          value-label="Value"
          add-label="Add global"
          key-pattern="[^}]+"
          key-pattern-message="A name can't contain }"
          .entries=${guard([this.globalsVersion], () => this.globalRows)}
          @change=${(e) => this.changeGlobals(e, false)}
        ></lit-material-key-value-editor>
        <h3>Secret globals</h3>
        <label class="switch-row">
          <lit-material-switch aria-label="Show secret global values" ?checked=${this.revealGlobals} @change=${(e) => (this.revealGlobals = e.target.checked)}></lit-material-switch>
          <span>Show secret values</span>
        </label>
        <lit-material-key-value-editor
          aria-label="Secret globals"
          key-label="Name"
          value-label="Value"
          add-label="Add a secret global"
          key-pattern="[^}]+"
          key-pattern-message="A name can't contain }"
          .maskValues=${!this.revealGlobals}
          .entries=${guard([this.globalsVersion], () => this.globalSecretRows)}
          @change=${(e) => this.changeGlobals(e, true)}
        ></lit-material-key-value-editor>
      </section>`;
  }

  // Two editors, one environment: plain variables and secrets (masked). Both
  // write `variables`; the secrets editor also sets the `secrets` names.
  change(e, env, secret) {
    const editor = e.target;
    if (!editor.valid) return;
    const names = secretNames(env);
    const own = editor.toObject();
    const other = Object.fromEntries(Object.entries(env.variables).filter(([k]) => names.includes(k) !== secret));
    const variables = secret ? { ...other, ...own } : { ...own, ...other };
    const secrets = secret ? Object.keys(own) : names;
    this.json = `${env.id}:${JSON.stringify(variables)}:${JSON.stringify(secrets)}`;
    this.store.setEnvironmentVariables(env.id, variables, secrets);
  }

  render() {
    const s = this.store;
    const selectedId = s.view.environmentId ?? s.activeEnvironmentId ?? s.environments[0]?.id;
    const env = s.environments.find((e) => e.id === selectedId);
    if (env) {
      // As with collection variables: new rows only when changed elsewhere.
      const names = secretNames(env);
      const json = `${env.id}:${JSON.stringify(env.variables)}:${JSON.stringify(names)}`;
      if (json !== this.json) {
        this.json = json;
        const rows = Object.entries(env.variables).map(([key, value]) => ({ key, value: String(value) }));
        this.rows = rows.filter((r) => !names.includes(r.key));
        this.secretRows = rows.filter((r) => names.includes(r.key));
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
              <div class="env-tabs">
                ${s.environments.map(
                  (e) => html`<button type="button" class=${`env-chip ${e.id === selectedId ? "selected" : ""}`} aria-current=${e.id === selectedId ? "true" : "false"}
                    @click=${() => s.show("environments", null, e.id)}>
                    ${s.environmentLabel(e)}${e.id === s.activeEnvironmentId ? html` <span class="active-tag">active</span>` : nothing}
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
                      @change=${(e) => this.change(e, env, false)}
                    ></lit-material-key-value-editor>
                    <section class="env-secrets" aria-labelledby="env-secrets-title">
                      <h2 id="env-secrets-title">Secrets</h2>
                      <p class="hint">${platform().secretsNote}</p>
                      <label class="switch-row">
                        <lit-material-switch aria-label="Show secret values" ?checked=${this.reveal} @change=${(e) => (this.reveal = e.target.checked)}></lit-material-switch>
                        <span>Show secret values</span>
                      </label>
                      <lit-material-key-value-editor
                        key-label="Name"
                        value-label="Value"
                        add-label="Add secret"
                        key-pattern="[^}]+"
                        key-pattern-message="A name can't contain }"
                        .maskValues=${!this.reveal}
                        .entries=${guard([this.rowsVersion], () => this.secretRows)}
                        @change=${(e) => this.change(e, env, true)}
                      ></lit-material-key-value-editor>
                    </section>`
                : nothing}`
          : html`<lit-material-empty-state headline="No environments yet">
              Create Dev, Staging and Prod with the same variable names and different values, then switch between them in the sidebar.
              <lit-material-button slot="actions" variant="filled" @click=${() => this.app.openDialog("add-environment")}>New environment</lit-material-button>
            </lit-material-empty-state>`}
        ${this.renderGlobals()}
      </div>`;
  }
}

customElements.define("hitop-environments", HitopEnvironments);

export class HitopCookies extends LightElement {
  static properties = { store: { attribute: false }, version: { attribute: false }, app: { attribute: false } };

  edit(cookie, value) {
    this.store.setCookies(this.store.cookies.map((c) => (c === cookie ? { ...c, value } : c)));
  }

  // Not `remove`: that would shadow Element.remove, and Lit could no longer
  // take this view out of the page.
  removeCookie(cookie) {
    this.store.setCookies(this.store.cookies.filter((c) => c !== cookie));
  }

  async clear() {
    if (await confirm({ headline: "Clear cookies?", message: "Requests stop sending them until servers set new ones.", confirmLabel: "Clear", destructive: true })) {
      this.store.setCookies([]);
    }
  }

  add(e) {
    e.preventDefault();
    const form = e.target;
    const get = (name) => form.querySelector(`[name="${name}"]`).value.trim();
    const domain = get("domain").replace(/^\./, "").toLowerCase();
    const name = get("name");
    const mark = (field, message) => {
      const el = form.querySelector(`[name="${field}"]`);
      el.error = Boolean(message);
      el.errorText = message;
    };
    mark("domain", domain ? "" : "Enter the domain the cookie is for");
    mark("name", name ? "" : "Enter the cookie name");
    if (!domain || !name) return;
    const cookie = { name, value: get("value"), domain, hostOnly: false, path: "/", expires: null, secure: false };
    this.store.setCookies([...this.store.cookies.filter((c) => !(c.name === name && c.domain === domain && c.path === "/")), cookie]);
    form.querySelectorAll("lit-material-text-field").forEach((f) => (f.value = ""));
  }

  render() {
    const s = this.store;
    const byDomain = new Map();
    for (const c of s.cookies) byDomain.set(c.domain, [...(byDomain.get(c.domain) ?? []), c]);
    return html`
      <div class="page">
        <header class="page-header">
          <h1>Cookies</h1>
          <div class="page-actions">
            <lit-material-button variant="outlined" ?disabled=${!s.cookies.length} @click=${() => this.clear()}>${icon("trash")} Clear cookies</lit-material-button>
          </div>
        </header>
        <label class="switch-row">
          <lit-material-switch aria-label="Keep and send cookies" ?checked=${s.cookieJarEnabled} @change=${(e) => s.setCookieJarEnabled(e.target.checked)}></lit-material-switch>
          <span>Keep cookies from responses and send them with later requests to the same site</span>
        </label>
        <p class="page-lead">${platform().cookiesNote}</p>
        ${byDomain.size
          ? [...byDomain].map(
              ([domain, cookies]) => html`
                <section class="cookie-domain" aria-label=${domain}>
                  <h2>${domain}</h2>
                  <ul class="cookie-list">
                    ${cookies.map(
                      (c) => html`<li>
                        <span class="cookie-name">${c.name}</span>
                        <lit-material-text-field variant="outlined" label="Value" .value=${c.value} @change=${(e) => this.edit(c, e.target.value)}></lit-material-text-field>
                        <span class="cookie-meta">Path ${c.path}${c.secure ? " · HTTPS only" : ""} · ${c.expires ? `expires ${new Date(c.expires).toLocaleString()}` : "kept until you close the browser"}</span>
                        <lit-material-icon-button aria-label=${`Delete ${c.name}`} @click=${() => this.removeCookie(c)}>${icon("trash")}</lit-material-icon-button>
                      </li>`,
                    )}
                  </ul>
                </section>`,
            )
          : html`<lit-material-empty-state size="compact" headline="No cookies yet">Cookies that responses set appear here, or add one below.</lit-material-empty-state>`}
        <form class="cookie-add" novalidate @submit=${(e) => this.add(e)}>
          <lit-material-text-field name="domain" variant="outlined" label="Domain" placeholder="api.example.com"></lit-material-text-field>
          <lit-material-text-field name="name" variant="outlined" label="Name"></lit-material-text-field>
          <lit-material-text-field name="value" variant="outlined" label="Value"></lit-material-text-field>
          <lit-material-button variant="text" type="submit">${icon("plus")} Add cookie</lit-material-button>
        </form>
      </div>`;
  }
}

customElements.define("hitop-cookies", HitopCookies);
