// The agent (HITOP Next A7, concept screen 3): Ask AI in a side sheet beside the request. It can
// change the request, write tests, explain a failure, or make a request from text. Changes
// arrive as a patch (agent.js), shown as a diff and, when they touch the tests, dry-run against
// the last response; nothing changes until the person applies it. Secret values are masked
// before anything is sent (ai.js describeExchange).
import { html, nothing } from "lit";
import { icon } from "../icons.js";
import { aiSettings, askAi, describeExchange, saveAiSettings } from "../ai.js";
import { patched, requestOf, requestText, tabUpdates, validatePatch } from "../agent.js";
import { testList } from "./request-view.js";

const ACTIONS = [
  ["edit", "Change this request"],
  ["tests", "Write tests"],
  ["explain", "Explain"],
  ["generate", "Request from text"],
];

/** The tab's request and response as the model sees them (secrets masked). */
function exchange(app, withScripts = false) {
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
      ...(withScripts ? { preRequestScript: t.preRequestScript, postRequestScript: t.postRequestScript } : {}),
    },
    s.secretValuesFor(t.loadedCollectionId),
  );
}

/**
 * U14: from an explanation to a fix: the explanation goes back as the instruction of "Change this
 * request", so the fix arrives as a patch (diff, dry run, Apply / Run with checks / Discard) and
 * the prompt stays visible and editable on that tab.
 */
export function fixPrompt(explanation) {
  return `Fix this request so that it succeeds and its tests pass. Change only what the failure needs. The failure, as explained before:\n${String(explanation ?? "").trim()}`;
}

/** A checked patch on the sheet, with its dry run when it changes the tests. */
async function showPatch(app, raw) {
  const s = app.store;
  const before = requestOf(s.tab);
  const v = validatePatch(raw, before);
  if (!v.ok) return app.setAgent({ error: `The answer couldn't be used: ${v.error}`, busy: false });
  app.setAgent({ patch: v.patch, dropped: v.dropped, before, busy: false, dryRun: null });
  if (v.patch.changes.postRequestScript !== undefined && s.tab.statusCode != null) {
    const tests = await s.dryRunChecks(v.patch.changes.postRequestScript);
    app.setAgent({ dryRun: tests });
  }
}

/** Starts (or restarts) an action. */
export async function runAi(app, action = app.agent?.action, prompt) {
  const set = (changes) => app.setAgent(changes);
  app.aiAbort?.abort();
  const controller = new AbortController();
  app.aiAbort = controller;
  set({ action, text: "", result: null, patch: null, dryRun: null, dropped: [], error: null, busy: true, ...(prompt !== undefined ? { prompt } : {}) });
  try {
    if (action === "edit") {
      const input = `${exchange(app, true)}\n\nWhat to change: ${prompt ?? ""}`;
      const out = await askAi("edit", input, { signal: controller.signal });
      await showPatch(app, out);
      return;
    }
    const input = action === "generate" ? prompt : exchange(app);
    let text = "";
    const out = await askAi(action, input, { signal: controller.signal, onText: (d) => set({ text: (text += d) }) });
    if (action === "generate") set({ result: out, busy: false });
    else if (action === "tests") {
      set({ text: out });
      const t = app.store.tab;
      await showPatch(app, { summary: "New tests in the script after the request.", changes: { postRequestScript: [t.postRequestScript?.trim(), out].filter(Boolean).join("\n\n") } });
    } else set({ text: out, busy: false });
  } catch (e) {
    if (!controller.signal.aborted) set({ error: e.message || String(e), busy: false });
  }
}

function patchView(app, a) {
  const s = app.store;
  const { patch, before } = a;
  const changed = Object.keys(patch.changes);
  const passed = a.dryRun?.filter((t) => t.pass).length ?? 0;
  return html`<section class="agent-patch" aria-label="Proposed change">
    <p class="agent-summary">${patch.summary}</p>
    ${changed.length
      ? html`<lit-material-diff class="agent-diff" label="Proposed change" original-label="Now" modified-label="After" .original=${requestText(before)} .modified=${requestText(patched(before, patch.changes))}></lit-material-diff>`
      : html`<p class="hint">The answer changes nothing in this request.</p>`}
    ${a.dropped?.length ? html`<p class="hint">Left out: ${a.dropped.join(", ")} (they held a hidden value).</p>` : nothing}
    ${a.dryRun
      ? html`<p class=${`agent-dry ${passed === a.dryRun.length ? "ok" : "bad"}`} role="status">${icon(passed === a.dryRun.length ? "check-circle" : "warning-circle")} Dry run against the last response: ${passed} of ${a.dryRun.length} pass</p>
          ${passed < a.dryRun.length ? testList(a.dryRun) : nothing}`
      : patch.changes.postRequestScript !== undefined && s.tab.statusCode == null
        ? html`<p class="hint">Send the request once to dry-run these tests.</p>`
        : nothing}
    ${changed.length
      ? html`<div class="agent-actions">
          <lit-material-button variant="filled" @click=${() => applyPatch(app, false)}>Apply</lit-material-button>
          <lit-material-button variant="outlined" @click=${() => applyPatch(app, true)}>${icon("play")} Run with checks</lit-material-button>
          <lit-material-button variant="text" @click=${() => app.setAgent({ patch: null, dryRun: null, text: "" })}>Discard</lit-material-button>
        </div>`
      : nothing}
  </section>`;
}

