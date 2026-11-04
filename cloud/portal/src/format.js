// Words and numbers the pages share: plan names, limits, statuses, prices, dates.

export function bytes(n) {
  if (n === null || n === undefined) return "no limit";
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${v} bytes` : `${v >= 10 || Number.isInteger(v) ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/** Seconds since the epoch as an ISO date-time for lit-material-timestamp, or "" for none. */
export const iso = (secs) => (secs ? new Date(secs * 1000).toISOString() : "");
/** The day only, for text. */
export const day = (secs) => (secs ? new Date(secs * 1000).toISOString().slice(0, 10) : "never");

export const shortId = (id) => String(id || "").slice(0, 8);
export const orgName = (o) => (o?.personal ? "Personal" : o?.name || "");

/** Usage meters: key in /v1/orgs/:org/usage, label, how to show a number. */
export const METRICS = [
  { key: "seats", label: "People (seats)" },
  { key: "workspaces", label: "Shared workspaces" },
  { key: "storageBytes", label: "Encrypted storage", format: bytes },
  { key: "devices", label: "Devices per person (most used)" },
  { key: "runners", label: "Runner credentials" },
  { key: "invitesPerDay", label: "Invites today" },
];

// The owner's prices (spec/CLOUD_SERVICE.md Q4), in US dollars before tax; Stripe's checkout
// (Managed Payments: Stripe is the merchant of record) adds tax and may show the buyer's currency.
export const PRICES = {
  pro: { monthly: { amount: "$8", period: "a month" }, yearly: { amount: "$6", period: "a month", note: "billed yearly" } },
  team: { monthly: { amount: "$12", period: "per seat a month" }, yearly: { amount: "$10", period: "per seat a month", note: "billed yearly" } },
  business: { monthly: { amount: "$25", period: "per seat a month" }, yearly: { amount: "$20", period: "per seat a month", note: "billed yearly" } },
};

export const PLAN_NAMES = { free: "Free", pro: "Pro", team: "Team", business: "Business", enterprise: "Enterprise" };

export const PLAN_FEATURES = {
  free: ["3 people, 3 shared workspaces", "100 MB encrypted storage", "3 devices per person", "5 runner credentials", "Run status rows"],
  pro: ["One person and two guests", "25 workspaces, 2 GB", "10 devices, 10 runner credentials", "Run history and webhook alerts"],
  team: ["Per seat, unlimited workspaces", "10 GB + 1 GB per seat", "5 free guest viewers", "Run history and Slack alerts", "Basic admin console"],
  business: ["Per seat, 50 GB + 2 GB per seat", "Single sign-on with your identity provider", "Signed audit export and daily anchors", "Full admin console, SCIM", "A year of run history"],
};

export const PLAN_NOTES = {
  free: "Free: 3 people, 3 shared workspaces, 100 MB, 3 devices per person, 5 runner credentials.",
  pro: "Pro: one person and two guests, 25 workspaces, 2 GB, 10 devices, 10 runner credentials.",
  team: "Team: per seat, unlimited workspaces, 10 GB + 1 GB per seat, 5 free guest viewers.",
  business: "Business: per seat, 50 GB + 2 GB per seat; hosted SSO and audit export come with it.",
  enterprise: "Enterprise: limits as the contract says.",
};

export const STATUS_NOTES = {
  trialing: "Team trial",
  on_trial: "trial",
  active: "active",
  past_due: "payment failed, being retried",
  cancelled: "cancelled, runs until the period ends",
  paused: "paused: Free limits until it resumes",
  unpaid: "unpaid: Free limits",
  expired: "ended: Free limits",
  refunded: "refunded: Free limits",
  incomplete: "waiting for the first payment: Free limits",
};

// Statuses that put a paid plan on Free limits (limits.rs plan_in_force), as one word for a label.
export const OFF_STATUS = { refunded: "refunded", expired: "ended", unpaid: "unpaid", paused: "paused", incomplete: "not paid yet" };

/** The plan in force, naming the bought one when it isn't: "free (pro refunded)". */
export function planLabel(plan, status) {
  return plan && plan !== "free" && OFF_STATUS[status] ? `free (${plan} ${OFF_STATUS[status]})` : plan || "free";
}

/** lit-material-status for a subscription or run status. */
export function statusKind(status) {
  if (["active", "passed", "ok", "verified"].includes(status)) return "ok";
  if (["trialing", "on_trial", "sealed", "pending", "invited"].includes(status)) return "info";
  if (["past_due", "cancelled", "paused", "incomplete", "missed"].includes(status)) return "warning";
  if (["failed", "unpaid", "expired", "refunded", "error"].includes(status)) return "error";
  return "neutral";
}

// Downloads `body` (a string or a Blob) as `name` (no server round trip: the portal already
// fetched it with the session).
export function download(name, body, type = "application/json") {
  const blob = body instanceof Blob ? body : new Blob([body], { type });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}
