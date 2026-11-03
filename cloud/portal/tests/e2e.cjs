// The portal end to end in Chromium against the real sync server binary (HITOP_SYNC_SERVER_BIN;
// required when CI is set) with HITOP Cloud's plan limits on: sign in with the mailed code, the
// personal org, create an org, invite a member and an address without an account, usage against
// the Free limits and the 402 when seats run out, a role change, the plan page, an access token
// (made, used, revoked), a passkey (added, then signing in with it through a virtual
// authenticator), sign out; passwords (C1b): create an account with a password and the mailed
// code, a wrong password, sign in, change it (the key bundle re-sealed under the new one), forgot
// and reset (the warning, the bundle gone); billing (C3) against a fake Lemon Squeezy API: buy Team
// with 3 seats (the checkout page, the signed custom data), the signed webhook deliveries Lemon
// Squeezy would send (fixtures of sync-server/tests/fixtures/lemonsqueezy), a 4th member refused
// with a "See plans" link, a 4th seat bought (quantity 4 sent to Lemon Squeezy), the member added,
// payment failed (the banner), a refund (Free limits), expired (Free, nothing removed), a bad
// signature refused, the billing portal link; C5: Business bought (per seat), the admin console
// (workspaces, people, devices; transfer ownership, a rotation request, a device removed), single
// sign-on per org (a domain claimed and verified through a fake DNS-over-HTTPS resolver, the
// provider saved and tested), the audit page (signed export, an anchor, the settings), the SSO
// button asking for the work address, and a server without a host key saying SSO isn't available.
// Serves dist/ (npm run build) with a few lines of Node.
//   (cd ../../webapp && npm run wasm)  # the password crypto: the web app's hitop-wasm build
//   HITOP_SYNC_SERVER_BIN=../../sync-server/target/debug/hitop-sync-server npm test
const { spawn } = require("child_process");
const crypto = require("crypto");
const fs = require("fs");
const http = require("http");
const net = require("net");
const os = require("os");
const path = require("path");
const { chromium } = require("playwright");

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok });
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : ` ${detail}`}`);
}

const freePort = () =>
  new Promise((resolve) => {
    const s = net.createServer().listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => resolve(port));
    });
  });

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".woff2": "font/woff2", ".svg": "image/svg+xml", ".wasm": "application/wasm" };

function serve(dir, port) {
  const server = http.createServer((req, res) => {
    const p = decodeURIComponent(new URL(req.url, "http://x").pathname);
    let file = path.join(dir, p === "/" ? "index.html" : p);
    if (!file.startsWith(dir) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) file = null;
    if (!file) {
      res.writeHead(404).end("not found");
      return;
    }
    res.writeHead(200, { "Content-Type": TYPES[path.extname(file)] || "text/plain" });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((resolve) => server.listen(port, "127.0.0.1", () => resolve(server)));
}

// Lemon Squeezy as the server calls it (checkouts, subscription items, subscriptions) plus the two
// pages a browser is sent to (checkout, billing portal).
const LS = { secret: "e2e-webhook-secret", key: "e2e-api-key", store: "4242", teamAnnual: "201", teamMonthly: "202", proAnnual: "101", businessAnnual: "301", businessMonthly: "302" };

// A DNS-over-HTTPS resolver (the JSON API) answering the TXT records the test sets (C5 domains).
function fakeDns(port) {
  const records = new Map();
  const server = http.createServer((req, res) => {
    const name = new URL(req.url, "http://x").searchParams.get("name");
    const txt = records.get(name) || [];
    res.writeHead(200, { "Content-Type": "application/dns-json" }).end(JSON.stringify(txt.length ? { Status: 0, Answer: txt.map((t) => ({ name, type: 16, TTL: 60, data: `"${t}"` })) } : { Status: 3 }));
  });
  return new Promise((resolve) => server.listen(port, "127.0.0.1", () => resolve({ server, records, url: `http://127.0.0.1:${port}/dns-query` })));
}
function fakeLemonSqueezy(port) {
  const seen = [];
  const server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (d) => (raw += d));
    req.on("end", () => {
      const p = new URL(req.url, "http://x").pathname;
      const body = raw ? JSON.parse(raw) : null;
      seen.push({ method: req.method, path: p, body, authorized: req.headers.authorization === `Bearer ${LS.key}` });
      const json = (status, v) => res.writeHead(status, { "Content-Type": "application/vnd.api+json" }).end(JSON.stringify(v));
      if (p.startsWith("/checkout/") || p.startsWith("/billing")) {
        res.writeHead(200, { "Content-Type": "text/html" }).end(`<!doctype html><title>Lemon Squeezy (fake)</title><h1>Lemon Squeezy (fake) ${p}</h1>`);
      } else if (req.headers.authorization !== `Bearer ${LS.key}`) {
        json(401, { errors: [{ detail: "Unauthenticated." }] });
      } else if (req.method === "POST" && p === "/v1/checkouts") {
        json(201, { data: { type: "checkouts", id: "c0ffee", attributes: { url: `http://127.0.0.1:${port}/checkout/c0ffee` } } });
      } else if (req.method === "PATCH" && p.startsWith("/v1/subscription-items/")) {
        json(200, { data: { type: "subscription-items", id: p.split("/").pop(), attributes: { quantity: body.data.attributes.quantity, updated_at: "2026-11-03T11:00:00.000000Z" } } });
      } else if (req.method === "GET" && p.startsWith("/v1/subscriptions/")) {
        json(200, { data: { type: "subscriptions", id: p.split("/").pop(), attributes: { urls: { customer_portal: `http://127.0.0.1:${port}/billing?signature=fresh` } } } });
      } else {
        json(404, { errors: [{ detail: "Not Found" }] });
      }
    });
  });
  return new Promise((resolve) => server.listen(port, "127.0.0.1", () => resolve({ server, seen, url: `http://127.0.0.1:${port}` })));
}

