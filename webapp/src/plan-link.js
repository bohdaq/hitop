// The plan link of a refused create on HITOP Cloud (a 402 `{error: "limit", upgradeUrl}` from the
// sync server, spec/CLOUD_SERVICE.md section 8): only an https page (a server can't make the Sync
// page open anything else, such as a javascript: URL), http only for localhost. The Account page shows
// it on the web only, never in the desktop build (App Store: no purchase links; the server's
// message still names the plan). No imports, so unit.mjs runs it in Node.
export function planLink(e) {
  if (!e || e.status !== 402 || e.body?.error !== "limit") return null;
  try {
    const u = new URL(String(e.body.upgradeUrl || ""));
    const ok = u.protocol === "https:" || (u.protocol === "http:" && ["localhost", "127.0.0.1"].includes(u.hostname));
    return ok ? u.href : null;
  } catch {
    return null;
  }
}

// ST1: a server's text without where to change or buy a plan, for the desktop's store builds (no
// purchase call to action, App Store 3.1.1/3.1.3): the sync server ends a 402 `limit` message with
// "See or change the plan at <portal>/orgs/<org>/plan", run notes name that page too, and a paid
// feature on Free (mocks, monitors) says "<feature> come with the Pro, Team, ... plans".
const PLAN_LIST = "(?:Pro|Team|Business|Enterprise)";
const COME_WITH = new RegExp(`\\bcome with the ${PLAN_LIST}(?:, ${PLAN_LIST})*(?:,? and ${PLAN_LIST})? plans?\\b`, "gi");
export function withoutPlanLink(text) {
  return String(text ?? "")
    .replace(/\s*See or change the plan at \S+/gi, "")
    .replace(COME_WITH, "aren't part of this plan")
    .replace(/,?\s*https?:\/\/[^\s)]*\/(?:plan|pricing|billing|checkout)\b[^\s)]*/gi, "")
    .replace(/\s+([.,;)])/g, "$1")
    .trim();
}

// ST1: a sync server's JSON answer as a store build may show it: no `upgradeUrl`, and an error's
// `message` without the plan link. Anything else (and an answer with nothing to take out) is
// returned as is, the same object.
export function storeSafeAnswer(json, status) {
  if (!json || typeof json !== "object" || Array.isArray(json)) return json;
  const message = status >= 400 && typeof json.message === "string" ? withoutPlanLink(json.message) : json.message;
  if (!("upgradeUrl" in json) && message === json.message) return json;
  const out = { ...json, message };
  if (!("message" in json)) delete out.message;
  delete out.upgradeUrl;
  return out;
}
