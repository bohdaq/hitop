// The Share dialog's Mock tab: the collection's mock server on HITOP Cloud (sync-server
// mocks.rs), answering from its requests' saved examples. Signed in, collection in the library.
import { LitElement, html, nothing } from "lit";
import { icon } from "../icons.js";
import { limitInfo } from "../org.js";
import "./limit-note.js";

class HitopMock extends LitElement {
  static properties = { store: { attribute: false }, collection: { attribute: false }, mock: { state: true }, busy: { state: true }, error: { state: true }, limit: { state: true } };

  createRenderRoot() {
    return this;
  }

  get api() {
    return this.store.library.client;
  }

  get path() {
    return `/v1/library/collection/${encodeURIComponent(this.collection.id)}/mock`;
  }

  connectedCallback() {
    super.connectedCallback();
    this.load();
  }

  async load() {
    try {
      const r = await this.api.call("GET", this.path, undefined, { allow: [404] });
      this.mock = r.status === 404 ? null : r.json;
    } catch (e) {
      this.error = e.message || String(e);
    }
  }

  async act(method) {
    this.busy = true;
    this.error = null;
    try {
      if (method === "PUT") await this.store.library.flush();
      const r = await this.api.call(method, this.path, undefined, { allow: [402] });
      if (r.status === 402) {
        this.error = `${r.json?.message ?? "Mock servers need a paid plan."}`;
        this.limit = limitInfo({ status: 402, body: r.json, message: r.json?.message });
      }
      else this.mock = method === "PUT" ? r.json : null;
    } catch (e) {
      this.error = e.message || String(e);
    } finally {
      this.busy = false;
    }
  }

  render() {
    const c = this.collection;
    const withExamples = (c.requests || []).filter((r) => r.examples?.length).length;
    const first = (c.requests || []).find((r) => r.examples?.length);
    const path = first ? (first.url || "").replace(/^\$\{[^}]+\}|^[a-z]+:\/\/[^/]+/i, "").split("?")[0] || "/" : "/";
    return html`<div class="share-file">
      <p>A public URL that answers like your API, from the examples saved on the requests (${withExamples} of ${(c.requests || []).length} have one), so a frontend can be built before the API is ready.</p>
      ${this.error ? html`<p class="error-text" role="alert">${this.error}</p>` : nothing}
      ${this.error && this.limit ? html`<hitop-limit-note .limit=${this.limit} .store=${this.store} .client=${this.api}></hitop-limit-note>` : nothing}
      ${this.mock
        ? html`<lit-material-code-block label="Mock URL" copyable>${this.mock.url}</lit-material-code-block>
            <p class="hint">Try <code>${first?.method ?? "GET"} ${this.mock.url}${path}</code>. Add <code>?example=&lt;name&gt;</code> to pick another saved example. Changes to the collection apply at once.</p>
            <lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.act("DELETE")}>Turn off</lit-material-button>`
        : this.mock === null
          ? html`<lit-material-button variant="filled" ?disabled=${this.busy} @click=${() => this.act("PUT")}>${icon("play")} Turn on the mock server</lit-material-button>
              ${withExamples ? nothing : html`<p class="hint">No request has a saved example yet: send one and choose Save as example.</p>`}`
          : html`<div class="skeleton block"></div>`}
    </div>`;
  }
}

customElements.define("hitop-mock", HitopMock);
