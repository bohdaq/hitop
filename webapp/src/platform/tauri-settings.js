// The desktop app's Settings page and its first-run question: the opt-in
// usage ping (desktop/src-tauri/src/commands.rs `telemetry_status`,
// `telemetry_set`; hitop-core's telemetry module) and the secret manager
// settings (`secret_manager_settings`, `secret_manager_settings_set`;
// hitop-core's secret_managers::Settings). Only the Tauri adapter imports
// this file, so the web build never contains it. In desktop builds without
// the ping (Mac App Store) `telemetry_status` fails: the page then has only
// the secret managers and the question doesn't appear.
import { LitElement, html, nothing } from "lit";
import { confirm } from "@lit-material/confirm";

export const PRIVACY_URL = "https://hitop.rws8.tech/privacy.html";

const describe = (status) => `POST ${status.endpoint}\n${JSON.stringify(status.ping, null, 2)}`;
const day = (secs) => new Date(secs * 1000).toISOString().slice(0, 10);

// The secret manager fields: [environment variable, settings key (none for
// a secret: the keychain holds those), label, placeholder].
const MANAGERS = [
  {
    title: "Vault or OpenBao",
    fields: [
      ["VAULT_ADDR", "vaultAddr", "Address (VAULT_ADDR)", "https://vault.example.com:8200"],
      ["VAULT_NAMESPACE", "vaultNamespace", "Namespace (VAULT_NAMESPACE)", "optional"],
      ["VAULT_TOKEN", null, "Token (VAULT_TOKEN)", ""],
    ],
  },
  {
    title: "AWS Secrets Manager",
    fields: [
      ["AWS_REGION", "awsRegion", "Region (AWS_REGION)", "eu-central-1"],
      ["AWS_PROFILE", "awsProfile", "Profile in ~/.aws (AWS_PROFILE)", "default"],
      ["AWS_ACCESS_KEY_ID", null, "Access key id (AWS_ACCESS_KEY_ID)", ""],
      ["AWS_SECRET_ACCESS_KEY", null, "Secret access key (AWS_SECRET_ACCESS_KEY)", ""],
      ["AWS_SESSION_TOKEN", null, "Session token (AWS_SESSION_TOKEN)", ""],
    ],
  },
  {
    title: "Azure Key Vault",
    fields: [
      ["AZURE_TENANT_ID", "azureTenantId", "Tenant id (AZURE_TENANT_ID)", ""],
      ["AZURE_CLIENT_ID", "azureClientId", "Client id (AZURE_CLIENT_ID)", ""],
      ["AZURE_CLIENT_SECRET", null, "Client secret (AZURE_CLIENT_SECRET)", ""],
    ],
  },
  {
    title: "Programs",
    fields: [
      ["HITOP_OP_BIN", "opPath", "1Password CLI (op)", "op, found on PATH"],
      ["HITOP_AZ_BIN", "azPath", "Azure CLI (az)", "az, found on PATH"],
      ["HITOP_AWS_BIN", "awsPath", "AWS CLI (aws)", "aws, found on PATH"],
    ],
  },
];

