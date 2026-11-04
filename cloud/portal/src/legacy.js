// The portal's routes were #/ hashes until 2026-11: limit errors, alert mails and Stripe return
// URLs from a sync server older than this portal still carry them. Maps a hash route to its path
// once, before the router reads the address (`#sso=<code>` is not a route: the sign-in page reads it).
export function legacyHashPath(hash) {
  const m = /^#\/(.*)$/.exec(hash || "");
  if (!m) return null;
  const parts = m[1].split("/").filter(Boolean).map(decodeURIComponent);
  if (parts.length === 0) return "/";
  if (parts[0] === "orgs" && parts[1]) {
    const org = encodeURIComponent(parts[1]);
    const sub = { plan: `/orgs/${org}/plan`, admin: `/orgs/${org}/settings/admin`, sso: `/orgs/${org}/settings/sso`, audit: `/orgs/${org}/settings/audit`, scim: `/orgs/${org}/settings/scim` };
    return sub[parts[2]] || `/orgs/${org}`;
  }
  if (parts[0] === "runs") return "/workspaces";
  if (parts[0] === "workspaces" && parts[1]) return `/workspaces/${encodeURIComponent(parts[1])}/runs${parts[3] ? `/${encodeURIComponent(parts[3])}` : ""}`;
  if (parts[0] === "tokens") return "/tokens";
  if (parts[0] === "account") return "/account";
  return "/";
}

// A redirect from the old address keeps its path (/cloud/orgs/<id>), as does a bookmark of it: the
// prefix comes off first, then a hash route is mapped.
const prefixed = /^\/cloud(\/|$)/.test(location.pathname) ? location.pathname.replace(/^\/cloud\/?/, "/") : "";
const mapped = legacyHashPath(location.hash);
if (mapped) history.replaceState(null, "", mapped + location.search);
else if (prefixed) history.replaceState(null, "", prefixed + location.search + location.hash);
