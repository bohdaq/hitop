// The portal end to end in Chromium against the real sync server binary (HITOP_SYNC_SERVER_BIN;
// required when CI is set) with HITOP Cloud's plan limits on: sign in with the mailed code, the
// personal org, create an org, invite a member and an address without an account, usage against
// the Free limits and the 402 when seats run out, a role change, the plan page, an access token
// (made, used, revoked), a passkey (added, then signing in with it through a virtual
// authenticator), sign out; passwords (C1b): create an account with a password and the mailed
// code, a wrong password, sign in, change it (the key bundle re-sealed under the new one), forgot
// and reset (the warning, the bundle gone); billing (C3) against a fake Stripe API (Managed
// Payments, prices by lookup key): buy Team with 3 seats (the Checkout Session, the signed
// metadata), the signed webhook events Stripe would send (fixtures of
// sync-server/tests/fixtures/stripe), a 4th member refused with a "See plans" link, a 4th seat
// bought (quantity 4 sent to Stripe, invoiced at once), the member added,
// payment failed (the banner), a refund (Free limits), expired (Free, nothing removed), a bad
// signature refused, the billing portal link; C5: Business bought (per seat), the admin console
// (workspaces, people, devices; transfer ownership, a rotation request, a device removed from the org's workspaces), single
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

// Stripe as the server calls it (prices by lookup key, Checkout Sessions, subscription updates,
// customer portal sessions; form-encoded) plus the two pages a browser is sent to (checkout,
// billing portal).
const ST = { secret: "whsec_e2e_webhook", key: "sk_test_e2e", version: "2026-10-28.endive" };
const LOOKUP_KEYS = ["pro_monthly", "pro_yearly", "team_monthly", "team_yearly", "business_monthly", "business_yearly"];
const priceOf = (key) => `price_e2e_${key}`;

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
function fakeStripe(port) {
  const seen = [];
  const server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (d) => (raw += d));
    req.on("end", () => {
      const u = new URL(req.url, "http://x");
      const p = u.pathname;
      const form = new URLSearchParams(req.method === "GET" ? u.search : raw);
      const authorized = req.headers.authorization === `Bearer ${ST.key}` && req.headers["stripe-version"] === ST.version;
      seen.push({ method: req.method, path: p, form, authorized });
      const json = (status, v) => res.writeHead(status, { "Content-Type": "application/json" }).end(JSON.stringify(v));
      if (p.startsWith("/checkout/") || p.startsWith("/billing/")) {
        res.writeHead(200, { "Content-Type": "text/html" }).end(`<!doctype html><title>Stripe (fake)</title><h1>Stripe (fake) ${p}: Sold through Link</h1>`);
      } else if (!authorized) {
        json(401, { error: { type: "invalid_request_error", message: "Invalid API Key provided" } });
      } else if (req.method === "GET" && p === "/v1/prices") {
        const wanted = form.getAll("lookup_keys[]");
        json(200, { object: "list", data: LOOKUP_KEYS.filter((k) => wanted.includes(k)).map((k) => ({ id: priceOf(k), object: "price", active: true, lookup_key: k, currency: "usd", tax_behavior: "exclusive", recurring: { interval: k.endsWith("yearly") ? "year" : "month" } })) });
      } else if (req.method === "POST" && p === "/v1/checkout/sessions") {
        json(200, { id: "cs_test_e2e", object: "checkout.session", url: `http://127.0.0.1:${port}/checkout/cs_test_e2e` });
      } else if (req.method === "POST" && p.startsWith("/v1/subscriptions/")) {
        json(200, { id: p.split("/").pop(), object: "subscription", status: "active", items: { data: [{ id: form.get("items[0][id]"), quantity: Number(form.get("items[0][quantity]")) }] } });
      } else if (req.method === "POST" && p === "/v1/billing_portal/sessions") {
        json(200, { id: "bps_test", object: "billing_portal.session", url: `http://127.0.0.1:${port}/billing/session_${form.get("customer")}` });
      } else {
        json(404, { error: { type: "invalid_request_error", message: "Unrecognized request URL" } });
      }
    });
  });
  return new Promise((resolve) => server.listen(port, "127.0.0.1", () => resolve({ server, seen, url: `http://127.0.0.1:${port}` })));
}

