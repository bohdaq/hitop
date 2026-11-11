// <hitop-sign-in>: the Sign in dialog (dialog kind "sign-in", hitop-app.js openSignIn), over
// whatever is on screen; after signing in it closes and the person stays where they were.
// Steps (`step`, mirrored in the app's dialog data for the headline and `data-step`):
//   loading     asking the server which providers it has (/v1/auth/methods)
//   providers   Continue with GitHub / Google (only those the server has), the fine print, and
//               "Company SSO or your own server…"
//   sso         Work email (an organization's sign-on by its domain) and Server; errors inline
//   waiting     the provider's pop-up is open (web): it sends the one-time code back through
//               /app/sync-callback.html; "Open the window again", Cancel, and when the pop-up
//               was blocked "Continue in this tab" (the tab goes and comes back, hitop-app resumeSignIn)
//   paste       the provider's page shows the code to paste (the desktop app, or a server that
//               doesn't send the browser back to this app)
//   finishing   trading the code for a session
// No passwords and no mailed codes here: the sync server keeps those endpoints for the CLI and
// HITOP Cloud's portal. Logic in sync.js (startBrowserSignIn / finishBrowserSignIn).
import { LitElement, html, nothing } from "lit";
import { unsafeSVG } from "lit/directives/unsafe-svg.js";
import { mark } from "../../../site/src/brand/mark.js";
import { DEFAULT_SERVER, DeviceLimitError, SyncClient } from "../sync.js";
import { syncClient } from "./sync-view.js";
import { loadSettings, saveSettings } from "../settings.js";
import { platform } from "../platform/index.js";

const PRIVACY_URL = "https://hitop.rws8.tech/privacy.html";
const CHANNEL = "hitop-sign-in";
const PROVIDER = { github: "GitHub", google: "Google", sso: "your organization's" };

const GITHUB_MARK = `<svg viewBox="0 0 16 16" width="18" height="18" aria-hidden="true"><path fill="currentColor" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>`;
const GOOGLE_G = `<svg viewBox="0 0 48 48" width="18" height="18" aria-hidden="true"><path fill="#EA4335" d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z"/><path fill="#4285F4" d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z"/><path fill="#FBBC05" d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z"/><path fill="#34A853" d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z"/></svg>`;

const errorText = (e) => (e && e.message) || String(e);
const flowId = () => Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => b.toString(16).padStart(2, "0")).join("");

// Which field a failed SSO start belongs under: the address (an organization's sign-on) or the server.
function fieldOf(e) {
  if (["org_sso_off", "sso_off"].includes(e?.code) || e?.status === 403 || /work address|this address/i.test(errorText(e))) return "email";
  if (e?.code === "no_sso" || e?.code === "no_provider" || e instanceof TypeError || /sync server|https?:\/\/|URL/i.test(errorText(e))) return "server";
  return "general";
}

class HitopSignIn extends LitElement {
  static properties = {
    store: { attribute: false },
    app: { attribute: false },
    resume: { attribute: false },
    step: { state: true },
    methods: { state: true },
    kind: { state: true },
    url: { state: true },
    blocked: { state: true },
    busy: { state: true },
    errors: { state: true },
    limit: { state: true },
  };

  createRenderRoot() {
    return this;
  }

  constructor() {
    super();
    this.step = "loading";
    this.errors = {};
    this.flow = "";
    this.back = "providers";
    this.email = "";
    this.server = loadSettings().syncServer || DEFAULT_SERVER;
    this.onMessage = (e) => {
      const m = e.data?.hitopSignIn;
      if (!m || !this.flow || m.flow !== this.flow || this.step !== "waiting") return;
      if (e.origin !== undefined && e.origin !== "" && e.origin !== location.origin) return;
      this.finish(m.code);
    };
  }

  get sync() {
    return syncClient(this.store);
  }

  get web() {
    return platform().name !== "tauri";
  }

  connectedCallback() {
    super.connectedCallback();
    try {
      this.channel = new BroadcastChannel(CHANNEL);
      this.channel.onmessage = this.onMessage;
    } catch {
      this.channel = null;
    }
    window.addEventListener("message", this.onMessage);
    if (this.resume) {
      // Back from "Continue in this tab": the provider sent this tab here with the code.
      this.kind = SyncClient.pendingInTab()?.kind ?? "";
      this.finish(this.resume);
      return;
    }
    this.loadMethods(this.server, true);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.channel?.close();
    this.channel = null;
    window.removeEventListener("message", this.onMessage);
    this.flow = "";
  }

  setStep(step) {
    this.step = step;
    const title = step === "sso" && this.hasProviders ? "Company SSO or your own server" : "Sign in to HITOP";
    if (this.app?.dialog?.kind === "sign-in") this.app.dialog = { ...this.app.dialog, step, title };
    this.updateComplete.then(() => this.focusFirst());
  }

