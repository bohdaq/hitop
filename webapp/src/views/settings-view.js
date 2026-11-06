// The Settings page, in the web app and the desktop: theme, the request time
// limit and what the app does with requests, the storage it uses (and
// clearing stored responses), and the keyboard shortcuts. The desktop adds
// its own sections (platform().settingsSections: secret managers and, where
// the build has it, its weekly ping) from platform/tauri-settings.js, which
// the web build never contains.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";
import { COLOR_SCHEME_EVENT, setColorScheme, storedColorScheme } from "@lit-material/color-scheme";
import { AI_MODELS, aiSettings, saveAiSettings } from "../ai.js";
import { platform } from "../platform/index.js";
import { storageUsage } from "../platform/response-store.js";
import { seconds } from "../settings.js";
import { shortcutList } from "./shortcut-list.js";

const THEME_KEY = "hitop-theme";

const size = (n) => (n == null ? "unknown" : n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);

class HitopAppSettings extends LitElement {
  static properties = {
    store: { attribute: false },
    version: { attribute: false },
    app: { attribute: false },
    usage: { state: true },
    theme: { state: true },
  };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.theme = storedColorScheme(THEME_KEY);
    this.onScheme = () => (this.theme = storedColorScheme(THEME_KEY));
    document.addEventListener(COLOR_SCHEME_EVENT, this.onScheme);
    this.measure();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    document.removeEventListener(COLOR_SCHEME_EVENT, this.onScheme);
  }

  async measure() {
    await this.store.responsesRestored;
    this.usage = await storageUsage();
  }

  setTimeoutSeconds(value) {
    const n = Number(String(value).trim() || 0);
    if (!Number.isFinite(n) || n < 0) return;
    this.store.setSettings({ timeoutMs: Math.round(n * 1000) });
  }

  async clearResponses() {
    const ok = await confirm({
      headline: "Clear stored responses?",
      message: "The responses your tabs show, and the large ones kept for after a reload, are removed. Requests, collections and History stay.",
      confirmLabel: "Clear",
      destructive: true,
    });
    if (!ok) return;
    await this.store.clearStoredResponses();
    this.measure();
  }

  renderStorage() {
    const u = this.usage;
    const s = this.store;
    const local = u?.local ?? {};
    const sum = (keys) => keys.reduce((n, k) => n + (local[k] ?? 0), 0);
    const localTotal = Object.values(local).reduce((a, b) => a + b, 0);
    const desktop = platform().name === "tauri";
    const rows = [
      ["Open tabs", sum(["hitop_tabs", "hitop_current_tab"])],
      ...(desktop ? [] : [["Collections", sum(["hitop_collections"])], ["History", sum(["hitop_history"])], ["Environments and globals", sum(["hitop_environments", "hitop_active_environment", "hitop_globals"])]]),
    ];
    const kept = s.keptBodies.size;
    return html`<section aria-label="Storage" class="settings-section">
      <h2>Storage</h2>
      <p class="page-lead">${desktop
        ? "Collections, environments and History are files in hitop's settings folder, shared with the CLI. This window keeps your open tabs, and large responses for after a reload."
        : "Everything stays in this browser. Browsers give a site about 5 MB of localStorage; large responses go to IndexedDB, which has more room."}</p>
      ${u
        ? html`<dl class="settings-facts">
            ${rows.map(([what, n]) => html`<dt>${what}</dt><dd>${size(n)}</dd>`)}
            <dt>localStorage in all</dt><dd>${size(localTotal)}</dd>
            <dt>Large responses (IndexedDB)</dt><dd>${s.responseStore ? `${kept} kept` : "not available here: responses over 256 KB are kept as their first 64 KB"}</dd>
            ${u.usage != null ? html`<dt>The browser counts</dt><dd>${size(u.usage)}${u.quota ? ` of ${size(u.quota)}` : ""}</dd>` : nothing}
          </dl>`
        : html`<p class="hint">Measuring…</p>`}
      <div class="settings-actions">
        <lit-material-button variant="outlined" @click=${() => this.clearResponses()}>Clear stored responses</lit-material-button>
      </div>
    </section>`;
  }

  // HITOP Cloud's personal library (library.js): signed in, collections are saved there unless
  // this is off (they then stay in this browser or on this computer).
  renderPrivacy() {
    const lib = this.store.library;
    if (!lib) return nothing;
    const signedIn = Boolean(lib.client.account());
    const on = lib.status.on;
    return html`<section aria-label="Privacy" class="settings-section">
      <h2>Privacy</h2>
      ${signedIn
        ? html`<label class="switch-row">
              <lit-material-switch aria-label="Save my collections to HITOP Cloud" ?checked=${on}
                @change=${(e) => lib.setEnabled(e.target.checked).then(() => this.requestUpdate())}></lit-material-switch>
              <span>Save my collections to HITOP Cloud</span>
            </label>
            <p class="hint sync-library">${on
              ? "Your collections, environments and globals open on every device you sign in on. HITOP Cloud can read them; collections you share with people stay end-to-end encrypted."
              : "Off: your collections stay in this browser only. Shared collections still sync, end-to-end encrypted."}</p>`
        : html`<p class="hint">Sign in (top right) to save your collections to HITOP Cloud. Until then they stay on this device.</p>`}
    </section>`;
  }

  // Ask AI (ai.js): the person's own Anthropic API key, on this device only.
  renderAi() {
    const ai = aiSettings();
    const save = (changes) => {
      saveAiSettings({ ...aiSettings(), ...changes });
      this.requestUpdate();
    };
    return html`<section aria-label="AI assistant" class="settings-section">
      <h2>AI assistant</h2>
      <p class="hint">Ask Claude to write tests for a response, explain a failure, or make a request from a curl command or a description (the sparkle button next to Send). It uses your own Anthropic API key, kept on this device only; secret values and credential headers are hidden from it.</p>
      <div class="sync-form">
        <lit-material-text-field id="ai-key" variant="outlined" type="password" label="Anthropic API key" .value=${ai.key} placeholder="sk-ant-…"
          @change=${(e) => save({ key: e.target.value.trim() })}></lit-material-text-field>
        <lit-material-select variant="outlined" label="Model" .value=${ai.model} @change=${(e) => save({ model: e.target.value })}>
          ${AI_MODELS.map(([id, label]) => html`<lit-material-select-option value=${id}>${label}</lit-material-select-option>`)}
        </lit-material-select>
        ${ai.key ? html`<lit-material-button variant="text" @click=${() => save({ key: "" })}>Remove the key</lit-material-button>` : nothing}
      </div>
    </section>`;
  }

  render() {
    const s = this.store;
    const desktop = platform().name === "tauri";
    const timeout = s.settings.timeoutMs;
    const extra = platform().settingsSections;
    return html`
      <div class="page settings-page">
        <header class="page-header"><h1>Settings</h1></header>
        <section aria-label="Appearance" class="settings-section">
          <h2>Appearance</h2>
          <lit-material-select class="settings-theme" variant="outlined" label="Theme" .value=${this.theme} @change=${(e) => setColorScheme(e.target.value, { storageKey: THEME_KEY })}>
            <lit-material-select-option value="light">Light</lit-material-select-option>
            <lit-material-select-option value="dark">Dark</lit-material-select-option>
            <lit-material-select-option value="auto">Match the system</lit-material-select-option>
          </lit-material-select>
          <p class="hint">The sun and moon button in the top bar switches it too.</p>
        </section>
        <section aria-label="Requests" class="settings-section">
          <h2>Requests</h2>
          <lit-material-text-field class="settings-timeout" variant="outlined" type="number" min="0" step="1" label="Time limit (seconds)"
            .value=${timeout ? String(timeout / 1000) : "0"}
            supporting-text=${timeout ? `A request with no response within ${seconds(timeout)} stops (Send and collection runs). 0 means no limit.` : "No limit: a request waits until the server answers or you cancel it. Applies to Send and collection runs."}
            @change=${(e) => this.setTimeoutSeconds(e.target.value)}></lit-material-text-field>
          <label class="switch-row">
            <lit-material-switch aria-label="Keep and send cookies" ?checked=${s.cookieJarEnabled} @change=${(e) => s.setCookieJarEnabled(e.target.checked)}></lit-material-switch>
            <span>Keep and send cookies (the cookie jar; its cookies are on the Cookies page)</span>
          </label>
          <ul class="settings-notes">
            ${desktop
              ? html`<li>Requests go straight from this computer: no proxy, no CORS.</li>`
              : html`<li>Requests to public hosts go through the HITOP proxy, so CORS doesn't block them; the proxy gives each one up to 20 s. Requests to localhost and private networks go straight from your browser.</li>`}
            <li>Redirects are followed. TLS certificates are always checked: there is no switch to turn that off.</li>
          </ul>
        </section>
    ${this.renderPrivacy()}
    ${this.renderAi()}
    ${this.renderStorage()}
        <section aria-label="Keyboard shortcuts" class="settings-section">
          <h2>Keyboard shortcuts</h2>
          ${shortcutList()}
        </section>
        ${extra ? extra(s, this.app) : nothing}
      </div>`;
  }
}

customElements.define("hitop-app-settings", HitopAppSettings);
