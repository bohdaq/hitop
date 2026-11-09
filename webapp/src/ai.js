// AI in the app (owner decision 2026-11-06): the person's own Anthropic API key, kept on this
// device only (localStorage `hitop_ai`, never in the library, history or exports). Three actions:
// write tests for a response, explain a failed request or test, and a request from a curl
// command, a description or a spec snippet. Secret values are masked (••••) before anything is
// sent, and Authorization / Cookie / API key header values never leave the device.
//
// Calls go straight from the browser (or the desktop webview) to the Claude API with the
// official SDK; the default model is Claude Opus 5.5, with the server-side refusal fallback on.

import { maskSecrets } from "./features.js";
import { PATCH_SCHEMA } from "./agent.js";

const KEY = "hitop_ai";
export const AI_MODELS = [
  ["claude-opus-5-5", "Claude Opus 5.5"],
  ["claude-sonnet-5-5", "Claude Sonnet 5.5"],
  ["claude-haiku-5-5", "Claude Haiku 5.5"],
];
const MAX_BODY = 20_000;
const SECRET_HEADERS = /^(authorization|proxy-authorization|cookie|set-cookie|x-api-key|api-key|x-auth-token)$/i;

export function aiSettings() {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) || "null");
    return { key: s?.key || "", model: AI_MODELS.some(([id]) => id === s?.model) ? s.model : AI_MODELS[0][0] };
  } catch {
    return { key: "", model: AI_MODELS[0][0] };
  }
}

export function saveAiSettings(next) {
  try {
    if (!next.key) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, JSON.stringify({ key: next.key, model: next.model }));
  } catch {
    // storage blocked: for this page only
  }
}

/** What the model sees of a request and its response: secret values and credential headers masked. */
export function describeExchange({ method, url, headers, body, status, responseHeaders, response, tests, error, preRequestScript, postRequestScript }, secrets = []) {
  const mask = (text) => maskSecrets(String(text ?? ""), secrets);
  const header = ([k, v]) => `${k}: ${SECRET_HEADERS.test(k) ? "••••" : mask(v)}`;
  const cut = (text) => (text.length > MAX_BODY ? `${text.slice(0, MAX_BODY)}\n… (${text.length - MAX_BODY} more characters left out)` : text);
  const lines = [`Request: ${method} ${mask(url)}`];
  const reqHeaders = (headers || []).filter((h) => h.name && h.enabled !== false).map((h) => [h.name, h.value]);
  if (reqHeaders.length) lines.push("Request headers:", ...reqHeaders.map(header));
  if (body) lines.push("Request body:", cut(mask(body)));
  if (preRequestScript?.trim()) lines.push("Script before the request:", cut(mask(preRequestScript)));
  if (postRequestScript?.trim()) lines.push("Script after the request:", cut(mask(postRequestScript)));
  if (error) lines.push(`The request failed: ${mask(error)}`);
  if (status != null) {
    lines.push(`Response status: ${status}`);
    const rh = Object.entries(responseHeaders || {});
    if (rh.length) lines.push("Response headers:", ...rh.map(header));
    if (response) lines.push("Response body:", cut(mask(response)));
  }
  if (tests?.length) lines.push("Test results:", ...tests.map((t) => `${t.pass ? "PASS" : "FAIL"} ${t.name}${t.detail ? `: ${mask(t.detail)}` : ""}`));
  return lines.join("\n");
}

const SCRIPT_API = `HITOP scripts are JavaScript. In the script after the request: pm.test(name, fn), pm.expect(value) (chai style: .to.eql, .to.be.a, .to.have.property, .to.include, .to.be.below), pm.response.to.have.status(code), pm.response.json(), pm.response.text(), pm.response.headers.get(name), pm.response.responseTime, pm.collectionVariables.set(name, value), pm.environment.set(name, value). HITOP's own helpers also work: assert.status(code), assert.bodyContains(text), getResponseValue('path.to.value'), setContext(key, value).`;

