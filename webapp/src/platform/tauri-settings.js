// The desktop app's Settings page and its first-run question: the opt-in
// usage ping (desktop/src-tauri/src/commands.rs `telemetry_status`,
// `telemetry_set`; hitop-core's telemetry module). Only the Tauri adapter
// imports this file, so the web build never contains it. In desktop builds
// without the ping (Mac App Store) `telemetry_status` fails and neither the
// page nor the question appears.
import { LitElement, html } from "lit";
import { confirm } from "@lit-material/confirm";

export const PRIVACY_URL = "https://hitop.rws8.tech/privacy.html";

const describe = (status) => `POST ${status.endpoint}\n${JSON.stringify(status.ping, null, 2)}`;
const day = (secs) => new Date(secs * 1000).toISOString().slice(0, 10);

class HitopSettings extends LitElement {
  static properties = { telemetry: { attribute: false }, status: { state: true }, error: { state: true } };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.telemetry.status().then(
      (s) => (this.status = s),
      (e) => (this.error = String(e)),
    );
  }

  async toggle(on) {
    try {
      this.status = await this.telemetry.set(on);
    } catch (e) {
      this.error = String(e);
    }
  }

  render() {
    const s = this.status;
    return html`
      <div class="page">
        <header class="page-header"><h1>Settings</h1></header>
        <h2>Usage ping</h2>
        ${s
          ? html`
              <label class="switch-row">
                <lit-material-switch aria-label="Send an anonymous usage ping once a week" ?checked=${s.enabled} @change=${(e) => this.toggle(e.target.checked)}></lit-material-switch>
                <span>Send an anonymous usage ping once a week</span>
              </label>
              <p class="page-lead">
                Off unless you turn it on. When it's on, HITOP sends this at most once a week, and nothing else: a random id made on this computer, the app, its version and the operating system. No requests, URLs or collections. ${PRIVACY_URL}
              </p>
              <pre class="code-preview" aria-label="What the ping sends">${describe(s)}</pre>
              ${s.enabled && s.lastPing ? html`<p class="hint">Last sent ${day(s.lastPing)}.</p>` : ""}
            `
          : html`<p class="hint">${this.error ?? "Loading…"}</p>`}
      </div>`;
  }
}

customElements.define("hitop-settings", HitopSettings);

// `telemetry`: { status(), set(on) } from the adapter. Returns the Settings
// page for the sidebar, or none when this build has no ping.
export async function settingsPages(telemetry) {
  let status;
  try {
    status = await telemetry.status();
  } catch {
    return { pages: [], status: null };
  }
  const pages = [{ kind: "settings", label: "Settings", icon: "sliders-horizontal", render: () => html`<hitop-settings .telemetry=${telemetry}></hitop-settings>` }];
  return { pages, status };
}

// Asked once, on the first start: No (Cancel) has the focus, and Escape or
// a click outside counts as No. Either answer is stored, so it isn't asked again.
export async function askOnce(telemetry, status) {
  if (!status || status.asked) return;
  const yes = await confirm({
    headline: "Send an anonymous usage ping?",
    message: `Once a week, HITOP can tell its developer that it's in use: a random id made on this computer, the app version and the operating system, nothing else. It's off unless you say yes, and you can change it any time in Settings. ${PRIVACY_URL}`,
    confirmLabel: "Yes, send it",
    cancelLabel: "No",
  });
  await telemetry.set(yes).catch(() => {});
}