const fixture = (name) => JSON.parse(fs.readFileSync(path.join(__dirname, "../../../sync-server/tests/fixtures/stripe", `${name}.json`), "utf8"));
let eventNo = 0;
// The Stripe event envelope: a fresh evt_ id, the type and `created`.
function envelope(name, type, created) {
  const b = fixture(name);
  Object.assign(b, { id: `evt_e2e_${++eventNo}`, type, created });
  return b;
}

// A customer.subscription.* event: `sub` (customer cus_<sub>), the price of lookup key `key`.
function subscriptionEvent(type, sub, key, status, quantity, created, metadata) {
  const b = envelope("subscription", type, created);
  Object.assign(b.data.object, { id: sub, customer: `cus_${sub}`, status, metadata });
  if (status === "canceled") Object.assign(b.data.object, { ended_at: created, canceled_at: created });
  const item = b.data.object.items.data[0];
  Object.assign(item, { id: `si_${sub}`, quantity, subscription: sub });
  Object.assign(item.price, { id: priceOf(key), lookup_key: key });
  return b;
}

function invoiceEvent(type, sub, created) {
  const b = envelope("invoice", type, created);
  b.data.object.customer = `cus_${sub}`;
  b.data.object.parent.subscription_details.subscription = sub;
  b.data.object.status = type === "invoice.payment_failed" ? "open" : "paid";
  return b;
}

function refundEvent(sub, created) {
  const b = envelope("charge", "charge.refunded", created);
  Object.assign(b.data.object, { customer: `cus_${sub}`, refunded: true, amount_refunded: b.data.object.amount });
  return b;
}

