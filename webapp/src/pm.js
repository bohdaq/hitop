// Postman scripts in the web app. The pm.* shim is shared/pm-shim.js, the
// same file hitop-core runs in the CLI and desktop; this module loads it and
// runs scripts with it, and finds the script checks for the import report.

// The shim's two functions from its source text.
export function loadShim(text) {
  return new Function(`${text}\nreturn { create: __hitopPm, unsupported: __hitopPmUnsupported };`)();
}

// The words that load the shim (as in hitop-core's scripting.rs): other
// scripts run exactly as before.
const WORDS = /\b(pm|postman|tests|responseBody|responseCode|responseTime|environment|globals)\b/;
export const usesPostmanApi = (script) => WORDS.test(script || "");

const NAMES = ["pm", "tests", "postman", "environment", "globals", "responseBody", "responseCode", "responseTime"];

// Runs `script` with(ctx), plus `pm` and Postman's legacy names when it may
// use them. `api` is the shim's host object (see shared/pm-shim.js).
export function runScript(shim, script, ctx, api) {
  if (!usesPostmanApi(script)) {
    new Function("ctx", `with(ctx) { ${script} }`)(ctx);
    return;
  }
  const p = shim.create(api);
  new Function("ctx", ...NAMES, `with(ctx) { ${script} }`)(ctx, ...NAMES.map((n) => p[n]));
  p.finish();
}

// `${name}` replaced where `variables` has a value; the rest left for a
// script to set.
export function fillVariables(text, variables) {
  if (typeof text !== "string") return text;
  return text.replace(/\$\{([^}]+)\}/g, (whole, name) => (Object.prototype.hasOwnProperty.call(variables, name.trim()) ? String(variables[name.trim()]) : whole));
}

// The `prerequest` and `test` scripts of an `event` list, joined exactly as
// hitop-core's importer joins them (postman.rs `event_scripts`).
function eventScripts(events) {
  const script = (listen) =>
    (Array.isArray(events) ? events : [])
      .filter((e) => e && e.listen === listen && e.disabled !== true)
      .map((e) => {
        const exec = e.script?.exec;
        if (Array.isArray(exec)) return exec.filter((l) => typeof l === "string").join("\n");
        return typeof exec === "string" ? exec : "";
      })
      .join("\n");
  return [script("prerequest"), script("test")];
}

// Every script in a Postman collection: the collection's, folders' and requests'.
export function postmanScripts(collection) {
  const out = new Set();
  const walk = (node) => {
    for (const s of eventScripts(node?.event)) if (s.trim()) out.add(s);
    for (const item of Array.isArray(node?.item) ? node.item : []) walk(item);
  };
  walk(collection);
  return [...out];
}

export const isPostmanCollection = (v) => Boolean(v && typeof v === "object" && !Array.isArray(v) && v.info && typeof v.info === "object" && Array.isArray(v.item));

// What hitop-wasm's importPostman takes: each script's unsupported calls.
export function scriptScans(shim, collection) {
  const scans = {};
  for (const code of postmanScripts(collection)) scans[code] = shim.unsupported(code).map((c) => [c.line, c.call, c.text]);
  return scans;
}

// The web app has no collection level auth: a request without its own gets
// the collection's.
export function forWebApp(collection) {
  const { auth, ...rest } = collection;
  return { ...rest, requests: (collection.requests || []).map((r) => (r.auth || !auth ? r : { ...r, auth })) };
}

// An import report issue as one line of UI text.
export function issueText(issue) {
  const where = String(issue.location).replace("(pre-request script)", "(script before)").replace("(post-request script)", "(script after)");
  return `${where}${issue.line ? `, line ${issue.line}` : ""}: ${issue.what}`;
}
