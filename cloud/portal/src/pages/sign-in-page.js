// Sign in: one screen for a password, a mailed code, creating an account, a reset, Google, GitHub,
// single sign-on and passkeys, all through the sync server. The form is lit-material's; the
// mailed code is pasted (or the whole link) into the portal's own step, since the server mails a
// long one-time token.
import "@lit-material/sign-in";
import "@lit-material/button";
import "@lit-material/text-field";
import "@lit-material/alert";
import { navigate } from "@lit-material/router";
import { LitElement, css, html, nothing } from "lit";
import { Api, normalizeServer, passkeySignIn, passwordReset, passwordSignIn, passwordSignUp, ssoFinish, ssoStart } from "../api.js";
import { describeError } from "../actions.js";
import { notify } from "../notify.js";
import { session } from "../session.js";
import { pageStyles } from "./page-styles.js";

export class SignInPage extends LitElement {
  static styles = [
    pageStyles,
    css`
      :host {
        display: grid;
        place-items: center;
        min-height: 100vh;
        padding: var(--space-200);
        box-sizing: border-box;
      }
      .wrap {
        width: min(100%, 400px);
        display: grid;
        gap: var(--space-200);
      }
      lit-material-sign-in {
        --lit-material-sign-in-width: 400px;
      }
      .logo {
        display: block;
        width: 40px;
        height: 40px;
      }
      .providers {
        display: grid;
        gap: var(--space-75);
      }
      .providers lit-material-button,
      .stack lit-material-button {
        width: 100%;
      }
      .foot {
        display: grid;
        gap: var(--space-100);
        text-align: center;
      }
      .foot p {
        color: var(--md-sys-color-on-surface-variant);
        font-size: var(--text-xs);
        line-height: var(--leading-xs);
      }
      .card h1 {
        font-size: var(--text-2xl);
        line-height: var(--leading-2xl);
      }
      .stack {
        display: grid;
        gap: var(--space-100);
      }
    `,
  ];

  static properties = {
    busy: { state: true },
    error: { state: true },
    notice: { state: true },
    step: { state: true },
    view: { state: true },
    methods: { state: true },
    sso: { state: true },
  };

  constructor() {
    super();
    this.busy = false;
    this.error = "";
    this.notice = "";
    // Waiting for a mailed code: { kind: "code" | "signup" | "reset", email }.
    this.step = null;
    this.view = { mode: "sign-in", method: "password" };
    this.methods = null;
    this.sso = null;
  }

  connectedCallback() {
    super.connectedCallback();
    const sso = /^#sso=(.+)$/.exec(location.hash);
    if (sso) {
      history.replaceState(null, "", location.pathname + location.search);
      this.finishSso(decodeURIComponent(sso[1]));
    } else if (session.signedIn) {
      navigate(session.next || "/", { replace: true });
      return;
    }
    this.loadMethods();
  }

  get server() {
    return normalizeServer(session.server);
  }

  async loadMethods() {
    try {
      const api = new Api(this.server);
      this.sso = await api.get("/v1/auth/sso");
      this.methods = await api.get("/v1/auth/methods").catch(() => null);
    } catch {
      this.sso = null;
    }
  }

  signedIn(server, reply) {
    session.signIn(server, reply);
    navigate(session.next || "/", { replace: true });
  }

  async run(fn) {
    this.busy = true;
    this.error = "";
    try {
      await fn();
    } catch (e) {
      this.error = describeError(e).message;
    } finally {
      this.busy = false;
    }
  }

  async finishSso(code) {
    await this.run(async () => {
      const { server, reply } = await ssoFinish(code);
      this.signedIn(server, reply);
    });
  }

  render() {
    if (this.step) return this.codeStep();
    const providers = this.methods?.google || this.methods?.github || this.methods?.apple || this.sso?.enabled;
    return html`<main class="wrap">
      ${this.view.mode === "forgot"
        ? html`<lit-material-alert variant="warning" aria-label="Reset warning">
            A reset signs you in again, but it can't unlock your end-to-end encrypted workspaces on a new device: your old password sealed their keys, and nobody, the server included, can open them without it. Keep your recovery phrase or a device that is still signed in.
          </lit-material-alert>`
        : nothing}
      <lit-material-sign-in
        methods="password code sign-up forgot"
        .email=${session.email}
        ?busy=${this.busy}
        .error=${this.error}
        .notice=${this.notice}
        @lit-material-sign-in=${this.submit}
        @lit-material-sign-in-view-change=${(e) => {
          this.view = e.detail;
          this.error = "";
        }}
      >
        <img slot="logo" class="logo" src="https://hitop.rws8.tech/favicon.svg" alt="HITOP" width="40" height="40" />
        ${providers
          ? html`<div slot="providers" class="providers">
              ${this.methods?.google ? this.provider("google", "Continue with Google") : nothing}
              ${this.methods?.github ? this.provider("github", "Continue with GitHub") : nothing}
              ${this.methods?.apple ? this.provider("apple", "Continue with Apple") : nothing}
              ${this.sso?.enabled ? this.provider("sso", `Sign in with ${this.sso.name || "SSO"}`) : nothing}
            </div>`
          : nothing}
        <div slot="footer" class="foot">
          ${this.view.mode === "sign-in"
            ? html`<lit-material-button variant="text" ?disabled=${this.busy} @click=${this.passkey}>Sign in with a passkey</lit-material-button>`
            : nothing}
          <p>The same account as hitop sync: the app's Sync page or <code>hitop sync login</code>. Your collections stay end-to-end encrypted: this page never sees them.</p>
        </div>
      </lit-material-sign-in>
    </main>`;
  }

