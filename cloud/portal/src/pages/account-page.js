// The account: who you are, how you sign in (password, passkeys), your devices.
import "@lit-material/text-field";
import "@lit-material/button";
import "@lit-material/list";
import "@lit-material/alert";
import "@lit-material/skeleton";
import "@lit-material/status";
import { TaskController, taskCache } from "@lit-material/task";
import { LitElement, css, html, nothing } from "lit";
import { changePassword, registerPasskey } from "../api.js";
import { act, describeError, errorAlert, loadError, sessionEnded } from "../actions.js";
import { notify } from "../notify.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

export class AccountPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      .card {
        max-width: 36rem;
      }
      ul {
        margin: 0;
        padding: 0;
        list-style: none;
      }
      li {
        padding: var(--space-75) 0;
        border-top: 1px solid var(--md-sys-color-outline-variant);
      }
      li:first-child {
        border-top: 0;
      }
    `,
  ];

  static properties = { busy: { state: true }, error: { state: true }, passkeyError: { state: true } };

  constructor() {
    super();
    this.busy = false;
    this.error = null;
    this.passkeyError = null;
    this.me = new TaskController(this, {
      task: (_a, signal) => session.api.get("/v1/me", { signal }),
      args: () => [session.account.session],
      key: ([t]) => ["me", t],
    });
  }

  render() {
    return html`
      <div class="title-row"><div><h1>Account</h1><p class="lead">${session.email}${session.hosted ? "" : ` on ${session.server}`}</p></div></div>
      ${this.me.render({
        pending: () => html`${[1, 2, 3].map(() => html`<lit-material-skeleton height="40px"></lit-material-skeleton>`)}`,
        error: loadError,
        complete: (me) => this.body(me),
      })}
    `;
  }

  body(me) {
    const hasPassword = Boolean(me.password?.set);
    return html`
      <section class="card">
        <h2>Password</h2>
        <p class="hint" aria-label="Password status">${hasPassword
          ? `Your account has a password.${me.bundles?.password ? " It also unlocks your device's keys on a new device; changing it here re-seals them." : ""}`
          : "No password yet. Add one to sign in with it; sign in with it in hitop once, and it also unlocks your encrypted workspaces on a new device."}</p>
        ${errorAlert(this.error)}
        <form class="grid" @submit=${(e) => this.password(e, me)}>
          ${hasPassword ? html`<lit-material-text-field name="current" variant="outlined" label="Current password" type="password" autocomplete="current-password" required></lit-material-text-field>` : nothing}
          <lit-material-text-field name="next" variant="outlined" label="New password" type="password" autocomplete="new-password" required minlength="8"></lit-material-text-field>
          <lit-material-text-field name="next2" variant="outlined" label="Repeat the new password" type="password" autocomplete="new-password" required></lit-material-text-field>
          <div class="actions"><lit-material-button variant=${hasPassword ? "outlined" : "filled"} type="submit" ?disabled=${this.busy}>${hasPassword ? "Change password" : "Set password"}</lit-material-button></div>
        </form>
      </section>
      <section class="card">
        <h2>Passkeys</h2>
        <p class="hint" aria-label="Passkey count">${me.passkeys.length} passkey(s). A passkey signs you in here and in hitop with your device's lock screen; it is not a workspace key.</p>
        ${errorAlert(this.passkeyError)}
        ${me.passkeys.length ? html`<ul aria-label="Passkeys">${me.passkeys.map((p) => html`<li>${p.name}</li>`)}</ul>` : nothing}
        <form class="row" @submit=${this.addPasskey}>
          <lit-material-text-field name="name" variant="outlined" label="Passkey name" placeholder="laptop"></lit-material-text-field>
          <lit-material-button variant="outlined" type="submit" ?disabled=${this.busy}>Add a passkey</lit-material-button>
        </form>
      </section>
      <section class="card">
        <h2>Devices</h2>
        <p class="hint">${me.keys.length} device key(s): the hitop installs that hold your workspace keys. See when each was last used, rename or remove one in the HITOP app (Account → Devices) or with <code>hitop sync devices</code>; the recovery phrase is set there too.</p>
        ${me.keys.length ? html`<ul aria-label="Devices">${me.keys.map((k) => html`<li>${k.name || k.id}</li>`)}</ul>` : nothing}
      </section>
    `;
  }

  async password(event, me) {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const next = String(data.get("next") || "");
    if (next !== String(data.get("next2") || "")) {
      this.error = { message: "The two new passwords differ" };
      return;
    }
    const r = await act(this, () => changePassword(session.api, me, String(data.get("current") || ""), next), {
      success: me.password?.set ? "Password changed. Your other devices were signed out; they sign in with the new one." : "Password set",
    });
    if (r.ok) {
      form.reset();
      taskCache.invalidate(["me"]);
    }
  }

  addPasskey = async (event) => {
    event.preventDefault();
    const form = event.target;
    const name = String(new FormData(form).get("name") || "").trim() || "portal";
    this.passkeyError = null;
    this.busy = true;
    try {
      await registerPasskey(session.api, name);
      form.reset();
      taskCache.invalidate(["me"]);
      notify("Passkey added: sign in with it next time");
    } catch (e) {
      if (!sessionEnded(e)) this.passkeyError = describeError(e);
    } finally {
      this.busy = false;
    }
  };
}

customElements.define("account-page", AccountPage);
