// HITOP Cloud organizations in the app: the top bar's switcher (hitop-app.js orgSelect) and what
// it filters (store.js visibleCollections, visibleEnvironments). Loaded when signed in.

const PLAN_NAMES = { free: "Free", pro: "Pro", team: "Team", business: "Business", enterprise: "Enterprise" };

/** "Personal · Free", "Acme · Team". */
export const orgLabel = (o) => `${o.personal ? "Personal" : o.name} · ${PLAN_NAMES[o.plan] ?? String(o.plan ?? "").replace(/^./, (x) => x.toUpperCase())}`;

/**
 * The account's orgs and its shared workspaces' orgs into the store; none signed out (or from a
 * server without orgs). `client`: the sync.js SyncClient.
 */
export async function loadOrgs(store, client) {
  if (!client.account()) {
    store.clearOrgs();
    return;
  }
  try {
    const { orgs } = await client.api("GET", "/v1/orgs");
    const workspaces = await client.workspaces().catch(() => []);
    store.setOrgs(orgs ?? [], new Map(workspaces.filter((w) => w.org).map((w) => [w.id, w.org])));
    await Promise.all((orgs ?? []).map((o) => loadUsage(store, client, o.id)));
  } catch {
    store.clearOrgs();
  }
}

/**
 * U49: one org's usage and limits (`GET /v1/orgs/:org/usage`) into `store.orgUsage`, for the title
 * bar's warning and the Share dialog; forgotten when it can't be read (nothing is claimed then).
 */
export async function loadUsage(store, client, org) {
  if (!org || !client?.account?.()) return;
  const usage = await client.api("GET", `/v1/orgs/${encodeURIComponent(org)}/usage`).catch(() => null);
  store.setOrgUsage(org, usage);
}