  provider(kind, label) {
    return html`<lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.startProvider(kind)}>${label}</lit-material-button>`;
  }

  /** The address typed so far (the SSO button needs it before the form is submitted). */
  typedEmail() {
    const form = this.renderRoot.querySelector("lit-material-sign-in");
    return String(form?.renderRoot?.querySelector('lit-material-text-field[type="email"]')?.value ?? form?.email ?? "").trim();
  }

  startProvider(kind) {
    return this.run(async () => {
      const s = this.server;
      const email = this.typedEmail();
      // Organizations bring their own SSO (C5): the server finds it from the work address.
      if (kind === "sso" && (this.sso?.orgSso || this.methods?.sso?.orgSso) && !email) {
        throw new Error("Enter your work address first: it tells the server which organization's single sign-on to use.");
      }
      session.remember(s, email);
      location.assign(await ssoStart(new Api(s), email, kind));
    });
  }

  passkey = () =>
    this.run(async () => {
      const s = this.server;
      const reply = await passkeySignIn(new Api(s), this.typedEmail());
      this.signedIn(s, reply);
    });

  submit = (event) => {
    const d = event.detail;
    return this.run(async () => {
      const s = this.server;
      const api = new Api(s);
      if (d.mode === "sign-up") {
        await passwordSignUp(api, d.email, d.password || "");
        session.remember(s, d.email);
        this.step = { kind: "signup", email: d.email };
      } else if (d.mode === "forgot") {
        await api.call("POST", "/v1/auth/password/forgot", { email: d.email });
        session.remember(s, d.email);
        this.step = { kind: "reset", email: d.email };
      } else if (d.method === "code") {
        await api.call("POST", "/v1/auth/email", { email: d.email });
        session.remember(s, d.email);
        this.step = { kind: "code", email: d.email };
      } else {
        this.signedIn(s, await passwordSignIn(api, d.email, d.password || ""));
      }
    });
  };

  // --- the mailed code: pasted as is, or the whole link (the code is after its '#') -------

  codeStep() {
    const { kind, email } = this.step;
    const words = {
      code: { heading: "Check your mail", submit: "Sign in" },
      signup: { heading: "Confirm your address", submit: "Confirm" },
      reset: { heading: "Set a new password", submit: "Reset password" },
    }[kind];
    return html`<main class="wrap">
      <form class="card" @submit=${this.finishStep}>
        <img class="logo" src="https://hitop.rws8.tech/favicon.svg" alt="HITOP" width="40" height="40" />
        <h1>${words.heading}</h1>
        <p class="lead">We mailed a code to <strong>${email}</strong>. Paste it, or the whole link.</p>
        ${this.error ? html`<lit-material-alert variant="error">${this.error}</lit-material-alert>` : nothing}
        <div class="stack">
          <lit-material-text-field name="code" variant="outlined" label="Code from the mail" autocomplete="one-time-code" required></lit-material-text-field>
          ${kind === "reset"
            ? html`<lit-material-text-field name="password" variant="outlined" label="New password" type="password" autocomplete="new-password" required minlength="8"></lit-material-text-field>
                <lit-material-text-field name="password2" variant="outlined" label="Repeat the new password" type="password" autocomplete="new-password" required></lit-material-text-field>`
            : nothing}
          <lit-material-button type="submit" ?disabled=${this.busy}>${this.busy ? "One moment…" : words.submit}</lit-material-button>
          <lit-material-button variant="text" ?disabled=${this.busy} @click=${() => (this.step = null)}>Use a different email</lit-material-button>
        </div>
      </form>
    </main>`;
  }

  finishStep = (event) => {
    event.preventDefault();
    const form = event.target;
    if (!form.reportValidity()) return;
    const data = new FormData(form);
    const token = String(data.get("code") || "").trim().split("#").pop().trim();
    const { kind } = this.step;
    return this.run(async () => {
      const s = this.server;
      const api = new Api(s);
      if (kind === "code") {
        this.signedIn(s, await api.call("POST", "/v1/auth/email/verify", { token }));
      } else if (kind === "signup") {
        const reply = await api.call("POST", "/v1/auth/password/verify", { token });
        notify("Account ready. Sign in with the same password in hitop: your first device seals its keys with it.");
        this.signedIn(s, reply);
      } else {
        const password = String(data.get("password") || "");
        if (password !== String(data.get("password2") || "")) throw new Error("The two new passwords differ");
        const reply = await passwordReset(api, token, password);
        notify(reply.bundleLost ? "Password reset. The key bundle your old password sealed is gone: on a new device, use your recovery phrase or a device that still has your workspaces." : "Password reset.");
        this.signedIn(s, reply);
      }
    });
  };
}

customElements.define("sign-in-page", SignInPage);