function applyPatch(app, run) {
  const s = app.store;
  const { patch } = app.agent;
  s.updateTab(tabUpdates(patch.changes, s.tab));
  app.setAgent({ patch: null, dryRun: null, text: "" });
  app.toast(run ? "Applied. Sending the request with its checks." : "Applied to the request. Save it to keep the change.");
  if (run) s.send();
}

/** The sheet's content (hitop-app renders the sheet). */
export function agentPanel(app) {
  const s = app.store;
  const t = s.tab;
  const a = app.agent ?? { action: "edit" };
  const hasKey = Boolean(aiSettings().key);
  if (!hasKey) {
    return html`<div class="agent-body">
      <p>The agent changes the open request, writes tests for a response, explains a failure, or makes a request from a curl command or a description.</p>
      <p>It uses your own Anthropic API key, kept on this device only (also in Settings, under AI assistant).</p>
      <form class="agent-key" @submit=${(e) => {
        // UX2: the key is taken here, without leaving for Settings.
        e.preventDefault();
        const key = String(app.querySelector("#agent-key")?.value ?? "").trim();
        if (!key) return;
        saveAiSettings({ ...aiSettings(), key });
        app.setAgent({});
        requestAnimationFrame(() => app.querySelector("#ai-prompt")?.focus());
      }}>
        <lit-material-text-field id="agent-key" variant="outlined" type="password" label="Anthropic API key" placeholder="sk-ant-…" autocomplete="off"></lit-material-text-field>
        <lit-material-button variant="filled" type="submit">Save key</lit-material-button>
      </form>
    </div>`;
  }
  const can = { edit: Boolean(t?.url), tests: t?.statusCode != null, explain: t?.statusCode != null || t?.responseType === "error", generate: true };
  const pick = (action) => (action === "edit" || action === "generate" ? app.setAgent({ action, text: "", result: null, patch: null, dryRun: null, error: null }) : runAi(app, action));
  return html`<div class="agent-body">
    <div class="share-tabs" role="tablist" aria-label="What to ask">
      ${ACTIONS.map(([id, label]) => html`<button type="button" role="tab" class="share-tab" aria-selected=${String(a.action === id)} ?disabled=${!can[id]} @click=${() => pick(id)}>${label}</button>`)}
    </div>
    ${a.action === "edit" || a.action === "generate"
      ? html`<lit-material-textarea id="ai-prompt" variant="outlined" rows="4"
            label=${a.action === "edit" ? "What should change? (\"add a check that total is positive\", \"send JSON with a Bearer token\")" : "A curl command, a description, or part of an OpenAPI spec"}
            .value=${a.prompt ?? ""}></lit-material-textarea>
          <lit-material-button variant="filled" ?disabled=${a.busy} @click=${() => runAi(app, a.action, app.querySelector("#ai-prompt")?.value ?? "")}>${icon("sparkle")} ${a.action === "edit" ? "Propose a change" : "Make the request"}</lit-material-button>`
      : html`<p class="hint">The agent sees this request and its response with secret values and credential headers hidden.</p>`}
    ${a.error ? html`<p class="error-text" role="alert">${a.error}</p>` : nothing}
    ${a.busy && !a.text ? html`<div class="skeleton block" aria-busy="true"></div>` : nothing}
    ${a.text && a.action === "explain" ? html`<p class="ai-answer" aria-live="polite">${a.text}</p>` : nothing}
    ${a.text && a.action === "explain" && !a.busy && can.edit
      ? html`<lit-material-button variant="outlined" class="agent-fix" @click=${() => runAi(app, "edit", fixPrompt(a.text))}>${icon("sparkle")} Propose a fix</lit-material-button>`
      : nothing}
    ${a.text && a.action === "tests" && !a.patch ? html`<lit-material-code-block class="ai-answer" label="Script after" copyable>${a.text}</lit-material-code-block>` : nothing}
    ${a.patch ? patchView(app, a) : nothing}
    ${a.result
      ? html`<lit-material-code-block class="ai-answer" label=${a.result.name} copyable>${`${a.result.method} ${a.result.url}\n${a.result.headers.map((h) => `${h.name}: ${h.value}`).join("\n")}${a.result.body ? `\n\n${a.result.body}` : ""}`}</lit-material-code-block>
          <lit-material-button variant="filled" @click=${() => {
            const r = a.result;
            if (t.url) s.addTab?.();
            s.updateTab({ title: r.name, method: r.method, url: r.url, headers: [...r.headers, { name: "", value: "" }], requestBody: r.body });
            app.setAgent({ result: null });
            app.closeAgent();
          }}>Open as a request</lit-material-button>`
      : nothing}
    <p class="hint agent-note">Secrets stay masked. Your key, your model: only the prompt leaves this device. Other agents get the same tools over MCP (<code>hitop mcp</code>).</p>
  </div>`;
}