  // The first thing to act on in each step (the first provider button on the first step), or the
  // field an error is about.
  focusFirst() {
    const el = this.querySelector("lit-material-text-field[error]") ?? this.querySelector("[data-first]");
    if (el) requestAnimationFrame(() => el.focus());
  }

  // The server's sign-in methods; unknown or unreachable counts as none (the second step).
  async loadMethods(server, first = false) {
    const m = await this.sync.authMethods(server).catch(() => null);
    if (server !== this.server) return;
    this.methods = m;
    if (first) this.setStep(this.hasProviders ? "providers" : "sso");
  }

  get hasProviders() {
    return Boolean(this.methods?.github || this.methods?.google);
  }

  field(id) {
    return String(this.querySelector(`#${id}`)?.value ?? "").trim();
  }

  // --- the browser flow -------------------------------------------------------------------

  // Where the provider's way back lands: the pop-up's callback page, this tab (`tab`), or
  // nowhere (the desktop: a page to paste the code from).
  returnTo(tab) {
    if (!this.web) return "";
    if (tab) return location.href.split("#")[0];
    const u = new URL("sync-callback.html", location.href);
    u.search = `?flow=${this.flow}`;
    u.hash = "";
    return u.href;
  }

  // Starts a flow on the server; a server that won't send the browser back here (extensions, a
  // self-hosted server that doesn't list this origin) gets the paste step instead.
  async start(kind, { tab = false } = {}) {
    this.flow = flowId();
    const returnTo = this.returnTo(tab);
    const opts = { email: kind === "sso" ? this.email : "", returnTo, keep: tab };
    try {
      return { url: await this.sync.startBrowserSignIn(this.server, kind, opts), paste: !returnTo };
    } catch (e) {
      if (!returnTo || e?.status !== 400 || !/returnTo/i.test(errorText(e))) throw e;
      return { url: await this.sync.startBrowserSignIn(this.server, kind, { ...opts, returnTo: "", keep: false }), paste: true };
    }
  }

  // Continue with GitHub / Google / SSO. The pop-up opens in the click itself (later, browsers
  // block it) and gets the provider's address once the server answered.
  async begin(kind) {
    this.errors = {};
    this.kind = kind;
    const popup = this.web ? window.open("about:blank", "hitop-sign-in", "popup,width=520,height=720") : null;
    this.blocked = this.web && !popup;
    this.busy = true;
    try {
      const { url, paste } = await this.start(kind);
      this.url = url;
      if (paste) {
        popup?.close();
        if (this.web) window.open(url, "_blank", "noopener,noreferrer");
        this.setStep("paste");
        return;
      }
      if (popup) popup.location.href = url;
      this.setStep("waiting");
    } catch (e) {
      popup?.close();
      this.fail(e);
    } finally {
      this.busy = false;
    }
  }

  // The pop-up was blocked or closed: the same provider in this tab (the app comes back here).
  async inThisTab() {
    this.busy = true;
    try {
      const { url } = await this.start(this.kind, { tab: true });
      location.assign(url);
    } catch (e) {
      this.busy = false;
      this.fail(e);
    }
  }

  fail(e) {
    if (this.kind === "sso" || this.step === "sso") {
      const where = fieldOf(e);
      const text = !(e instanceof TypeError) ? errorText(e) : /URL/.test(errorText(e)) ? "Enter the server's address, like https://sync.example.com" : `Couldn't reach ${this.server}`;
      this.errors = { [where]: text };
      this.setStep("sso");
    } else {
      this.errors = { general: errorText(e) };
      this.setStep(this.hasProviders ? "providers" : "sso");
    }
  }

  async finish(code) {
    const was = this.step;
    this.setStep("finishing");
    try {
      const r = await this.sync.finishBrowserSignIn(code);
      await this.signedIn(r);
    } catch (e) {
      if (e instanceof DeviceLimitError) return this.atLimit(e);
      this.errors = was === "paste" ? { code: errorText(e) } : { general: `Couldn't finish signing in: ${errorText(e)}` };
      if (was === "paste") this.setStep("paste");
      else if (this.methods) this.setStep(this.hasProviders ? "providers" : "sso");
      else this.loadMethods(this.server, true);
    }
  }

  // The plan's device limit: the account's devices, one to remove, then the sign-in again.
  atLimit(e) {
    this.limit = e;
    this.errors = {};
    this.setStep("devices");
  }

  async removeDevice(id) {
    const e = this.limit;
    this.busy = true;
    this.errors = {};
    try {
      await this.sync.removeDevice(e.pending, id);
      this.setStep("finishing");
      await this.signedIn(await this.sync.retrySignIn(e.pending));
    } catch (err) {
      if (err instanceof DeviceLimitError) return this.atLimit(err);
      this.errors = err?.code === "key_in_use"
        ? { [id]: "This device holds keys to shared collections, so it stays. Remove it from that device, or pick another." }
        : { general: errorText(err) };
      this.setStep("devices");
    } finally {
      this.busy = false;
    }
  }

