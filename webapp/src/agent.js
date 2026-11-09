// The agent's edits (HITOP Next A7): the model answers with a patch of the open request, never
// free text that we paste in. A patch names only the fields it changes; anything else is refused.
// Values shown to the model as •••• (secrets, credential headers) are never written back: a
// changed field that still contains one is dropped, and a header that kept •••• keeps its value.
// Pure, unit-checked in tests/unit-ux.mjs.

export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const FIELDS = ["method", "url", "headers", "body", "preRequestScript", "postRequestScript"];
const MASK = "••••";

/** The JSON schema the model answers in (every field present; null = unchanged). */
export const PATCH_SCHEMA = {
  type: "object",
  additionalProperties: false,
  required: ["summary", "changes"],
  properties: {
    summary: { type: "string", description: "One or two sentences on what changes and why" },
    changes: {
      type: "object",
      additionalProperties: false,
      required: FIELDS,
      properties: {
        method: { type: ["string", "null"], enum: [...METHODS, null] },
        url: { type: ["string", "null"] },
        headers: {
          type: ["array", "null"],
          description: "The complete header list after the change, or null to keep the headers",
          items: { type: "object", additionalProperties: false, required: ["name", "value"], properties: { name: { type: "string" }, value: { type: "string" } } },
        },
        body: { type: ["string", "null"] },
        preRequestScript: { type: ["string", "null"], description: "The whole script before the request after the change, or null" },
        postRequestScript: { type: ["string", "null"], description: "The whole script after the request (tests) after the change, or null" },
      },
    },
  },
};

/**
 * Checks a model answer against the patch format. Returns `{ ok: true, patch: {summary, changes} }`
 * with only the fields that change (and differ from `current`), or `{ ok: false, error }`.
 * `dropped` lists fields left out because they carried a hidden value.
 */
export function validatePatch(raw, current = {}) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return { ok: false, error: "The answer isn't a patch object." };
  if (typeof raw.summary !== "string") return { ok: false, error: "The patch has no summary." };
  const ch = raw.changes;
  if (!ch || typeof ch !== "object" || Array.isArray(ch)) return { ok: false, error: "The patch has no changes object." };
  const unknown = Object.keys(ch).filter((k) => !FIELDS.includes(k));
  if (unknown.length) return { ok: false, error: `The patch changes fields a request doesn't have: ${unknown.join(", ")}.` };
  const changes = {};
  const dropped = [];
  for (const k of FIELDS) {
    const v = ch[k];
    if (v === null || v === undefined) continue;
    if (k === "method") {
      const m = String(v).toUpperCase();
      if (!METHODS.includes(m)) return { ok: false, error: `Unknown method ${v}.` };
      if (m !== String(current.method ?? "").toUpperCase()) changes.method = m;
      continue;
    }
    if (k === "headers") {
      if (!Array.isArray(v) || v.some((h) => !h || typeof h.name !== "string" || typeof h.value !== "string")) return { ok: false, error: "Headers must be a list of {name, value}." };
      const before = (current.headers || []).filter((h) => h.name && h.enabled !== false);
      const restored = v
        .filter((h) => h.name.trim())
        .map((h) => {
          if (!h.value.includes(MASK)) return { name: h.name.trim(), value: h.value };
          const same = before.find((b) => b.name.toLowerCase() === h.name.trim().toLowerCase());
          return same ? { name: same.name, value: same.value } : null;
        });
      if (restored.includes(null)) dropped.push("headers");
      const list = restored.filter(Boolean);
      const key = (hs) => JSON.stringify(hs.map((h) => [h.name, h.value]));
      if (!restored.includes(null) && key(list) !== key(before.map((h) => ({ name: h.name, value: h.value })))) changes.headers = list;
      continue;
    }
    if (typeof v !== "string") return { ok: false, error: `${k} must be text.` };
    if (v.includes(MASK)) {
      dropped.push(k);
      continue;
    }
    if (v !== (current[k] ?? "")) changes[k] = v;
  }
  return { ok: true, patch: { summary: raw.summary.trim(), changes }, dropped };
}

/** A request as text, for the before/after diff. */
export function requestText(r) {
  const lines = [`${String(r.method || "GET").toUpperCase()} ${r.url || ""}`];
  for (const h of r.headers || []) if (h.name && h.enabled !== false) lines.push(`${h.name}: ${h.value}`);
  if (r.body) lines.push("", r.body);
  if (r.preRequestScript?.trim()) lines.push("", "# Script before", r.preRequestScript.trim());
  if (r.postRequestScript?.trim()) lines.push("", "# Script after", r.postRequestScript.trim());
  return lines.join("\n");
}

/** The tab's request in the field names patches use. */
export function requestOf(tab) {
  return { method: tab.method, url: tab.url, headers: (tab.headers || []).filter((h) => h.name && h.enabled !== false), body: tab.requestBody ?? "", preRequestScript: tab.preRequestScript ?? "", postRequestScript: tab.postRequestScript ?? "" };
}

/** The request after the patch (for the diff). */
export function patched(request, changes) {
  return { ...request, ...changes };
}

/**
 * The tab fields to update for a patch (the tab calls the body `requestBody`). The model only saw
 * the enabled headers, so the tab's disabled ones are kept as they are.
 */
export function tabUpdates(changes, tab = {}) {
  const out = {};
  for (const [k, v] of Object.entries(changes)) {
    if (k === "body") out.requestBody = v;
    else if (k === "headers") out.headers = [...v.map((h) => ({ name: h.name, value: h.value })), ...(tab.headers || []).filter((h) => h.name && h.enabled === false), { name: "", value: "" }];
    else out[k] = v;
  }
  return out;
}
