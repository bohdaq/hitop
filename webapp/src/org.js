// What the Organization area shows (views/org-view.js), as plain data: its sections, usage meters
// with their warning levels, the plan cards, and a plan limit's answer. Pure, so tests/unit-ux.mjs
// checks it in Node.

export const PLAN_NAMES = { free: "Free", pro: "Pro", team: "Team", business: "Business", enterprise: "Enterprise" };

// Prices as the site's pricing page says them (site/src/pages/pricing.js); Stripe has the truth.
const PLANS = [
  { plan: "free", price: "$0", items: ["3 members", "3 shared collections", "100 MB"] },
  { plan: "pro", price: "$6 / month", items: ["One person", "25 shared collections", "2 GB, 10 devices"] },
  { plan: "team", price: "$10 / seat / month", items: ["Unlimited shared collections", "10 GB + 1 GB a seat", "5 free guest viewers"], perSeat: true },
  { plan: "business", price: "$25 / seat / month", items: ["Everything in Team", "SSO for your domain", "Audit export, admin console"], perSeat: true },
];

/** The left menu: Security & audit only for owners and admins of a team organization. */
export function sectionsFor(org) {
  const admin = org?.role === "owner" || org?.role === "admin";
  return [
    ["general", "General"],
    ["members", "Members"],
    ["billing", "Plan & billing"],
    ["tokens", "Access tokens & MCP"],
    ["runs", "Runs & monitors"],
    ...(admin && !org?.personal ? [["security", "Security & audit"]] : []),
  ];
}

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const bytes = (n) => (n >= GB ? `${(n / GB).toFixed(n >= 10 * GB ? 0 : 1)} GB` : `${Math.round(n / MB)} MB`);

/**
 * One meter per limited metric of `GET /v1/orgs/:org/usage`: `level` is "ok", "warn" (80% or
 * more) or "full"; a metric without a limit (null) shows its use only.
 */
export function meters(usage) {
  const u = usage?.usage ?? {};
  const l = usage?.limits ?? {};
  const rows = [
    ["Seats", "seats", u.seats, l.seats],
    ["Guest viewers", "guests", u.guestViewers ?? u.guests, l.guests ?? l.guestViewers],
    ["Shared collections", "workspaces", u.workspaces, l.workspaces],
    ["Storage", "storage", u.storageBytes, l.storageBytes, bytes],
    ["Devices", "devices", u.devices, l.devices],
    ["CI runners", "runners", u.runners, l.runners],
  ];
  return rows
    // Not on this plan at all (Free has 0 guest viewers): no meter.
    .filter(([, , used, max]) => used != null && !(max === 0 && used === 0))
    .map(([label, id, used, max, fmt = String]) => {
      const percent = max ? Math.min(100, Math.round((used / max) * 100)) : 0;
      // U49: warn before the limit, also where 80% never comes (2 of 3): one left of a count.
      const lastOne = max != null && max >= 2 && id !== "storage" && max - used === 1;
      const level = max == null ? "ok" : used >= max ? "full" : percent >= 80 || lastOne ? "warn" : "ok";
      return {
        id,
        label,
        used,
        max: max ?? null,
        percent,
        level,
        text: max == null ? `${fmt(used)} · no limit` : `${fmt(used)} of ${fmt(max)}`,
        note: level === "full" ? (id === "seats" ? "Full: add a seat to invite someone else" : "Full: new ones are refused until there is room") : level === "warn" ? (lastOne ? "Nearly full: 1 left" : "Nearly full") : "",
      };
    });
}

/**
 * U49: the meters at "warn" or "full", fullest first: what the title bar warns about before a
 * create is refused. Empty without limits (a self-hosted server) or without usage.
 */
export function usageWarnings(usage) {
  if (!usage?.limits) return [];
  const rank = { full: 0, warn: 1 };
  return meters(usage)
    .filter((m) => m.level !== "ok")
    .sort((a, b) => rank[a.level] - rank[b.level] || b.percent - a.percent);
}

/**
 * U49: before a collection is shared into `org` (its usage from `GET /v1/orgs/:org/usage`), the
 * room left for shared collections: null when there is plenty or no limit; else { level, text }.
 */
export function shareRoom(usage, org) {
  const m = usage?.limits ? meters(usage).find((x) => x.id === "workspaces") : null;
  if (!m || m.max == null || m.level === "ok") return null;
  const where = org?.personal ? "your personal account" : org?.name ?? "this organization";
  const plan = PLAN_NAMES[usage.planInForce ?? usage.plan] ?? "This plan";
  if (m.level === "full") return { level: "full", text: `All ${m.max} shared collections of ${where} (${plan}) are used: sharing this one will be refused until one is removed or the plan grows.` };
  const left = m.max - m.used;
  return { level: "warn", text: `${m.used} of ${m.max} shared collections of ${where} (${plan}) are used: sharing this one ${left === 1 ? "takes the last one" : `leaves ${left - 1}`}.` };
}

