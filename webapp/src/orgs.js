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
  // Only the newest load applies: one started mid sign-in (account saved, session not yet) can fail
  // after a later one succeeded, and must not clear what that one set.
  const load = (store.orgsLoad = (store.orgsLoad ?? 0) + 1);
  const latest = () => store.orgsLoad === load;
  if (!client.account()) {
    store.clearOrgs();
    return;
  }
  try {
    const { orgs } = await client.api("GET", "/v1/orgs");
    const workspaces = await client.workspaces().catch(() => []);
    if (!latest()) return;
    store.setOrgs(orgs ?? [], new Map(workspaces.filter((w) => w.org).map((w) => [w.id, w.org])), new Map(workspaces.map((w) => [w.id, w.role])));
    await Promise.all((orgs ?? []).map((o) => loadUsage(store, client, o.id)));
  } catch {
    if (latest()) store.clearOrgs();
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