// Managed Payments refuses these on a Checkout Session (the owner's Dashboard list).
const REFUSED = ["automatic_tax", "tax_id_collection", "subscription_data[default_tax_rates]", "payment_method_collection", "payment_method_configuration", "payment_method_options", "payment_method_types", "saved_payment_method_options", "customer_update[name]", "customer_update[address]", "shipping_address_collection", "shipping_options", "subscription_data[application_fee_percent]", "subscription_data[on_behalf_of]", "subscription_data[transfer_data]", "subscription_data[invoice_settings]"];
const refusedKeys = (form) => [...form.keys()].filter((k) => REFUSED.some((r) => k === r || k.startsWith(`${r}[`)));
const metadataOf = (form, prefix) => Object.fromEntries([...form.entries()].filter(([k]) => k.startsWith(`${prefix}[`)).map(([k, v]) => [k.slice(prefix.length + 1, -1), v]));
const T0 = 1793700000;

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
  const stripe = await fakeStripe(await freePort());
  const dns = await fakeDns(await freePort());
  const keyDir = fs.mkdtempSync(path.join(os.tmpdir(), "hitop-portal-hostkey-"));
  const hostKey = path.join(keyDir, "host.key");
  fs.writeFileSync(hostKey, crypto.randomBytes(32).toString("hex"), { mode: 0o600 });
  const server = await syncServer(origin, {
    HITOP_SYNC_HOST_KEY_FILE: hostKey,
    HITOP_SYNC_DNS_URL: dns.url,
    // Only the two secrets: the prices are found by lookup key.
    HITOP_SYNC_STRIPE_SECRET_KEY: ST.key,
    HITOP_SYNC_STRIPE_WEBHOOK_SECRET: ST.secret,
    HITOP_SYNC_STRIPE_API_URL: stripe.url,
  });
  // A webhook delivery as Stripe sends it: the raw bytes, Stripe-Signature t=<now>,v1=<HMAC of "t.body">.
  const deliver = async (body, v1) => {
    const raw = JSON.stringify(body);
    const t = Math.floor(Date.now() / 1000);
    const sig = v1 ?? crypto.createHmac("sha256", ST.secret).update(`${t}.${raw}`).digest("hex");
    const r = await fetch(`${server.url}/v1/billing/stripe`, { method: "POST", headers: { "Content-Type": "application/json", "Stripe-Signature": `t=${t},v1=${sig}` }, body: raw });
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

    // No server field: the hosted server, or ?server= (kept with the account, even after sign out).
    await page.goto(`${origin}/?server=${encodeURIComponent(server.url)}`);
    await app.getByRole("button", { name: "Use a code instead" }).click();
    check("the sign-in page has no server field", (await app.getByLabel("Sync server").count()) === 0);
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
    await page.waitForURL(`${stripe.url}/checkout/**`, { timeout: 10000 });
    const made = stripe.seen.find((r) => r.method === "POST" && r.path === "/v1/checkout/sessions");
    const custom = made ? metadataOf(made.form, "subscription_data[metadata]") : {};
    check(
      "Buy Team opens a Managed Payments Checkout Session for 3 seats of the org",
      made?.authorized && made.form.get("managed_payments[enabled]") === "true" && made.form.get("mode") === "subscription" && custom.org_id === orgId && made.form.get("client_reference_id") === orgId && made.form.get("line_items[0][quantity]") === "3" && made.form.get("line_items[0][price]") === priceOf("team_yearly"),
      made && made.form.toString(),
    );
    check("...with nothing Managed Payments refuses", made && refusedKeys(made.form).length === 0, made && refusedKeys(made.form).join());
    const bad = await deliver(subscriptionEvent("customer.subscription.created", "sub_e2e1", "team_yearly", "active", 3, T0, custom), "0".repeat(64));
    check("a webhook with a bad signature is refused", bad.status === 401, JSON.stringify(bad));
    const createdEvent = subscriptionEvent("customer.subscription.created", "sub_e2e1", "team_yearly", "active", 3, T0, custom);
    const created = await deliver(createdEvent);
    const again = await deliver(createdEvent);
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
    const update = stripe.seen.find((r) => r.method === "POST" && r.path.startsWith("/v1/subscriptions/"));
    check("a 4th seat: quantity 4 sent to Stripe, invoiced at once", update?.path === "/v1/subscriptions/sub_e2e1" && update.form.get("items[0][id]") === "si_sub_e2e1" && update.form.get("items[0][quantity]") === "4" && update.form.get("proration_behavior") === "always_invoice", update && update.form.toString());
    const dave = await server.api("POST", `/v1/orgs/${orgId}/members`, bobSession, { email: "dave@example.com", role: "member" });
    check("...and the 4th member fits", dave.status === 201, JSON.stringify(dave.json));
    await deliver(subscriptionEvent("customer.subscription.updated", "sub_e2e1", "team_yearly", "active", 4, T0 + 3601, custom));
    // Payment failed: the banner, still Team.
    await deliver(invoiceEvent("invoice.payment_failed", "sub_e2e1", T0 + 365 * 86400));
    await deliver(subscriptionEvent("customer.subscription.updated", "sub_e2e1", "team_yearly", "past_due", 4, T0 + 365 * 86400 + 1, custom));
    await page.reload();
    await idle();
    check("payment failed: the banner, still Team", (await app.getByLabel("Payment failed").count()) === 1 && (await app.innerText()).includes("Plan: team"), await app.innerText());
    // The billing portal opens a Stripe customer portal session for the org's customer.
    await app.getByRole("button", { name: "Manage billing" }).click();
    await page.waitForURL(`${stripe.url}/billing/**`, { timeout: 10000 });
    check("Manage billing opens the Stripe customer portal", page.url().endsWith("/billing/session_cus_sub_e2e1"), page.url());
    // A full refund ends the plan; deleted (ended) is Free too; nothing is removed.
    const refunded = await deliver(refundEvent("sub_e2e1", T0 + 366 * 86400));
    await page.goto(`${origin}/#/orgs/${orgId}/plan`);
    await idle();
    check("a refund: Free limits", refunded.json.outcome === "applied" && (await alertText()).includes("refunded: Free limits"), await alertText());
    const expired = await deliver(subscriptionEvent("customer.subscription.deleted", "sub_e2e1", "team_yearly", "canceled", 4, T0 + 380 * 86400, custom));
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
    await page.waitForURL(`${stripe.url}/checkout/**`, { timeout: 10000 });
    const bizCheckout = stripe.seen.filter((r) => r.method === "POST" && r.path === "/v1/checkout/sessions").pop();
    const bizCustom = bizCheckout ? metadataOf(bizCheckout.form, "subscription_data[metadata]") : {};
    check("Buy Business: the Business price, 3 seats, Globex", bizCustom.org_id === globex && bizCheckout.form.get("line_items[0][price]") === priceOf("business_yearly") && bizCheckout.form.get("line_items[0][quantity]") === "3", bizCheckout && bizCheckout.form.toString());
    const biz = await deliver(subscriptionEvent("customer.subscription.created", "sub_e2e2", "business_yearly", "active", 3, T0 + 7200, bizCustom));
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
    const laptop = after.devices.find((d) => d.name === "bob-laptop");
    check(
      "a device removed from the org's workspaces only, after a warning",
      dialog.includes("removed from this organization's workspaces") && dialog.includes("aren't touched") && laptop?.orgWorkspaces === 0,
      dialog + JSON.stringify(after.devices),
    );

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

    // HITOP Cloud C6: SCIM. Off until an owner creates a token; the IdP (here: fetch) provisions a
    // user and a group; the admin sets what the group gives; revoking ends provisioning.
    await page.goto(`${origin}/#/orgs/${globex}`);
    await idle();
    await app.getByRole("link", { name: "SCIM" }).click();
    await idle();
    check("SCIM: off until a token exists, base URL shown", (await app.innerText()).includes("Off: no token yet") && (await app.getByLabel("SCIM base URL").innerText()) === `${server.url}/scim/v2`, (await app.innerText()).slice(0, 400) + (await alertText()));
    await app.getByLabel("Token name (your identity provider)").fill("Okta");
    await app.getByRole("button", { name: "Create SCIM token" }).click();
    await idle();
    const scimToken = (await app.getByLabel("New SCIM token").innerText().catch(() => "")).trim();
    check("SCIM: an owner creates a token, shown once", scimToken.startsWith("hitop_scim_") && (await app.getByRole("table", { name: "SCIM tokens" }).innerText()).includes("Okta"), scimToken + (await alertText()));
    const scimDave = await server.api("POST", "/scim/v2/Users", scimToken, { schemas: ["urn:ietf:params:scim:schemas:core:2.0:User"], userName: "dave@example.com", active: true });
    const qa = await server.api("POST", "/scim/v2/Groups", scimToken, { displayName: "QA", members: [{ value: scimDave.json?.id }] });
    check("SCIM: the IdP provisions a user (invited: no account yet) and a group", scimDave.status === 201 && qa.status === 201 && (await server.api("GET", `/v1/orgs/${globex}/members`, adaSession)).json.invites.some((i) => i.email === "dave@example.com"), JSON.stringify([scimDave.json, qa.json]));
    await page.reload();
    await idle();
    check("SCIM: the token isn't shown again", !(await app.innerText()).includes(scimToken));
    await app.getByLabel("Organization role of QA").selectOption("admin");
    await idle();
    await app.getByLabel("Workspace for QA").selectOption({ label: wsId.slice(0, 8) });
    await app.getByLabel("Workspace role for QA").selectOption("viewer");
    await app.getByLabel("Group QA").getByRole("button", { name: "Give workspace" }).click();
    await idle();
    const qaRoles = (await server.api("GET", `/v1/orgs/${globex}/scim`, adaSession)).json.groups.find((g) => g.displayName === "QA");
    check("SCIM: a group gives an org role and a workspace role", qaRoles?.orgRole === "admin" && qaRoles.workspaces[0]?.workspace === wsId && qaRoles.workspaces[0].role === "viewer" && (await app.getByLabel("Workspaces of QA").innerText()).includes("viewer"), JSON.stringify(qaRoles) + (await alertText()));
    check("SCIM: the invited address waits for the workspace owner", (await server.api("GET", `/v1/workspaces/${wsId}/invites`, bobSession)).json.invites.some((i) => i.email === "dave@example.com" && i.role === "viewer"));
    dialog = "";
    page.once("dialog", (d) => {
      dialog = d.message();
      d.accept();
    });
    await app.getByRole("table", { name: "SCIM tokens" }).getByRole("row").filter({ hasText: "Okta" }).getByRole("button", { name: "Revoke" }).click();
    await idle();
    check("SCIM: revoking a token ends provisioning", dialog.includes("stops provisioning") && (await server.api("GET", "/scim/v2/Users", scimToken)).status === 401, dialog);

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
    stripe.server.close();
    dns.server.close();
    server.stop();
    fs.rmSync(keyDir, { recursive: true, force: true });
  }
  const failed = results.filter((r) => !r.ok);
  console.log(`${results.length - failed.length}/${results.length} passed`);
  process.exit(failed.length ? 1 : 0);
})();