/**
 * The plan cards for an org: what it can buy (`billing.offers`, monthly when sold, else annual),
 * the current plan marked. A personal org buys Pro; a team organization Team or Business.
 */
export function planCards(org, billing, inForce = org?.plan ?? "free") {
  const offers = billing?.offers ?? [];
  const forOrg = PLANS.filter((p) => (org?.personal ? ["free", "pro"].includes(p.plan) : p.plan !== "pro"));
  return forOrg.map((p) => {
    const sold = offers.filter((o) => o.plan === p.plan);
    const offer = sold.find((o) => o.period === "monthly") ?? sold[0];
    const current = (org?.plan ?? "free") === p.plan;
    // The org's plan but not in force (expired, unpaid, cancelled): paying again is buying it.
    const lapsed = current && p.plan !== "free" && inForce !== p.plan;
    return {
      ...p,
      name: PLAN_NAMES[p.plan],
      current,
      lapsed,
      action: lapsed ? `Pay for ${PLAN_NAMES[p.plan]}` : `Choose ${PLAN_NAMES[p.plan]}`,
      period: offer?.period ?? null,
      buy: Boolean(offer) && (!current || lapsed),
      recommended: p.plan === (org?.personal ? "pro" : "team"),
      note: p.plan === "free" ? "Always free" : !offer ? "Not sold here" : "",
    };
  });
}

const SECTIONS = ["general", "members", "billing", "tokens", "runs", "security"];

/**
 * A link into the Organization area, as the portal sends it (cloud/portal/src/app-redirect.js):
 * `#org=<id>&section=<s>` (org optional). Null for any other hash.
 */
export function parseOrgLink(hash) {
  const h = String(hash || "").replace(/^#/, "");
  if (!/(^|&)section=/.test(h)) return null;
  const p = new URLSearchParams(h);
  const section = p.get("section");
  if (!SECTIONS.includes(section)) return null;
  return { org: p.get("org") || null, section };
}

/**
 * U50: who can lift a plan limit, as the limit note says it. `org`: the refused org as the
 * account knows it (`GET /v1/orgs`: {id, name, role, personal}), or null when it isn't one of
 * the account's; `members`: `GET /v1/orgs/:org/members` when it could be read, else null; `me`:
 * the signed-in address. Never prints an org id as a name.
 * Returns { canFix, people: [{email, role}], text }.
 */
export function limitFixers({ org = null, members = null, me = "" } = {}) {
  if (org?.personal) return { canFix: true, people: [], text: "It's your personal account's plan: you can change it on Plan & billing." };
  if (org && (org.role === "owner" || org.role === "admin")) return { canFix: true, people: [], text: `You're ${org.role === "owner" ? "an owner" : "an admin"} of ${org.name}: you can change its plan on Plan & billing.` };
  const name = org?.name ? org.name : "the organization";
  const people = (members?.members ?? [])
    .filter((m) => (m.role === "owner" || m.role === "admin") && m.email !== me)
    .sort((a, b) => (a.role === b.role ? a.email.localeCompare(b.email) : a.role === "owner" ? -1 : 1))
    .map((m) => ({ email: m.email, role: m.role }));
  if (people.length) {
    const list = people.map((p) => `${p.email} (${p.role})`).join(", ");
    return { canFix: false, people, text: `Only owners and admins change the plan. Ask ${people.length === 1 ? "" : "one of "}${list} to make room or upgrade ${name}.` };
  }
  return { canFix: false, people: [], text: `Only owners and admins change the plan. Ask an owner of ${name} to make room or upgrade.` };
}

/** A refused create over a plan limit (402 `{error: "limit"}`), as the limit dialog needs it. */
export function limitInfo(e) {
  const b = e?.body;
  if (!e || e.status !== 402 || b?.error !== "limit") return null;
  const what = { seats: "seats", workspaces: "shared collections", storage: "storage", devices: "devices", runners: "CI runners", invites: "invites today", guests: "guest viewers" }[b.limit] ?? b.limit ?? "this";
  return {
    limit: b.limit ?? null,
    plan: b.plan ?? null,
    used: b.used ?? null,
    max: b.max ?? null,
    org: b.org ?? null,
    message: e.message || `The plan's limit on ${what} is reached.`,
    headline: b.max != null ? `${PLAN_NAMES[b.plan] ? `The ${PLAN_NAMES[b.plan]} plan` : "This plan"} has used its ${b.max} ${what}` : `${PLAN_NAMES[b.plan] ? `The ${PLAN_NAMES[b.plan]} plan` : "This plan"} has no room for more ${what}`,
  };
}