// Variables that come from one place (hitop-core secret_managers::GROUPS):
// when the app's environment sets one, the saved ones of its group are unused.
const GROUPS = [
  ["VAULT_ADDR", "BAO_ADDR"],
  ["VAULT_TOKEN", "BAO_TOKEN"],
  ["VAULT_NAMESPACE", "BAO_NAMESPACE"],
  ["AWS_REGION", "AWS_DEFAULT_REGION"],
  ["AWS_PROFILE", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN"],
  ["AZURE_TENANT_ID", "AZURE_CLIENT_ID", "AZURE_CLIENT_SECRET"],
];

const fieldId = (name) => `sm-${name.toLowerCase().replace(/_/g, "-")}`;

class HitopSettings extends LitElement {
  static properties = {
    telemetry: { attribute: false },
    managers: { attribute: false },
    status: { state: true },
    error: { state: true },
    sm: { state: true },
    smError: { state: true },
    smNote: { state: true },
    smBusy: { state: true },
  };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    if (this.telemetry) {
      this.telemetry.status().then(
        (s) => (this.status = s),
        (e) => (this.error = String(e)),
      );
    }
    this.managers?.load().then(
      (s) => (this.sm = s),
      (e) => (this.smError = String(e)),
    );
  }

  async toggle(on) {
    try {
      this.status = await this.telemetry.set(on);
    } catch (e) {
      this.error = String(e);
    }
  }

  shadowed(name) {
    const shell = this.sm?.shell ?? [];
    const group = GROUPS.find((g) => g.includes(name)) ?? [name];
    return group.find((n) => shell.includes(n));
  }

  // Saves every non-secret field, and the secrets typed in (blank leaves a
  // saved one as it is); `remove` names a secret to take out of the keychain.
  async saveManagers(remove) {
    const settings = { ...(this.sm?.settings ?? {}) };
    const secrets = {};
    for (const group of MANAGERS) {
      for (const [name, key] of group.fields) {
        const value = this.querySelector(`#${fieldId(name)}`)?.value ?? "";
        if (key) settings[key] = value.trim() || undefined;
        else if (value.trim()) secrets[name] = value;
      }
    }
    if (remove) secrets[remove] = null;
    this.smBusy = true;
    this.smError = null;
    try {
      this.sm = await this.managers.save(settings, secrets);
      for (const group of MANAGERS) for (const [name, key] of group.fields) if (!key) this.querySelector(`#${fieldId(name)}`).value = "";
      this.smNote = remove ? `Removed ${remove} from your system keychain.` : "Saved. Secret values are in your system keychain.";
    } catch (e) {
      this.smError = String(e);
    } finally {
      this.smBusy = false;
    }
  }

  renderField([name, key, label, placeholder]) {
    const sm = this.sm;
    const shell = this.shadowed(name);
    const note = shell ? `${shell} is set in this app's environment: that value wins` : "";
    if (key) {
      return html`<lit-material-text-field id=${fieldId(name)} variant="outlined" label=${label} placeholder=${placeholder} .value=${sm.settings?.[key] ?? ""} supporting-text=${note}></lit-material-text-field>`;
    }
    const saved = Boolean(sm.secrets?.[name]);
    return html`<div class="sm-secret">
      <lit-material-text-field id=${fieldId(name)} type="password" variant="outlined" label=${label} placeholder=${saved ? "saved: type to replace" : ""}
        supporting-text=${note || (saved ? "Saved in your system keychain" : "Not saved")}></lit-material-text-field>
      ${saved ? html`<lit-material-button variant="text" ?disabled=${!!this.smBusy} @click=${() => this.saveManagers(name)}>Remove ${name}</lit-material-button>` : nothing}
    </div>`;
  }

  renderManagers() {
    const sm = this.sm;
    return html`<section aria-label="Secret managers">
      <h2>Secret managers</h2>
      <p class="page-lead">
        For \${secret:vault|aws|azure|op:...} references. An app opened from the Dock or a launcher doesn't see your shell's variables, so set them here.
        A variable this app's environment already sets wins (a whole group at a time: a shell AWS_PROFILE means no saved AWS keys). Tokens and keys go to your
        system keychain, never to a file; the rest goes to ${sm?.path ?? "secret-managers.json in hitop's settings folder"}. The hitop CLI reads both too.
      </p>
      ${sm
        ? html`<form class="sm-form" @submit=${(e) => (e.preventDefault(), this.saveManagers())}>
            ${MANAGERS.map((g) => html`<fieldset class="sm-group"><legend>${g.title}</legend>${g.fields.map((f) => this.renderField(f))}</fieldset>`)}
            ${sm.keychainError ? html`<p class="hint">The system keychain isn't available: ${sm.keychainError}</p>` : nothing}
            <div class="sm-actions">
              <lit-material-button variant="filled" type="submit" ?disabled=${!!this.smBusy}>Save secret manager settings</lit-material-button>
              ${this.smNote ? html`<span class="hint" role="status">${this.smNote}</span>` : nothing}
            </div>
          </form>`
        : nothing}
      ${this.smError ? html`<p class="hint" role="alert">${this.smError}</p>` : sm ? nothing : html`<p class="hint">Loading…</p>`}
    </section>`;
  }

  render() {
    const s = this.status;
    // Sections of the shared Settings page (views/settings-view.js).
    return html`
        ${this.telemetry
          ? html`<h2>Usage ping</h2>
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
                : html`<p class="hint">${this.error ?? "Loading…"}</p>`}`
          : nothing}
        ${this.managers ? this.renderManagers() : nothing}`;
  }
}

customElements.define("hitop-settings", HitopSettings);

// `telemetry`: { status(), set(on) } and `managers`: { load(), save(settings,
// secrets) } from the adapter. Returns the desktop's sections of the Settings
// page (the usage ping part only when this build has the ping) and the ping's status.
export async function settingsSections(telemetry, managers) {
  let status = null;
  try {
    status = await telemetry.status();
  } catch {
    telemetry = null;
  }
  const sections = () => html`<hitop-settings class="desktop-settings" .telemetry=${telemetry} .managers=${managers}></hitop-settings>`;
  return { sections, status };
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
