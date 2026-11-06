// The "Ask AI" dialog (ai.js): write tests for the response, explain a failure, or a request
// from a curl command, a description or a spec snippet. The answer streams into the dialog;
// nothing changes until the person applies it.
import { html, nothing } from "lit";
import { icon } from "../icons.js";
import { aiSettings, askAi, describeExchange } from "../ai.js";

const TITLES = { tests: "Write tests", explain: "Explain", generate: "Request from text" };

/** The tab's request and response as the model sees them (secrets masked). */
function exchange(app) {
  const s = app.store;
  const t = s.tab;
  const failed = t.responseType === "error" ? String(t.response || "").replace(/^Error: /, "") : null;
  return describeExchange(
    {
      method: t.method,
      url: t.url,
      headers: t.headers,
      body: t.requestBody,
      status: failed ? null : t.statusCode,
      responseHeaders: t.responseHeaders,
      response: failed ? "" : t.response,
      tests: t.tests,
      error: failed,
    },
    s.secretValuesFor(t.loadedCollectionId),
  );
}

/** Starts (or restarts) the dialog's action. */
export async function runAi(app, action = app.dialog?.action, prompt) {
  const set = (changes) => {
    if (app.dialog?.kind === "ai") app.dialog = { ...app.dialog, ...changes };
  };
  app.aiAbort?.abort();
  const controller = new AbortController();
  app.aiAbort = controller;
  set({ action, text: "", result: null, error: null, busy: true });
  try {
    const input = action === "generate" ? prompt : exchange(app);
    let text = "";
    const out = await askAi(action, input, { signal: controller.signal, onText: (d) => set({ text: (text += d) }) });
    set(action === "generate" ? { result: out, busy: false } : { text: out, busy: false });
  } catch (e) {
    if (!controller.signal.aborted) set({ error: e.message || String(e), busy: false });
  }
}

export function aiDialog(app, d) {
  const s = app.store;
  const t = s.tab;
  const hasKey = Boolean(aiSettings().key);
  const close = () => {
    app.aiAbort?.abort();
    app.closeDialog();
  };
  const pick = (action) => (action === "generate" ? (app.dialog = { ...d, action, text: "", result: null, error: null }) : runAi(app, action));
  const actions = [["tests", "Write tests", t.statusCode != null], ["explain", "Explain", t.statusCode != null || t.responseType === "error"], ["generate", "Request from text", true]];
  let body;
  if (!hasKey) {
    body = html`<p>Ask Claude to write tests for a response, explain a failure, or make a request from a curl command or a description.</p>
      <p>It uses your own Anthropic API key, kept on this device only. Add one in Settings, under AI assistant.</p>`;
  } else {
    body = html`<div class="share-tabs" role="tablist" aria-label="What to ask">
        ${actions.map(([a, label, ok]) => html`<button type="button" role="tab" class="share-tab" aria-selected=${String(d.action === a)} ?disabled=${!ok} @click=${() => pick(a)}>${label}</button>`)}
      </div>
      ${d.action === "generate"
        ? html`<lit-material-textarea id="ai-prompt" variant="outlined" label="A curl command, a description, or part of an OpenAPI spec" rows="6" autofocus></lit-material-textarea>`
        : html`<p class="hint">Claude sees this request and its response with secret values and credential headers hidden.</p>`}
      ${d.error ? html`<p class="error-text" role="alert">${d.error}</p>` : nothing}
      ${d.busy && !d.text ? html`<div class="skeleton block" aria-busy="true"></div>` : nothing}
      ${d.text
        ? d.action === "tests"
          ? html`<lit-material-code-block class="ai-answer" label="Script after" copyable>${d.text}</lit-material-code-block>`
          : html`<p class="ai-answer" aria-live="polite">${d.text}</p>`
        : nothing}
      ${d.result
        ? html`<lit-material-code-block class="ai-answer" label=${d.result.name} copyable>${`${d.result.method} ${d.result.url}\n${d.result.headers.map((h) => `${h.name}: ${h.value}`).join("\n")}${d.result.body ? `\n\n${d.result.body}` : ""}`}</lit-material-code-block>`
        : nothing}`;
  }
  const apply =
    d.action === "tests" && d.text && !d.busy
      ? html`<lit-material-button variant="filled" @click=${() => {
          s.updateTab({ postRequestScript: [t.postRequestScript?.trim(), d.text].filter(Boolean).join("\n\n") });
          close();
          app.toast("Added to Script after. Send the request to run the tests.");
        }}>${icon("plus")} Add to Script after</lit-material-button>`
      : d.action === "generate" && d.result
        ? html`<lit-material-button variant="filled" @click=${() => {
            const r = d.result;
            if (t.url) s.addTab?.();
            s.updateTab({ title: r.name, method: r.method, url: r.url, headers: [...r.headers, { name: "", value: "" }], requestBody: r.body });
            close();
          }}>Open as a request</lit-material-button>`
        : d.action === "generate" && hasKey
          ? html`<lit-material-button variant="filled" ?disabled=${d.busy} @click=${() => runAi(app, "generate", app.querySelector("#ai-prompt")?.value ?? "")}>${icon("sparkle")} Make the request</lit-material-button>`
          : nothing;
  return {
    headline: hasKey ? TITLES[d.action] ?? "Ask AI" : "Ask AI",
    body,
    footer: html`<div slot="actions" class="dialog-actions">
      ${!hasKey ? html`<lit-material-button variant="filled" @click=${() => (close(), app.go("settings"))}>Open Settings</lit-material-button>` : nothing}
      <lit-material-button variant="text" @click=${close}>Close</lit-material-button>
      ${apply}
    </div>`,
  };
}
