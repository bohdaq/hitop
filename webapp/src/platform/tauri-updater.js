// Self-update of the desktop app's GitHub Release builds (macOS, Windows, Linux
// AppImage): desktop/src-tauri/src/updates.rs, Cargo feature `updater`. Rust's
// `updater_status` answers {version} only in those builds; the Mac App Store
// build (store policy) and the AppImage-less Linux packages answer an error, and
// then nothing here runs. Only the Tauri adapter imports this file, so the web
// build never contains it (scripts/check-no-telemetry.mjs checks that).
//
// The official updater plugin does the work: `plugin:updater|check` reads the
// latest GitHub Release's latest.json (pre-releases never count as latest),
// `plugin:updater|download_and_install` downloads and verifies the signature
// against the public key in tauri.github.conf.json, `plugin:process|restart`
// relaunches. Checks run at startup and then every 24 hours while the switch in
// Settings → Updates is on (localStorage `hitop_updates` {auto}). Errors are
// quiet: one line in Settings, never a dialog.
import { LitElement, html, nothing } from "lit";

const KEY = "hitop_updates";
const DAY = 24 * 60 * 60 * 1000;
// How often the app looks at the clock for the next daily check.
const TICK = 60 * 60 * 1000;

function loadPrefs() {
  try {
    const p = JSON.parse(localStorage.getItem(KEY) || "{}");
    return { auto: p.auto !== false };
  } catch {
    return { auto: true };
  }
}

function savePrefs(prefs) {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // not saved: the switch still applies until the app closes
  }
}

const oneLine = (e) => String(e?.message ?? e).split("\n")[0].slice(0, 200);
const clock = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
const mb = (n) => `${(n / 1024 / 1024).toFixed(1)} MB`;

// The plugins' commands, through the adapter's invoke (window.__TAURI_INTERNALS__).
export function updaterApi(invoke) {
  const internals = () => window.__TAURI_INTERNALS__;
  return {
    status: () => invoke("updater_status"),
    // null when there is nothing newer; else {rid, version, currentVersion, date, body}.
    check: () => invoke("plugin:updater|check", {}),
    // Events {event: Started {contentLength} | Progress {chunkLength} | Finished} on a channel,
    // in order (the same channel framing as ws_connect).
    install(rid, onEvent) {
      const early = new Map();
      let next = 0;
      const callback = internals().transformCallback((payload) => {
        if (!payload) return;
        if ("end" in payload) return internals().unregisterCallback?.(callback);
        early.set(payload.index, payload.message);
        while (early.has(next)) {
          const m = early.get(next);
          early.delete(next);
          next += 1;
          onEvent(m);
        }
      });
      return invoke("plugin:updater|download_and_install", { rid, onEvent: `__CHANNEL__:${callback}` });
    },
    restart: () => invoke("plugin:process|restart"),
  };
}

// One per window: the state the banner and the Settings section draw.
//   status   idle | checking | current | available | downloading | restarting | failed
//   error    the last check's or install's error, one line (null when the last one worked)
export class Updater extends EventTarget {
  constructor(api, version) {
    super();
    this.api = api;
    this.version = version;
    this.prefs = loadPrefs();
    this.status = "idle";
    this.update = null;
    this.error = null;
    this.lastCheck = null;
    this.progress = null;
    this.dismissed = null;
    this.timer = null;
    // Saves the app's pending storage writes before an installer may quit it (set by start()).
    this.beforeInstall = async () => {};
  }

  changed() {
    this.dispatchEvent(new Event("change"));
  }

  get auto() {
    return this.prefs.auto;
  }

  setAuto(on) {
    this.prefs = { ...this.prefs, auto: Boolean(on) };
    savePrefs(this.prefs);
    this.schedule();
    this.changed();
    if (on && (!this.lastCheck || Date.now() - this.lastCheck >= DAY)) this.check();
  }

  // At startup, then every 24 hours while automatic checks are on.
  start(beforeInstall) {
    if (beforeInstall) this.beforeInstall = beforeInstall;
    this.schedule();
    if (this.auto) this.check();
  }

  schedule() {
    clearInterval(this.timer);
    this.timer = null;
    if (!this.auto) return;
    this.timer = setInterval(() => {
      if (this.auto && this.status !== "checking" && this.lastCheck && Date.now() - this.lastCheck >= DAY) this.check();
    }, TICK);
  }

  async check() {
    if (["checking", "downloading", "restarting"].includes(this.status)) return;
    this.status = "checking";
    this.changed();
    try {
      const found = await this.api.check();
      this.lastCheck = Date.now();
      this.error = null;
      this.update = found ? { rid: found.rid, version: found.version } : null;
      this.status = found ? "available" : "current";
    } catch (e) {
      this.lastCheck = Date.now();
      this.error = `Couldn't check for updates: ${oneLine(e)}`;
      this.status = this.update ? "available" : "failed";
    }
    this.changed();
  }

  // "Later": the banner stays away for this version until the app starts again.
  later() {
    this.dismissed = this.update?.version ?? null;
    this.changed();
  }

  get bannerShown() {
    return Boolean(this.update) && (this.status === "downloading" || this.status === "restarting" || (this.status === "available" && this.dismissed !== this.update.version));
  }

