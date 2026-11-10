// U50: who can lift a plan limit that just stopped something (402 `limit`), named from the org's
// member list when this account can read it (any member can), else "an owner of <org>". Used in
// the limit dialog and inline where a dialog is already open (the Share dialog's People, Mock and
// Monitor tabs). For an owner or admin it offers Plan & billing instead.
import { LitElement, html, nothing } from "lit";
import { icon } from "../icons.js";
import { limitFixers } from "../org.js";

export class HitopLimitNote extends LitElement {
  static properties = { limit: { attribute: false }, store: { attribute: false }, client: { attribute: false }, bare: { type: Boolean }, fixers: { state: true } };

  createRenderRoot() {
    return this;
  }

  willUpdate(changed) {
    if (changed.has("limit")) this.load();
  }

  /** The refused org as this account knows it (never from the top bar: a share files under the collection's org). */
  get org() {
    return this.store?.orgs?.find((o) => o.id === this.limit?.org) ?? null;
  }

  async load() {
    const org = this.org;
    const me = this.client?.account?.()?.email ?? "";
    this.fixers = limitFixers({ org, me });
    if (!this.fixers.canFix && this.limit?.org && this.client) {
      const members = await this.client.api("GET", `/v1/orgs/${encodeURIComponent(this.limit.org)}/members`).catch(() => null);
      if (members) this.fixers = limitFixers({ org, members, me });
    }
  }

  render() {
    const f = this.fixers;
    if (!f) return nothing;
    const app = this.closest("hitop-app");
    return html`<div class="limit-note" role="note" aria-label="Who can fix this">
      <p>${icon(f.canFix ? "info" : "users-three")} <span class="limit-fixers">${f.text}</span></p>
      ${f.canFix && app && this.org && !this.bare
        ? html`<lit-material-button variant="outlined" @click=${() => {
            if (this.store.currentOrg !== this.org.id) this.store.setCurrentOrg(this.org.id);
            app.closeDialog();
            app.openOrg("billing");
          }}>Plan &amp; billing</lit-material-button>`
        : nothing}
    </div>`;
  }
}

customElements.define("hitop-limit-note", HitopLimitNote);
