// HITOP Next 7b (owner decision 2026-11-09): organization settings, members, plan & billing and
// access tokens live in the app now (webapp/src/views/org-view.js). The portal's pages for them
// send the browser there, keeping the organization id: https://hitop.rws8.tech/app/#org=<id>&section=<s>.
// The pages the app doesn't replace stay here: sign-in, home, workspaces and run history, the
// account (passkeys, sessions), and an organization's SSO setup, SCIM, admin console and audit
// anchors (settings tabs sso, scim, admin, audit).
//
// `localStorage.hitop_portal_stay = "1"` keeps the old pages (an escape hatch for operators, and
// how the portal's e2e still drives them). No imports, so it runs in Node too.

export const APP_URL = "https://hitop.rws8.tech/app/";
const STAY_KEY = "hitop_portal_stay";

/** The app section a portal path maps to, or null when the portal keeps the page. */
export function appSection(path) {
  const p = String(path || "").replace(/\/+$/, "") || "/";
  if (p === "/orgs") return { org: null, section: "general" };
  if (p === "/tokens") return { org: null, section: "tokens" };
  const m = /^\/orgs\/([^/]+)(?:\/(members|plan|settings\/(general|danger)))?$/.exec(p);
  if (!m) return null;
  const org = decodeURIComponent(m[1]);
  const section = m[2] === "members" ? "members" : m[2] === "plan" ? "billing" : "general";
  return { org, section };
}

/** The app's URL for a portal path (null when the portal keeps it). */
export function appLink(path, appUrl = APP_URL) {
  const s = appSection(path);
  if (!s) return null;
  const params = new URLSearchParams();
  if (s.org) params.set("org", s.org);
  params.set("section", s.section);
  return `${appUrl}#${params.toString()}`;
}

/** True unless this browser asked to stay on the portal's own pages. */
export function redirecting() {
  try {
    return localStorage.getItem(STAY_KEY) !== "1";
  } catch {
    return true;
  }
}