  async install() {
    if (!this.update || this.status === "downloading" || this.status === "restarting") return;
    this.status = "downloading";
    this.error = null;
    this.progress = { received: 0, total: null };
    this.changed();
    try {
      await this.beforeInstall();
      await this.api.install(this.update.rid, (e) => {
        if (e?.event === "Started") this.progress = { received: 0, total: e.data?.contentLength ?? null };
        else if (e?.event === "Progress") this.progress = { ...this.progress, received: this.progress.received + (e.data?.chunkLength ?? 0) };
        else if (e?.event === "Finished") this.status = "restarting";
        this.changed();
      });
      this.status = "restarting";
      this.changed();
      // macOS and Linux: the new version is in place; start it. (Windows: the installer already quit the app.)
      await this.api.restart();
    } catch (e) {
      this.error = `The update didn't install: ${oneLine(e)}`;
      this.status = "available";
      this.dismissed = this.update.version;
      this.changed();
    }
  }
}

const progressText = (p) => (p?.total ? `${mb(p.received)} of ${mb(p.total)}` : p?.received ? mb(p.received) : "");

// The quiet bar at the bottom of the window: "HITOP 1.3.0 is available · Restart to update · Later".
class HitopUpdateBar extends LitElement {
  static properties = { updater: { attribute: false } };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.onChange = () => this.requestUpdate();
    this.updater.addEventListener("change", this.onChange);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.updater.removeEventListener("change", this.onChange);
  }

  render() {
    const u = this.updater;
    if (!u.bannerShown) return nothing;
    const busy = u.status === "downloading" || u.status === "restarting";
    const p = u.progress;
    return html`<div class="update-bar" role="status" aria-label="Update" style="position: fixed; right: 16px; bottom: 16px; z-index: 30; max-width: calc(100vw - 32px); display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; padding: 8px 8px 8px 16px; border-radius: 12px; border: 1px solid var(--md-sys-color-outline-variant); background: var(--md-sys-color-surface-container-high); color: var(--md-sys-color-on-surface); box-shadow: 0 4px 16px rgb(0 0 0 / 0.35);">
      ${busy
        ? html`<span>${u.status === "restarting" ? `Installing HITOP ${u.update.version}…` : `Downloading HITOP ${u.update.version}… ${progressText(p)}`}</span>
            <progress aria-label="Update download" max=${p?.total || 1} .value=${p?.total ? p.received : u.status === "restarting" ? 1 : 0} style="width: 8rem;"></progress>`
        : html`<span>HITOP ${u.update.version} is available</span>
            <lit-material-button variant="filled" @click=${() => u.install()}>Restart to update</lit-material-button>
            <lit-material-button variant="text" @click=${() => u.later()}>Later</lit-material-button>`}
    </div>`;
  }
}
customElements.define("hitop-update-bar", HitopUpdateBar);

// Settings → Updates.
class HitopUpdates extends LitElement {
  static properties = { updater: { attribute: false } };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.onChange = () => this.requestUpdate();
    this.updater.addEventListener("change", this.onChange);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.updater.removeEventListener("change", this.onChange);
  }

  line() {
    const u = this.updater;
    if (u.status === "checking") return "Checking for updates…";
    if (u.status === "downloading") return `Downloading HITOP ${u.update.version}… ${progressText(u.progress)}`;
    if (u.status === "restarting") return `Installing HITOP ${u.update.version}…`;
    if (u.status === "available") return `HITOP ${u.update.version} is available.`;
    if (u.status === "current") return `HITOP is up to date (checked at ${clock(u.lastCheck)}).`;
    return u.auto ? "" : "Automatic checks are off.";
  }

  render() {
    const u = this.updater;
    const busy = ["checking", "downloading", "restarting"].includes(u.status);
    const line = this.line();
    return html`<section aria-label="Updates" class="settings-section updates-section">
      <h2>Updates</h2>
      <p class="page-lead">This is HITOP ${u.version}. New versions come from GitHub Releases, signed by HITOP's developer; HITOP checks once at startup and then once a day, and asks before installing.</p>
      <label class="switch-row">
        <lit-material-switch aria-label="Check for updates automatically" ?checked=${u.auto} @change=${(e) => u.setAuto(e.target.checked)}></lit-material-switch>
        <span>Check for updates automatically</span>
      </label>
      <div style="display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px;">
        <lit-material-button variant="outlined" ?disabled=${busy} @click=${() => u.check()}>Check now</lit-material-button>
        ${u.status === "available" ? html`<lit-material-button variant="filled" @click=${() => u.install()}>Restart to update</lit-material-button>` : nothing}
      </div>
      ${line ? html`<p class="hint update-state" role="status">${line}</p>` : nothing}
      ${u.error ? html`<p class="hint update-error">${u.error}</p>` : nothing}
    </section>`;
  }
}
customElements.define("hitop-updates", HitopUpdates);

// The adapter's part: null when this build doesn't update itself (Mac App Store,
// .deb/.rpm), else { updater, section() for Settings, start(app) for onStart }.
export async function desktopUpdates(invoke) {
  const api = updaterApi(invoke);
  let status;
  try {
    status = await api.status();
  } catch {
    return null;
  }
  const updater = new Updater(api, status?.version ?? "");
  return {
    updater,
    section: () => html`<hitop-updates .updater=${updater}></hitop-updates>`,
    start(app, flush) {
      const bar = document.createElement("hitop-update-bar");
      bar.updater = updater;
      document.body.append(bar);
      updater.start(async () => {
        app?.store?.save?.();
        await flush?.();
      });
    },
  };
}