  async signedIn(r) {
    const settings = loadSettings();
    const server = this.sync.account()?.server ?? this.server;
    if (server === DEFAULT_SERVER) delete settings.syncServer;
    else settings.syncServer = server;
    saveSettings(settings);
    await this.app.signedIn(r, true);
  }

  cancel() {
    this.flow = "";
    this.errors = {};
    this.setStep(this.kind === "sso" ? "sso" : this.hasProviders ? "providers" : "sso");
  }

  // Enter in a field of the dialog's form (hitop-app submitDialog).
  submitStep() {
    if (this.busy) return;
    if (this.step === "sso") this.continueSso();
    else if (this.step === "paste") this.pasted();
  }

  continueSso() {
    this.email = this.field("signin-email");
    const server = this.field("signin-server") || DEFAULT_SERVER;
    if (server !== this.server) {
      this.server = server;
      this.loadMethods(server);
    }
    this.begin("sso");
  }

  pasted() {
    const code = this.field("signin-code");
    if (!code) {
      this.errors = { code: "Paste the code the sign-in page shows" };
      return;
    }
    this.finish(code);
  }

  // --- rendering ------------------------------------------------------------------------------

  renderProviders() {
    const m = this.methods ?? {};
    const list = [m.github && "github", m.google && "google"].filter(Boolean);
    return html`
      <div class="signin-brand" aria-hidden="true">${unsafeSVG(mark())}</div>
      <h2 class="signin-title">Sign in to HITOP</h2>
      <p class="signin-lead">Sync your collections across devices and share them with your team. New here? The same button creates your account.</p>
      <div class="signin-providers">
        ${list.map((k, i) => html`<button type="button" class="signin-provider" id=${`signin-${k}`} ?data-first=${i === 0} ?disabled=${this.busy}
          @click=${() => this.begin(k)}>${unsafeSVG(k === "github" ? GITHUB_MARK : GOOGLE_G)}<span>Continue with ${PROVIDER[k]}</span></button>`)}
      </div>
      ${this.errors.general ? html`<p class="signin-error" role="alert">${this.errors.general}</p>` : nothing}
      <p class="signin-fine">Requests, environments and the CLI keep working without an account.
        <a href=${PRIVACY_URL} target="_blank" rel="noopener noreferrer">Privacy policy</a></p>
      <div class="signin-more">
        <button type="button" class="signin-link" @click=${() => ((this.errors = {}), this.setStep("sso"))}>Company SSO or your own server…</button>
      </div>`;
  }

  renderSso() {
    const e = this.errors;
    return html`
      ${this.hasProviders ? html`<button type="button" class="signin-back" @click=${() => ((this.errors = {}), this.setStep("providers"))}>‹ Back</button>` : nothing}
      <h2 class="signin-title">${this.hasProviders ? "Company SSO or your own server" : "Sign in to HITOP"}</h2>
      <p class="signin-lead">Sign in with your company's identity provider, or on a HITOP sync server your team runs. Your work email finds your organization's sign-on.</p>
      <lit-material-text-field id="signin-email" data-first variant="outlined" label="Work email" type="email" autocomplete="email" .value=${this.email}
        ?error=${Boolean(e.email)} error-text=${e.email ?? ""} supporting-text=${e.email ? "" : "Leave empty for the server's own sign-on"}
        @input=${(ev) => ((this.email = String(ev.target.value ?? "").trim()), e.email && (this.errors = { ...e, email: "" }))}></lit-material-text-field>
      <lit-material-text-field id="signin-server" variant="outlined" label="Server" type="url" .value=${this.server}
        ?error=${Boolean(e.server)} error-text=${e.server ?? ""}
        @input=${() => e.server && (this.errors = { ...e, server: "" })}
        @change=${(ev) => {
          const v = String(ev.target.value ?? "").trim() || DEFAULT_SERVER;
          if (v === this.server) return;
          this.server = v;
          this.methods = null;
          this.loadMethods(v);
        }}></lit-material-text-field>
      ${e.general ? html`<p class="signin-error" role="alert">${e.general}</p>` : nothing}
      <lit-material-button variant="filled" class="signin-primary" ?disabled=${this.busy} @click=${() => this.continueSso()}>Continue with SSO</lit-material-button>
      ${this.hasProviders ? nothing : html`<p class="signin-fine">Requests, environments and the CLI keep working without an account.</p>`}`;
  }