const fixture = (name) => JSON.parse(fs.readFileSync(path.join(__dirname, "../../../sync-server/tests/fixtures/lemonsqueezy", `${name}.json`), "utf8"));

function subscriptionEvent(event, id, variant, status, quantity, updated, custom) {
  const b = fixture("subscription");
  b.meta.event_name = event;
  b.meta.custom_data = custom;
  b.data.id = id;
  Object.assign(b.data.attributes, { store_id: Number(LS.store), variant_id: Number(variant), order_id: Number(`9${id}`), status, updated_at: updated });
  Object.assign(b.data.attributes.first_subscription_item, { id: Number(`7${id}`), quantity });
  if (["cancelled", "expired"].includes(status)) b.data.attributes.ends_at = "2027-11-03T10:00:00.000000Z";
  return b;
}

function invoiceEvent(event, sub, refunded, updated) {
  const b = fixture("subscription_invoice");
  b.meta.event_name = event;
  Object.assign(b.data.attributes, { store_id: Number(LS.store), subscription_id: Number(sub), refunded, status: refunded ? "refunded" : "paid", updated_at: updated });
  return b;
}

async function syncServer(portalOrigin, extraEnv = {}) {
  const bin = process.env.HITOP_SYNC_SERVER_BIN;
  if (!bin) {
    if (process.env.CI) throw new Error("HITOP_SYNC_SERVER_BIN must be set on CI");
    console.log("HITOP_SYNC_SERVER_BIN not set: skipped");
    process.exit(0);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "hitop-portal-e2e-"));
  const port = await freePort();
  const url = `http://127.0.0.1:${port}`;
  let log = "";
  const child = spawn(path.resolve(bin), [], {
    cwd: dir,
    env: {
      ...process.env,
      RWS_CONFIG_IP: "127.0.0.1",
      RWS_CONFIG_PORT: String(port),
      HITOP_SYNC_PUBLIC_URL: url,
      HITOP_SYNC_RP_ID: "localhost",
      HITOP_SYNC_RP_ORIGINS: portalOrigin,
      HITOP_SYNC_DATABASE: `sqlite:${path.join(dir, "sync.db")}`,
      HITOP_SYNC_MAIL: "log",
      HITOP_SYNC_ALLOWED_ORIGINS: portalOrigin,
      HITOP_SYNC_LIMITS: "plans",
      HITOP_SYNC_PORTAL_URL: `${portalOrigin}/`,
      ...extraEnv,
    },
    stdio: ["ignore", "ignore", "pipe"],
  });
  child.stderr.on("data", (d) => (log += d));
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${url}/healthz`)).ok) break;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  const mails = (email) => log.split(`mail to ${email}:`).length - 1;
  const code = async (email, after = 0) => {
    for (let i = 0; i < 100 && mails(email) <= after; i++) await new Promise((r) => setTimeout(r, 100));
    for (let i = 0; i < 100; i++) {
      const parts = log.split(`mail to ${email}:`);
      if (parts.length > 1) {
        const line = parts[parts.length - 1].split("\n").find((l) => l.includes("/signin#"));
        if (line) return line.trim().split("#").pop();
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error(`no mail to ${email}`);
  };
  const api = async (method, p, token, body) => {
    const r = await fetch(url + p, { method, headers: { ...(token ? { Authorization: `Bearer ${token}` } : {}), ...(body ? { "Content-Type": "application/json" } : {}) }, body: body ? JSON.stringify(body) : undefined });
    const text = await r.text();
    return { status: r.status, json: text ? JSON.parse(text) : null };
  };
  const signIn = async (email) => {
    await api("POST", "/v1/auth/email", null, { email });
    return (await api("POST", "/v1/auth/email/verify", null, { token: await code(email) })).json.session;
  };
  const stop = () => {
    child.kill();
    fs.rmSync(dir, { recursive: true, force: true });
  };
  return { url, code, mails, api, signIn, stop, log: () => log };
}

(async () => {
  const dist = path.join(__dirname, "..", "dist");
  if (!fs.existsSync(path.join(dist, "index.html"))) throw new Error("build first (npm run build)");
  const portalPort = await freePort();
  // localhost, not 127.0.0.1: WebAuthn needs a domain name as the relying party id.
  const origin = `http://localhost:${portalPort}`;
  const portal = await serve(dist, portalPort);
  const ls = await fakeLemonSqueezy(await freePort());
  const dns = await fakeDns(await freePort());
  const keyDir = fs.mkdtempSync(path.join(os.tmpdir(), "hitop-portal-hostkey-"));
  const hostKey = path.join(keyDir, "host.key");
  fs.writeFileSync(hostKey, crypto.randomBytes(32).toString("hex"), { mode: 0o600 });
  const server = await syncServer(origin, {
    HITOP_SYNC_HOST_KEY_FILE: hostKey,
    HITOP_SYNC_DNS_URL: dns.url,
    HITOP_SYNC_LS_VARIANT_BUSINESS_ANNUAL: LS.businessAnnual,
    HITOP_SYNC_LS_VARIANT_BUSINESS_MONTHLY: LS.businessMonthly,
    HITOP_SYNC_LS_WEBHOOK_SECRET: LS.secret,
    HITOP_SYNC_LS_STORE_ID: LS.store,
    HITOP_SYNC_LS_VARIANT_TEAM_ANNUAL: LS.teamAnnual,
    HITOP_SYNC_LS_VARIANT_TEAM_MONTHLY: LS.teamMonthly,
    HITOP_SYNC_LS_VARIANT_PRO_ANNUAL: LS.proAnnual,
    HITOP_SYNC_LS_API_KEY: LS.key,
    HITOP_SYNC_LS_API_URL: ls.url,
    HITOP_SYNC_LS_TEST_MODE: "on",
  });
  // A webhook delivery as Lemon Squeezy sends it: the raw bytes, signed.
  const deliver = async (body, signature) => {
    const raw = JSON.stringify(body);
    const sig = signature ?? crypto.createHmac("sha256", LS.secret).update(raw).digest("hex");
    const r = await fetch(`${server.url}/v1/billing/lemonsqueezy`, { method: "POST", headers: { "Content-Type": "application/json", "X-Event-Name": body.meta.event_name, "X-Signature": sig }, body: raw });
    return { status: r.status, json: await r.json() };
  };
  const browser = await chromium.launch();
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const app = page.locator("hitop-cloud");
  const idle = async () => {
    await page.waitForFunction(() => document.querySelector('hitop-cloud [aria-busy="false"]'), null, { timeout: 15000 });
    await page.waitForTimeout(150);
  };
  const alertText = () => app.locator('[role="alert"]').innerText().catch(() => "");
  try {
    check("the portal answers /healthz", (await (await fetch(`${origin}/healthz`)).text()).trim() === "ok");
    // Bob has an account already; Carol has none.
    const bobSession = await server.signIn("bob@example.com");

    await page.goto(`${origin}/`);
    await app.getByRole("button", { name: "Use a code instead" }).click();
    await app.getByLabel("Sync server").fill(server.url);
    await app.getByLabel("Email").fill("ada@example.com");
    await app.getByRole("button", { name: "Send code" }).click();
    await idle();
    await app.getByLabel("Code from the mail").fill(await server.code("ada@example.com"));
    await app.getByRole("button", { name: "Sign in", exact: true }).click();
    await idle();
    await page.getByRole("heading", { name: "Organizations" }).waitFor({ timeout: 10000 });
    check("signed in: the personal org is listed", (await app.locator("table").innerText()).includes("Personal"), await alertText());
    check("the footer names the commit (AGPL 13)", /^[0-9a-f]{7,}$|^main$/.test((await page.locator("#portal-commit").innerText()).trim()));

    await app.getByLabel("Organization name").fill("Acme");
    await app.getByRole("button", { name: "Create" }).click();
    await idle();
    await page.getByRole("heading", { name: "Acme" }).waitFor({ timeout: 10000 });
    const orgId = decodeURIComponent(page.url().split("#/orgs/")[1]);
    check("the org page opens", orgId.length === 32, page.url());
    const seats = () => app.locator('tr[data-metric="seats"]').innerText();
    check("usage: one seat of three", /\t1\t3/.test(await seats()), await seats());

    // HITOP Cloud C4: run history. A workspace of Ada's gets a run (a status row, as a runner
    // uploads it; Free keeps status rows only); the portal lists it and sets the alerts.
    const adaSession = JSON.parse(await page.evaluate(() => localStorage.getItem("hitop_cloud"))).session;
    const key = (await server.api("POST", "/v1/me/keys", adaSession, { name: "e2e", publicKey: crypto.randomBytes(32).toString("base64url") })).json.id;
    const wsId = crypto.randomBytes(16).toString("hex");
    check("a workspace for the run history", (await server.api("POST", "/v1/workspaces", adaSession, { id: wsId, wraps: [{ keyId: key, wrapped: crypto.randomBytes(80).toString("base64url") }] })).status === 201);
    const up = await server.api("POST", `/v1/workspaces/${wsId}/runs`, adaSession, { id: "portal-run-1", startedAt: 1793700000, collection: "Shop", environment: "prod", status: "failed", durationMs: 50, requests: 2, failed: 1 });
    check("a run is uploaded", up.status === 201 && up.json.stored === "status", JSON.stringify(up.json));
    check("usage: run uploads today against the plan", (await app.locator('[data-metric="runsPerDay"]').innerText().catch(() => "")).includes("Run uploads today: 0 of 100"), await app.innerText());
    await page.goto(`${origin}/#/runs`);
    await idle();
    await app.getByRole("link", { name: wsId.slice(0, 8) }).click();
    await idle();
    const runsText = await app.getByLabel("Runs", { exact: true }).innerText().catch(() => "");
    check("the run history lists the status row", runsText.includes("failed") && runsText.includes("Shop (prod)") && runsText.includes("1/2") && runsText.includes("status only"), runsText + (await alertText()));
    check("Free keeps status rows only, and says so", (await app.innerText()).includes("status rows only"));
    await page.goto(`${origin}/#/workspaces/${wsId}/runs/portal-run-1`);
    await idle();
    check("an alert's link opens the run", (await app.getByLabel("Run portal-run-1").innerText().catch(() => "")).includes("1/2 requests passed"), await app.innerText());
    check("Free alerts are mail only", (await app.getByLabel("Webhook URLs (https, one per line)").count()) === 0);
    await app.getByLabel("Mail to members (comma separated)").fill("ada@example.com");
    await app.getByRole("button", { name: "Save alerts" }).click();
    await idle();
    check("an owner saves the alert settings", (await app.locator('[role="status"]').innerText().catch(() => "")).includes("Alert settings saved") && (await server.api("GET", `/v1/workspaces/${wsId}/alerts`, adaSession)).json.settings.email[0] === "ada@example.com", await alertText());
    await page.goto(`${origin}/#/orgs/${orgId}`);
    await idle();

    await app.getByLabel("Email to invite").fill("bob@example.com");
    await app.getByRole("button", { name: "Invite", exact: true }).click();
    await idle();
    check("an account is added", (await app.locator('[role="status"]').innerText()).includes("Added bob@example.com"));
    await app.getByLabel("Email to invite").fill("carol@example.com");
    await app.getByRole("button", { name: "Invite", exact: true }).click();
    await idle();
    const members = await app.getByRole("table", { name: "Members" }).innerText();
    check("an address without an account is invited", members.includes("carol@example.com") && members.includes("(invited)"), members);
    check("usage: three seats of three, shown full", /\t3\t3/.test(await seats()) && (await app.locator('tr[data-metric="seats"]').getAttribute("class")) === "portal-full", await seats());
    await app.getByLabel("Email to invite").fill("dave@example.com");
    await app.getByRole("button", { name: "Invite", exact: true }).click();
    await idle();
    const refused = await alertText();
    check("a fourth seat is refused with the plan's message", refused.includes("plan limit") && refused.includes("free plan") && refused.includes("3 people"), refused);

    await app.getByLabel("Role of bob@example.com").selectOption("admin");
    await idle();
    const bobOrgs = await server.api("GET", "/v1/orgs", bobSession);
    check("a role change reaches the server", bobOrgs.json.orgs.some((o) => o.id === orgId && o.role === "admin"), JSON.stringify(bobOrgs.json));
    await app.getByRole("button", { name: "Cancel invite" }).click();
    await idle();
    check("an invite is cancelled", !(await app.getByRole("table", { name: "Members" }).innerText()).includes("carol@example.com"));

    // The upgrade link of a refused create opens the plan page.
    const refusedApi = await server.api("POST", `/v1/orgs/${orgId}/members`, bobSession, { email: "x1@example.com", role: "member" });
    const refusedApi2 = refusedApi.status === 201 ? await server.api("POST", `/v1/orgs/${orgId}/members`, bobSession, { email: "x2@example.com", role: "member" }) : refusedApi;
    check("the API refuses with 402 and an upgrade URL", refusedApi2.status === 402 && refusedApi2.json.upgradeUrl === `${origin}/#/orgs/${orgId}/plan`, JSON.stringify(refusedApi2.json));
    await page.goto(refusedApi2.json.upgradeUrl);
    await idle();
    await page.getByRole("heading", { name: "Plan: free" }).waitFor({ timeout: 10000 });
    check("the plan page shows the Free limits", (await app.innerText()).includes("Free: 3 people"));

    // Billing (C3): buy Team with 3 seats.
    await app.getByLabel("Buy Team").getByLabel("Seats").fill("3");
    await app.getByRole("button", { name: "Buy Team, yearly" }).click();
    await page.waitForURL(`${ls.url}/checkout/**`, { timeout: 10000 });
    const made = ls.seen.find((r) => r.method === "POST" && r.path === "/v1/checkouts");
    const custom = made?.body.data.attributes.checkout_data.custom;
    check("Buy Team opens Lemon Squeezy's checkout for 3 seats of the org", made?.authorized && custom.org_id === orgId && made.body.data.attributes.checkout_data.variant_quantities[0].quantity === 3 && made.body.data.relationships.variant.data.id === LS.teamAnnual, JSON.stringify(made?.body));
    const bad = await deliver(subscriptionEvent("subscription_created", "1", LS.teamAnnual, "active", 3, "2026-11-03T10:00:00.000000Z", custom), "0".repeat(64));
    check("a webhook with a bad signature is refused", bad.status === 401, JSON.stringify(bad));
    const created = await deliver(subscriptionEvent("subscription_created", "1", LS.teamAnnual, "active", 3, "2026-11-03T10:00:00.000000Z", custom));
    const again = await deliver(subscriptionEvent("subscription_created", "1", LS.teamAnnual, "active", 3, "2026-11-03T10:00:00.000000Z", custom));
    check("the signed purchase applies, its replay doesn't", created.json.outcome === "applied" && again.json.outcome === "duplicate", JSON.stringify([created, again]));
    await page.goto(`${origin}/#/orgs/${orgId}/plan`);
    await idle();
    await page.getByRole("heading", { name: "Plan: team" }).waitFor({ timeout: 10000 });
    check("back on the plan page: Team, 3 seats", (await app.getByLabel("Subscription").innerText()).includes("3 seat(s) bought, 3 in use"), await app.innerText());
    // A 4th member: refused with a link to the plans, then a 4th seat, then added.
    await page.goto(`${origin}/#/orgs/${orgId}`);
    await idle();
    await app.getByLabel("Email to invite").fill("dave@example.com");
    await app.getByRole("button", { name: "Invite", exact: true }).click();
    await idle();
    const seeing = app.locator('[role="alert"] a.portal-upgrade');
    check("a 4th member is refused, with a See plans link", (await alertText()).includes("team plan") && (await seeing.getAttribute("href")) === `#/orgs/${orgId}/plan`, await alertText());
    await seeing.click();
    await idle();
    await page.getByLabel("Seats").fill("4");
    await app.getByRole("button", { name: "Change seats" }).click();
    await idle();
    const patch = ls.seen.find((r) => r.method === "PATCH");
    check("a 4th seat: quantity 4 sent to Lemon Squeezy, charged at once", patch?.path === "/v1/subscription-items/71" && patch.body.data.attributes.quantity === 4 && patch.body.data.attributes.invoice_immediately === true, JSON.stringify(patch?.body));
    const dave = await server.api("POST", `/v1/orgs/${orgId}/members`, bobSession, { email: "dave@example.com", role: "member" });
    check("...and the 4th member fits", dave.status === 201, JSON.stringify(dave.json));
    await deliver(subscriptionEvent("subscription_updated", "1", LS.teamAnnual, "active", 4, "2026-11-03T11:00:01.000000Z", custom));
    // Payment failed: the banner, still Team.
    await deliver(invoiceEvent("subscription_payment_failed", "1", false, "2027-11-03T10:00:00.000000Z"));
    await deliver(subscriptionEvent("subscription_updated", "1", LS.teamAnnual, "past_due", 4, "2027-11-03T10:00:01.000000Z", custom));
    await page.reload();
    await idle();
    check("payment failed: the banner, still Team", (await app.getByLabel("Payment failed").count()) === 1 && (await app.innerText()).includes("Plan: team"), await app.innerText());
    // The billing portal opens Lemon Squeezy's page.
    await app.getByRole("button", { name: "Manage billing" }).click();
    await page.waitForURL(`${ls.url}/billing**`, { timeout: 10000 });
    check("Manage billing opens the Lemon Squeezy customer portal", page.url().includes("signature=fresh"), page.url());
    // A refund ends the plan; expired is Free too; nothing is removed.
    const refunded = await deliver(invoiceEvent("subscription_payment_refunded", "1", true, "2027-11-04T10:00:00.000000Z"));
    await page.goto(`${origin}/#/orgs/${orgId}/plan`);
    await idle();
    check("a refund: Free limits", refunded.json.outcome === "applied" && (await alertText()).includes("refunded: Free limits"), await alertText());
    const expired = await deliver(subscriptionEvent("subscription_expired", "1", LS.teamAnnual, "expired", 4, "2027-11-17T10:00:00.000000Z", custom));
    const people = (await server.api("GET", `/v1/orgs/${orgId}/members`, bobSession)).json;
    const reads = await server.api("GET", "/v1/workspaces", bobSession);
    check("expired: Free limits, all 4 people still there, reads work", expired.json.outcome === "applied" && people.members.length + people.invites.length === 4 && reads.status === 200, JSON.stringify(expired.json));
    const fifth = await server.api("POST", `/v1/orgs/${orgId}/members`, bobSession, { email: "eve@example.com", role: "member" });
    check("...and creates beyond Free are refused", fifth.status === 402 && fifth.json.plan === "free", JSON.stringify(fifth.json));

    // HITOP Cloud C5: a Business org (bought per seat), its admin console, single sign-on with a
    // verified domain, the audit page. Acme is on Free limits now: its admin console says so.
    await page.goto(`${origin}/#/orgs/${orgId}/admin`);
    await idle();
    check("the admin console on Free: refused with a See plans link", (await alertText()).includes("admin console") && (await app.locator('[role="alert"] a.portal-upgrade').count()) === 1, await alertText());
    const globex = (await server.api("POST", "/v1/orgs", adaSession, { name: "Globex" })).json.id;
    check("Bob joins Globex", (await server.api("POST", `/v1/orgs/${globex}/members`, adaSession, { email: "bob@example.com", role: "member" })).status === 201);
    await page.goto(`${origin}/#/orgs/${globex}/plan`);
    await idle();
    check("the plan page offers Business next to Team", (await app.getByRole("button", { name: "Buy Business, yearly" }).count()) === 1 && (await app.innerText()).includes("$20 per seat a month"), await app.innerText());
    await app.getByLabel("Buy Business").getByLabel("Seats").fill("3");
    await app.getByRole("button", { name: "Buy Business, yearly" }).click();
    await page.waitForURL(`${ls.url}/checkout/**`, { timeout: 10000 });
    const bizCheckout = ls.seen.filter((r) => r.method === "POST" && r.path === "/v1/checkouts").pop();
    const bizCustom = bizCheckout?.body.data.attributes.checkout_data.custom;
    check("Buy Business: the Business variant, 3 seats, Globex", bizCustom?.org_id === globex && bizCheckout.body.data.relationships.variant.data.id === LS.businessAnnual && bizCheckout.body.data.attributes.checkout_data.variant_quantities[0].quantity === 3, JSON.stringify(bizCheckout?.body));
    const biz = await deliver(subscriptionEvent("subscription_created", "2", LS.businessAnnual, "active", 3, "2026-11-03T12:00:00.000000Z", bizCustom));
    check("...the purchase applies: Business", biz.json.outcome === "applied" && (await server.api("GET", `/v1/orgs/${globex}/limits`, adaSession)).json.planInForce === "business", JSON.stringify(biz.json));
    // Ada's workspace moves into Globex; Bob (one device) edits it.
    check("the workspace moves into Globex", (await server.api("POST", `/v1/orgs/${globex}/workspaces`, adaSession, { workspace: wsId })).status === 200);
    const bobKey = (await server.api("POST", "/v1/me/keys", bobSession, { name: "bob-laptop", publicKey: crypto.randomBytes(32).toString("base64url") })).json.id;
    const bobAdded = await server.api("POST", `/v1/workspaces/${wsId}/members`, adaSession, { email: "bob@example.com", role: "editor", wraps: [{ keyId: bobKey, wrapped: crypto.randomBytes(80).toString("base64url") }] });
    check("Bob is added to the workspace", bobAdded.status === 201, JSON.stringify(bobAdded.json));

    await page.goto(`${origin}/#/orgs/${globex}`);
    await idle();
    await app.getByRole("link", { name: "Admin console" }).click();
    await idle();
    const card = app.getByLabel(`Workspace ${wsId.slice(0, 8)}`);
    const wsPeople = await app.getByLabel(`People of ${wsId.slice(0, 8)}`).innerText().catch(() => "");
    check("admin console: the org's workspace and its people", /ada@example\.com\s+owner/.test(wsPeople) && /bob@example\.com\s+editor/.test(wsPeople), wsPeople + (await alertText()));
    const devices = await app.getByRole("table", { name: "Devices" }).innerText().catch(() => "");
    check("admin console: the members' devices", devices.includes("bob-laptop") && devices.includes("e2e"), devices);
    check("no key material on the page", !(await app.innerText()).includes("wrapped"));
    await app.getByLabel(`New owner of ${wsId.slice(0, 8)}`).selectOption({ label: "bob@example.com" });
    await card.getByRole("button", { name: "Transfer ownership" }).click();
    await idle();
    const roles = (await server.api("GET", `/v1/workspaces/${wsId}`, bobSession)).json.members.map((m) => `${m.email}:${m.role}`).sort().join();
    check("transfer ownership: Bob owns it, Ada edits", roles === "ada@example.com:editor,bob@example.com:owner", roles + (await alertText()));
    await card.getByRole("button", { name: "Ask for a key rotation" }).click();
    await idle();
    check("a rotation request shows on the workspace", (await app.getByLabel("Rotation requested").count()) === 1 && (await server.api("GET", `/v1/workspaces/${wsId}`, bobSession)).json.rotationRequested?.by === "ada@example.com", await alertText());
    let dialog = "";
    page.once("dialog", (d) => {
      dialog = d.message();
      d.accept();
    });
    await app.getByRole("table", { name: "Devices" }).getByRole("row").filter({ hasText: "bob-laptop" }).getByRole("button", { name: "Remove" }).click();
    await idle();
    const after = (await server.api("GET", `/v1/orgs/${globex}/admin`, adaSession)).json;
    check("a device removed everywhere, after a warning", dialog.includes("removed from their account everywhere") && !after.devices.some((d) => d.name === "bob-laptop") && !(await app.getByRole("table", { name: "Devices" }).innerText()).includes("bob-laptop"), dialog + JSON.stringify(after.devices));

    // Single sign-on: claim globex.test, publish the TXT record, verify; the provider.
    await page.goto(`${origin}/#/orgs/${globex}`);
    await idle();
    await app.getByRole("link", { name: "Single sign-on" }).click();
    await idle();
    await app.getByLabel("Domain to claim").fill("globex.test");
    await app.getByRole("button", { name: "Claim domain" }).click();
    await idle();
    const domains = await app.getByRole("table", { name: "Domains" }).innerText().catch(() => "");
    const txt = /hitop-verify=[0-9a-f]{32}/.exec(domains)?.[0];
    check("a claimed domain shows the TXT record to publish", domains.includes("_hitop-verify.globex.test") && Boolean(txt) && domains.includes("not verified"), domains + (await alertText()));
    await app.getByRole("table", { name: "Domains" }).getByRole("row").filter({ hasText: "globex.test" }).getByRole("button", { name: "Verify" }).click();
    await idle();
    check("verify before the record exists: says so", (await alertText()).includes("no TXT record"), await alertText());
    dns.records.set("_hitop-verify.globex.test", ["v=spf1 -all", txt]);
    await app.getByRole("table", { name: "Domains" }).getByRole("row").filter({ hasText: "globex.test" }).getByRole("button", { name: "Verify" }).click();
    await idle();
    check("...and after: verified", (await app.getByRole("table", { name: "Domains" }).innerText()).includes("verified 20"), await alertText());
    check("the redirect URI to register", (await app.getByLabel("Redirect URI").innerText()) === `${server.url}/v1/auth/sso/callback`);
    const deadIdp = `http://127.0.0.1:${await freePort()}/realm`;
    await app.getByLabel("Issuer URL").fill(deadIdp);
    await app.getByLabel("Client ID").fill("hitop-portal-e2e");
    await app.getByLabel("Client secret").fill("e2e-client-secret");
    await app.getByLabel("Button name").fill("Globex SSO");
    await app.getByRole("button", { name: "Save" }).click();
    await idle();
    const ssoApi = await server.api("GET", `/v1/orgs/${globex}/sso`, adaSession);
    check("the provider is saved, the secret never comes back", (await app.locator('[role="status"]').innerText().catch(() => "")).includes("saved") && ssoApi.json.config?.issuer === deadIdp && ssoApi.json.config.hasSecret && !JSON.stringify(ssoApi.json).includes("e2e-client-secret") && !(await app.innerText()).includes("e2e-client-secret"), JSON.stringify(ssoApi.json) + (await alertText()));
    await app.getByRole("button", { name: "Test" }).click();
    await idle();
    check("Test reports an unreachable provider", (await alertText()).includes("provider check failed"), await alertText());

    // Audit: the signed export, an anchor now, the settings.
    await page.goto(`${origin}/#/orgs/${globex}/audit`);
    await idle();
    check("no daily anchor yet", (await app.getByLabel("No anchors").count()) === 1, await app.innerText());
    let dl = page.waitForEvent("download");
    await app.getByRole("button", { name: "Download signed export" }).click();
    const exported = fs.readFileSync(await (await dl).path(), "utf8").trim().split("\n").map((l) => JSON.parse(l));
    check("the signed export downloads: header, chains, events, signature", exported[0].type === "hitop-audit-export" && exported.some((l) => l.type === "chain" && l.chain === wsId) && exported.some((l) => l.event === "owner_transferred") && exported.at(-1).type === "signature" && exported.at(-1).alg === "Ed25519", JSON.stringify(exported.at(-1)));
    dl = page.waitForEvent("download");
    await app.getByRole("button", { name: "Anchor now" }).click();
    const anchor = JSON.parse(fs.readFileSync(await (await dl).path(), "utf8"));
    check("Anchor now downloads a signed anchor of every chain", anchor.type === "hitop-audit-anchor" && anchor.org === globex && anchor.chains.some((c) => c.chain === wsId) && anchor.signature.length > 40, JSON.stringify(anchor).slice(0, 300));
    await app.locator("#audit-public").check();
    await idle();
    const settings = (await server.api("GET", `/v1/orgs/${globex}/audit/anchors`, adaSession)).json;
    check("an owner publishes the anchors (opt-in), mail stays on", settings.publicAnchors === true && settings.mailAnchors === true && (await app.innerText()).includes("how many workspaces"), JSON.stringify(settings));

    // The SSO button now asks for the work address, then starts with that org's provider.
    {
      const ctx = await browser.newContext();
      await ctx.addInitScript((s) => localStorage.setItem("hitop_cloud", JSON.stringify({ server: s })), server.url);
      const p2 = await ctx.newPage();
      const a2 = p2.locator("hitop-cloud");
      let started = null;
      p2.on("request", (r) => {
        if (r.url().endsWith("/v1/auth/sso/start")) started = JSON.parse(r.postData() || "{}");
      });
      await p2.goto(`${origin}/`);
      await a2.getByRole("button", { name: "Sign in with SSO" }).waitFor({ timeout: 10000 });
      await a2.getByLabel("Email").fill("");
      await a2.getByRole("button", { name: "Sign in with SSO" }).click();
      await p2.waitForTimeout(300);
      check("the SSO button asks for the work address", (await a2.locator('[role="alert"]').innerText().catch(() => "")).includes("work address"));
      await a2.getByLabel("Email").fill("zoe@globex.test");
      await a2.getByRole("button", { name: "Sign in with SSO" }).click();
      await p2.waitForTimeout(800);
      check("...and starts with it (the org's provider, unreachable here)", started?.email === "zoe@globex.test" && (await a2.locator('[role="alert"]').innerText().catch(() => "")).includes("identity provider"), JSON.stringify(started));
      await ctx.close();
    }
    // A server without a host key (the hosted test server): no single sign-on per org.
    {
      const plain = await syncServer(origin);
      try {
        const zed = await plain.signIn("zed@example.com");
        const zorg = (await plain.api("POST", "/v1/orgs", zed, { name: "Zed" })).json.id;
        const ctx = await browser.newContext();
        await ctx.addInitScript(([s, t]) => localStorage.setItem("hitop_cloud", JSON.stringify({ server: s, session: t, email: "zed@example.com" })), [plain.url, zed]);
        const p3 = await ctx.newPage();
        await p3.goto(`${origin}/#/orgs/${zorg}/sso`);
        await p3.locator("hitop-cloud").getByLabel("Not available").waitFor({ timeout: 10000 });
        check("without a host key: single sign-on isn't available", (await p3.locator("hitop-cloud").getByLabel("Not available").innerText()).includes("isn't available on this server"));
        await ctx.close();
      } finally {
        plain.stop();
      }
    }

    // Access tokens.
    await page.getByRole("navigation", { name: "Portal" }).getByRole("link", { name: "Access tokens" }).click();
    await idle();
    await app.getByLabel("Token name").fill("ci");
    await app.getByRole("button", { name: "Create token" }).click();
    await idle();
    const token = (await app.getByLabel("New token").innerText()).trim();
    check("a token is shown once", token.startsWith("hitop_pat_"), token);
    const used = await server.api("GET", "/v1/workspaces", token);
    const write = await server.api("POST", "/v1/orgs", token, { name: "x" });
    check("the token reads, and only reads", used.status === 200 && write.status === 403, `${used.status} ${write.status}`);
    await page.reload();
    await idle();
    check("listed after a reload, never the token itself", (await app.getByRole("table", { name: "Tokens" }).innerText()).includes("ci") && !(await app.innerText()).includes("hitop_pat_"));
    await app.getByRole("button", { name: "Revoke" }).click();
    await idle();
    check("a revoked token stops working", (await server.api("GET", "/v1/workspaces", token)).status === 401);

    // A passkey through Chromium's virtual authenticator, then signing in with it.
    const cdp = await context.newCDPSession(page);
    await cdp.send("WebAuthn.enable");
    await cdp.send("WebAuthn.addVirtualAuthenticator", { options: { protocol: "ctap2", transport: "internal", hasResidentKey: true, hasUserVerification: true, isUserVerified: true } });
    await page.getByRole("navigation", { name: "Portal" }).getByRole("link", { name: "Account" }).click();
    await idle();
    await app.getByLabel("Passkey name").fill("laptop");
    await app.getByRole("button", { name: "Add a passkey" }).click();
    await idle();
    check("a passkey is added", (await app.innerText()).includes("1 passkey(s)"), await alertText());
    await app.getByRole("button", { name: "Sign out" }).click();
    await idle();
    check("signed out", await app.getByRole("button", { name: "Sign in with a passkey" }).isVisible());
    await app.getByLabel("Sync server").fill(server.url);
    await app.getByLabel("Email").fill("ada@example.com");
    await app.getByRole("button", { name: "Sign in with a passkey" }).click();
    await idle();
    await page.getByRole("heading", { name: "Account" }).waitFor({ timeout: 10000 }).catch(() => {});
    check("signed in with the passkey", /Signed in as\s+ada@example\.com/.test(await app.innerText()), `${await alertText()} | ${(await app.innerText()).slice(0, 300)}`);

    // Passwords. The client crypto (hitop-sync-crypto in WASM) also runs here, as a hitop device
    // would: it seals a key bundle with the password, which the portal must re-seal on a change.
    const wasm = await import(path.join(__dirname, "../../../webapp/src/wasm/format.js"));
    const w = (await wasm.formatFromBytes(fs.readFileSync(path.join(__dirname, "../../../webapp/src/wasm/pkg/hitop_wasm_bg.wasm")))).sync;
    const keysOf = async (email, password) => w.passwordKeys(password, (await server.api("POST", "/v1/auth/password/params", null, { email })).json.kdf);
    await app.getByRole("button", { name: "Sign out" }).click();
    await idle();
    if (await app.getByRole("button", { name: "Use a password instead" }).count()) await app.getByRole("button", { name: "Use a password instead" }).click();
    await app.getByLabel("Sync server").fill(server.url);
    await app.getByLabel("Email").fill("erin@example.com");
    await app.getByLabel("Password", { exact: true }).fill("erin's first password");
    let before = server.mails("erin@example.com");
    await app.getByRole("button", { name: "Create account" }).click();
    await idle();
    await app.getByLabel("Code from the confirmation mail").fill(await server.code("erin@example.com", before));
    await app.getByRole("button", { name: "Confirm" }).click();
    await idle();
    check("create an account with a password and the mailed code", /Signed in as\s+erin@example\.com/.test(await app.innerText()), await alertText());
    const k1 = await keysOf("erin@example.com", "erin's first password");
    const erin = (await server.api("POST", "/v1/auth/password/login", null, { email: "erin@example.com", authKey: k1.authKey })).json;
    const sealed = w.bundleSeal(k1.bundleKey, erin.user.id, '{"v":1,"keyId":"device-1"}');
    check("a hitop device seals its keys with the password", (await server.api("PUT", "/v1/me/recovery?method=password", erin.session, { blob: sealed })).status === 200);
    await app.getByRole("button", { name: "Sign out" }).click();
    await idle();
    const passwordSignIn = async (password) => {
      await app.getByLabel("Sync server").fill(server.url);
      await app.getByLabel("Email").fill("erin@example.com");
      await app.getByLabel("Password", { exact: true }).fill(password);
      await app.getByRole("button", { name: "Sign in", exact: true }).click();
      await idle();
    };
    await passwordSignIn("not erin's password");
    check("a wrong password is refused, without saying which part", (await alertText()).includes("email address or the password is wrong"), await alertText());
    await passwordSignIn("erin's first password");
    check("sign in with the password", /Signed in as\s+erin@example\.com/.test(await app.innerText()), await alertText());
    await page.getByRole("navigation", { name: "Portal" }).getByRole("link", { name: "Account" }).click();
    await idle();
    await app.getByLabel("Current password").fill("erin's first password");
    await app.getByLabel("New password", { exact: true }).fill("erin's second password");
    await app.getByLabel("Repeat the new password").fill("erin's second password");
    await app.getByRole("button", { name: "Change password" }).click();
    await idle();
    check("change the password", (await app.locator('[role="status"]').innerText().catch(() => "")).includes("Password changed"), await alertText());
    const k2 = await keysOf("erin@example.com", "erin's second password");
    const s2 = (await server.api("POST", "/v1/auth/password/login", null, { email: "erin@example.com", authKey: k2.authKey })).json.session;
    const got = await server.api("GET", "/v1/me/recovery?method=password", s2);
    check("the key bundle is re-sealed under the new password", w.bundleOpen(k2.bundleKey, erin.user.id, got.json.blob) === '{"v":1,"keyId":"device-1"}', JSON.stringify(got.json).slice(0, 80));
    check("the old password no longer signs in", (await server.api("POST", "/v1/auth/password/login", null, { email: "erin@example.com", authKey: k1.authKey })).status === 401);

    await app.getByRole("button", { name: "Sign out" }).click();
    await idle();
    await app.getByRole("button", { name: "Forgot password?" }).click();
    check("the page warns before a reset", (await app.getByLabel("Reset warning").innerText()).includes("can't unlock your end-to-end encrypted workspaces"));
    await app.getByLabel("Sync server").fill(server.url);
    await app.getByLabel("Email").fill("erin@example.com");
    before = server.mails("erin@example.com");
    await app.getByRole("button", { name: "Send reset code" }).click();
    await idle();
    await app.getByLabel("Code from the reset mail").fill(await server.code("erin@example.com", before));
    await app.getByLabel("New password", { exact: true }).fill("erin's third password");
    await app.getByLabel("Repeat the new password").fill("erin's third password");
    await app.getByRole("button", { name: "Reset password" }).click();
    await idle();
    const resetNote = await app.locator('[role="status"]').innerText().catch(() => "");
    check("a reset signs in and says the key bundle is gone", /Signed in as\s+erin@example\.com/.test(await app.innerText()) && resetNote.includes("is gone"), resetNote + (await alertText()));
    check("...it is gone on the server", (await server.api("GET", "/v1/me/recovery?method=password", JSON.parse(await page.evaluate(() => localStorage.getItem("hitop_cloud"))).session)).status === 404);

    const events = (await server.api("GET", `/v1/me/audit?limit=100`, bobSession)).json.events.map((e) => e.event);
    check("the server's audit log has the sign-ins", events.includes("signin"), events.join());
    check("no page errors", errors.length === 0, errors.join(" | "));
  } catch (e) {
    check("the run finished", false, `${e.stack}\n--- server log ---\n${server.log().slice(-3000)}`);
  } finally {
    await browser.close();
    portal.close();
    ls.server.close();
    dns.server.close();
    server.stop();
    fs.rmSync(keyDir, { recursive: true, force: true });
  }
  const failed = results.filter((r) => !r.ok);
  console.log(`${results.length - failed.length}/${results.length} passed`);
  process.exit(failed.length ? 1 : 0);
})();
