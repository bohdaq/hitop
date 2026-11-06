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
