// Personal access tokens: a create dialog (name, scope, expiry), the new token shown once with a
// copy button, the list, revoke with a confirm. A token is not a key: it can't decrypt anything.
import "@lit-material/data-table";
import "@lit-material/dialog";
import "@lit-material/text-field";
import "@lit-material/select";
import "@lit-material/segmented-button";
import "@lit-material/button";
import "@lit-material/clipboard";
import "@lit-material/timestamp";
import "@lit-material/status";
import "@lit-material/skeleton";
import "@lit-material/empty-state";
import { confirm } from "@lit-material/confirm";
import { TaskController, taskCache } from "@lit-material/task";
import { LitElement, css, html, nothing } from "lit";
import { act, describeError, errorAlert, loadError, sessionEnded } from "../actions.js";
import { notify } from "../notify.js";
import { iso } from "../format.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

export class TokensPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      lit-material-new-secret {
        margin-bottom: var(--space-300);
      }
      .scope {
        display: grid;
        gap: var(--space-50);
      }
    `,
  ];

  static properties = { busy: { state: true }, error: { state: true }, dialogError: { state: true }, newToken: { state: true }, write: { state: true } };

  constructor() {
    super();
    this.busy = false;
    this.error = null;
    this.dialogError = null;
    this.newToken = null;
    this.write = false;
    this.tokens = new TaskController(this, {
      task: (_a, signal) => session.api.get("/v1/me/tokens", { signal }).then((r) => r.tokens),
      args: () => [session.account.session],
      key: ([t]) => ["tokens", t],
    });
  }

  get dialog() {
    return this.renderRoot.querySelector("lit-material-dialog");
  }

  render() {
    return html`
      <div class="title-row">
        <div><h1>Access tokens</h1><p class="lead">Personal access tokens call the sync API as you (scripts, integrations). Read tokens only read. A token is not a key: it can't decrypt anything.</p></div>
        <lit-material-button @click=${this.openDialog}>Create token</lit-material-button>
      </div>
      ${errorAlert(this.error)}
      ${this.newToken
        ? html`<lit-material-new-secret label=${`Access token "${this.newToken.name}"`} value=${this.newToken.token}>
            <lit-material-button slot="actions" variant="text" @click=${() => (this.newToken = null)}>Done</lit-material-button>
          </lit-material-new-secret>`
        : nothing}
      ${this.tokens.render({
        pending: () => html`${[1, 2].map(() => html`<lit-material-skeleton height="44px"></lit-material-skeleton>`)}`,
        error: loadError,
        complete: (tokens) => (tokens.length ? this.table(tokens) : this.empty()),
      })}
      <lit-material-dialog @close=${() => (this.dialogError = null)}>
        <span slot="headline">Create a token</span>
        <form id="token-form" class="dialog-form" @submit=${this.create}>
          ${errorAlert(this.dialogError)}
          <lit-material-text-field name="name" variant="outlined" label="Token name" required placeholder="ci"></lit-material-text-field>
          <div class="scope">
            <span class="hint" id="scope-label">Scope</span>
            <lit-material-segmented-button-group aria-labelledby="scope-label" @change=${(e) => (this.write = [...e.currentTarget.querySelectorAll("lit-material-segmented-button")].find((b) => b.selected)?.dataset.scope === "write")}>
              <lit-material-segmented-button data-scope="read" ?selected=${!this.write}>Read</lit-material-segmented-button>
              <lit-material-segmented-button data-scope="write" ?selected=${this.write}>Read and write</lit-material-segmented-button>
            </lit-material-segmented-button-group>
          </div>
          <lit-material-select name="days" variant="outlined" label="Expires in" value="90">
            ${[30, 90, 365].map((d) => html`<lit-material-select-option value=${String(d)}>${d} days</lit-material-select-option>`)}
          </lit-material-select>
        </form>
        <lit-material-button slot="actions" variant="text" @click=${() => this.dialog.close()}>Cancel</lit-material-button>
        <lit-material-button slot="actions" type="submit" form="token-form" ?disabled=${this.busy}>${this.busy ? "Creating…" : "Create"}</lit-material-button>
      </lit-material-dialog>
    `;
  }

  empty() {
    return html`<lit-material-empty-state headline="No tokens yet">
      A token lets a script or an integration call the sync API as you. Create one, copy it once, and revoke it here when it's done.
      <lit-material-button slot="actions" @click=${this.openDialog}>Create token</lit-material-button>
    </lit-material-empty-state>`;
  }

  table(tokens) {
    return html`<div class="scroll"><lit-material-data-table aria-label="Tokens">
      <lit-material-data-table-row header>
        <lit-material-data-table-cell header flex>Name</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Scopes</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Created</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Expires</lit-material-data-table-cell>
        <lit-material-data-table-cell header>Last used</lit-material-data-table-cell>
        <lit-material-data-table-cell header><span class="sr-only">Actions</span></lit-material-data-table-cell>
      </lit-material-data-table-row>
      ${tokens.map(
        (t) => html`<lit-material-data-table-row data-row-id=${t.id}>
          <lit-material-data-table-cell flex>${t.name}</lit-material-data-table-cell>
          <lit-material-data-table-cell>${t.scopes.join(", ")}</lit-material-data-table-cell>
          <lit-material-data-table-cell><lit-material-timestamp date=${iso(t.createdAt)}></lit-material-timestamp></lit-material-data-table-cell>
          <lit-material-data-table-cell>${t.expired ? html`<lit-material-status status="error">expired</lit-material-status>` : html`<lit-material-timestamp date=${iso(t.expiresAt)}></lit-material-timestamp>`}</lit-material-data-table-cell>
          <lit-material-data-table-cell>${t.lastUsedAt ? html`<lit-material-timestamp date=${iso(t.lastUsedAt)} relative></lit-material-timestamp>` : "never"}</lit-material-data-table-cell>
          <lit-material-data-table-cell><lit-material-button variant="text" ?disabled=${this.busy} @click=${() => this.revoke(t)}>Revoke</lit-material-button></lit-material-data-table-cell>
        </lit-material-data-table-row>`,
      )}
    </lit-material-data-table></div>`;
  }

  openDialog = () => {
    this.dialogError = null;
    this.dialog.show();
  };

  create = async (event) => {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const name = String(data.get("name") || "").trim();
    const scopes = this.write ? ["read", "write"] : ["read"];
    const expiresInDays = Number(data.get("days") || 90);
    this.dialogError = null;
    this.busy = true;
    try {
      const t = await session.api.call("POST", "/v1/me/tokens", { name, scopes, expiresInDays });
      this.dialog.close();
      form.reset();
      this.write = false;
      this.newToken = { name, token: t.token };
      taskCache.invalidate(["tokens"]);
    } catch (e) {
      if (!sessionEnded(e)) this.dialogError = describeError(e);
    } finally {
      this.busy = false;
    }
  };

  async revoke(t) {
    const ok = await confirm({ headline: `Revoke ${t.name}?`, message: "Anything using it stops working at once.", confirmLabel: "Revoke", destructive: true });
    if (!ok) return;
    const r = await act(this, () => session.api.call("DELETE", `/v1/me/tokens/${enc(t.id)}`), { success: `Revoked ${t.name}` });
    if (r.ok) {
      if (this.newToken?.name === t.name) this.newToken = null;
      taskCache.invalidate(["tokens"]);
    }
  }
}

const enc = encodeURIComponent;

customElements.define("tokens-page", TokensPage);