  renderWaiting() {
    const name = PROVIDER[this.kind] ?? "the sign-in";
    return html`
      <div class="signin-wait" role="status" aria-live="polite">
        <span class="signin-spinner" aria-hidden="true"></span>
        <h2 class="signin-title">${this.blocked ? "Your browser blocked the sign-in window" : this.kind === "sso" ? "Continue in your organization's sign-in window" : `Continue in the ${name} window`}</h2>
        <p class="signin-lead">${this.blocked ? "Open it again, or continue in this tab: HITOP comes back here when you're signed in." : "Approve HITOP there; this dialog closes by itself."}</p>
      </div>
      <div class="signin-actions">
        ${this.blocked
          ? html`<lit-material-button variant="filled" data-first ?disabled=${this.busy} @click=${() => this.inThisTab()}>Continue in this tab</lit-material-button>`
          : nothing}
        <lit-material-button variant=${this.blocked ? "outlined" : "filled"} ?data-first=${!this.blocked} ?disabled=${this.busy} @click=${() => this.begin(this.kind)}>Open the window again</lit-material-button>
        <lit-material-button variant="text" @click=${() => this.cancel()}>Cancel</lit-material-button>
      </div>`;
  }

  renderPaste() {
    const e = this.errors;
    const name = this.kind === "sso" ? "your organization" : PROVIDER[this.kind];
    return html`
      <h2 class="signin-title">Finish in your browser</h2>
      <p class="signin-lead">Sign in with ${name} on the page that opened, then paste the code it shows here. <a href=${this.url} target="_blank" rel="noopener noreferrer">Open the sign-in page</a></p>
      <pre class="code-preview signin-url" aria-label="SSO sign-in link">${this.url}</pre>
      <lit-material-text-field id="signin-code" data-first variant="outlined" label="Code shown after signing in" autocomplete="off"
        ?error=${Boolean(e.code)} error-text=${e.code ?? ""} @input=${() => e.code && (this.errors = {})}></lit-material-text-field>
      <div class="signin-actions">
        <lit-material-button variant="filled" ?disabled=${this.busy} @click=${() => this.pasted()}>Sign in</lit-material-button>
        <lit-material-button variant="text" @click=${() => this.cancel()}>Cancel</lit-material-button>
      </div>`;
  }

  renderDevices() {
    const e = this.limit;
    const plan = e.plan ? `${e.plan[0].toUpperCase()}${e.plan.slice(1)} plan` : "plan";
    return html`
      <h2 class="signin-title">Remove a device to sign in here</h2>
      <p class="signin-lead">Your ${plan} allows ${e.max} ${e.max === 1 ? "device" : "devices"}, and this account has ${e.keys.length}. Remove one you no longer use; nothing else on the account changes.
        ${e.upgradeUrl ? html`Or <a href=${e.upgradeUrl} target="_blank" rel="noopener noreferrer">see plans</a>.` : nothing}</p>
      <ul class="signin-devices" aria-label="Devices on this account">
        ${e.keys.map((k, i) => html`<li>
          <span class="signin-device-name">${k.name || `Device ${i + 1}`}</span>
          <lit-material-button variant="outlined" ?disabled=${this.busy} @click=${() => this.removeDevice(k.id)}>Remove<span class="visually-hidden"> ${k.name || `device ${i + 1}`}</span></lit-material-button>
          ${this.errors[k.id] ? html`<p class="signin-error" role="alert">${this.errors[k.id]}</p>` : nothing}
        </li>`)}
      </ul>
      ${this.errors.general ? html`<p class="signin-error" role="alert">${this.errors.general}</p>` : nothing}
      <div class="signin-actions">
        <lit-material-button variant="text" ?disabled=${this.busy} @click=${() => ((this.limit = null), this.cancel())}>Cancel</lit-material-button>
      </div>`;
  }

  render() {
    const step = this.step;
    const body =
      step === "providers" ? this.renderProviders()
      : step === "sso" ? this.renderSso()
      : step === "waiting" ? this.renderWaiting()
      : step === "paste" ? this.renderPaste()
      : step === "devices" && this.limit ? this.renderDevices()
      : html`<div class="signin-wait" role="status"><span class="signin-spinner" aria-hidden="true"></span><p class="signin-lead">${step === "finishing" ? "Signing in…" : "Loading…"}</p></div>`;
    return html`<div class="signin" data-step=${step} aria-busy=${String(step === "loading" || step === "finishing" || Boolean(this.busy))}>${body}</div>`;
  }
}

customElements.define("hitop-sign-in", HitopSignIn);

// The dialog's parts for hitop-app renderDialog: the visible title is in the body (the mark and
// the back link sit around it), the headline slot names the dialog for assistive technology.
export function signInDialog(app, d) {
  return {
    headline: html`<span class="visually-hidden">${d.title ?? "Sign in to HITOP"}</span>`,
    body: html`<hitop-sign-in .store=${app.store} .app=${app} .resume=${d.resume ?? ""}></hitop-sign-in>`,
    footer: nothing,
  };
}