const PROMPTS = {
  tests: {
    system: `You write the "script after" of an API request in HITOP, an API client. ${SCRIPT_API} Write a short script with 3 to 6 focused tests of this response: the status, the shape and types of the important fields, and anything the request implies (an id it asked for, a created resource). Test stable facts, not values that change between calls (timestamps, random ids). Answer with the script only: no Markdown fences, no explanation.`,
    effort: "medium",
  },
  explain: {
    system: "You help a developer with a failed API request or failing tests in HITOP, an API client. Say briefly what went wrong and the most likely fixes, most likely first. Plain text, no Markdown headings, at most 150 words. Values shown as •••• are hidden on purpose: don't ask for them.",
    effort: "low",
  },
  // The agent (HITOP Next A7): a change to the open request, answered as a patch (agent.js).
  edit: {
    system: `You change one HTTP request in HITOP, an API client, as the developer asks. ${SCRIPT_API} Answer with a patch in the JSON schema given: in "changes", give the new value of each field you change (the whole header list, the whole script) and null for fields you leave as they are. Use \${name} placeholders for secrets and per-environment values. Values shown as •••• are hidden on purpose: never write •••• into a field, keep such fields null. "summary" says in one or two sentences what changes and why.`,
    effort: "medium",
    schema: PATCH_SCHEMA,
  },
  generate: {
    system: "You turn what a developer pastes (a curl command, a description of an API call, or part of an OpenAPI spec) into one HTTP request for HITOP, an API client. Use ${name} placeholders for secrets and for values that differ per environment (a base URL used by many requests, tokens). Answer in the JSON schema given.",
    effort: "low",
    schema: {
      type: "object",
      additionalProperties: false,
      required: ["name", "method", "url", "headers", "body"],
      properties: {
        name: { type: "string", description: "A short name for the request" },
        method: { type: "string", enum: ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] },
        url: { type: "string" },
        headers: { type: "array", items: { type: "object", additionalProperties: false, required: ["name", "value"], properties: { name: { type: "string" }, value: { type: "string" } } } },
        body: { type: "string", description: "The raw body, empty for none" },
      },
    },
  },
};

let sdk = null;

/**
 * Runs one action (`tests`, `explain`, `generate`) on `input` text. `onText(delta)` gets the
 * answer as it streams (not for `generate`, whose JSON comes whole). Returns the text, or the
 * parsed request for `generate`. Throws with a readable message.
 */
export async function askAi(action, input, { onText, signal } = {}) {
  const { key, model } = aiSettings();
  if (!key) throw new Error("Add your Anthropic API key in Settings, under AI assistant.");
  const p = PROMPTS[action];
  sdk ??= await import("@anthropic-ai/sdk");
  const Anthropic = sdk.default;
  // The key belongs to the person using this browser, who typed it into Settings.
  const client = new Anthropic({ apiKey: key, dangerouslyAllowBrowser: true });
  const params = {
    model,
    max_tokens: 16000,
    system: p.system,
    messages: [{ role: "user", content: input }],
    output_config: { effort: p.effort, ...(p.schema ? { format: { type: "json_schema", schema: p.schema } } : {}) },
    // A policy decline on Opus or Sonnet is retried on a fallback model by the API.
    ...(model === "claude-haiku-5-5" ? {} : { betas: ["server-side-fallback-2026-07-01"], fallbacks: "default" }),
  };
  try {
    const stream = client.beta.messages.stream(params, { signal });
    if (onText && !p.schema) stream.on("text", (delta) => onText(delta));
    const message = await stream.finalMessage();
    if (message.stop_reason === "refusal") throw new Error("Claude declined this request. Try rewording it.");
    const text = message.content.filter((b) => b.type === "text").map((b) => b.text).join("");
    if (message.stop_reason === "max_tokens" && !text) throw new Error("The answer was too long. Try a smaller response or request.");
    if (p.schema) return JSON.parse(text);
    return text.replace(/^```[a-z]*\n?|\n?```$/g, "").trim();
  } catch (e) {
    if (e instanceof Anthropic.AuthenticationError) throw new Error("The Anthropic API key was refused. Check it in Settings, under AI assistant.");
    if (e instanceof Anthropic.PermissionDeniedError) throw new Error("This API key can't use that model. Pick another one in Settings, under AI assistant.");
    if (e instanceof Anthropic.RateLimitError) throw new Error("The Claude API is rate limiting this key. Try again in a minute.");
    if (e instanceof Anthropic.APIUserAbortError) throw e;
    if (e instanceof Anthropic.APIError) throw new Error(`The Claude API answered ${e.status ?? "with an error"}: ${e.message}`);
    throw e;
  }
}
