// Visual check: screenshots of the key screens at 1440×900 (the HITOP Next
// boards' size), seeded with a realistic collection, into a gitignored folder:
//   node tests/visual.cjs [out dir]            (default: visual/<time>/)
// Serves dist/ itself (build first: npm run build), so no hitop-server is
// needed; sends are not made (a seeded tab already carries a response).
// Compare the pictures with the design boards (HITOP_NEXT_UI.md) by eye, or
// diff two runs.
const { chromium } = require("playwright");
const fs = require("fs");
const http = require("http");
const path = require("path");

const DIST = path.join(__dirname, "../dist");
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".png": "image/png", ".json": "application/json" };
const out = process.argv[2] || path.join(__dirname, "../visual", new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19));

// --- seed: the "shop-api" collection of the boards --------------------------------------
const COL = 7001;
const req = (id, method, name, file, url, extra = {}) => ({ id, name, method, url, file, headers: [], body: "", preRequestScript: "", postRequestScript: "", ...extra });
const BASE = "${baseUrl}";
const requests = [
  req(1, "POST", "Log in", "auth/log-in", `${BASE}/v2/auth/login`, { body: '{"email": "${email}", "password": "${password}"}' }),
  req(2, "GET", "Me", "auth/me", `${BASE}/v2/me`),
  req(3, "GET", "List orders", "orders/list-orders", `${BASE}/v2/orders?expand=items,customer`),
  req(4, "POST", "Create order", "orders/create-order", `${BASE}/v2/orders?expand=items,customer`, {
    headers: [{ name: "Content-Type", value: "application/json" }, { name: "Idempotency-Key", value: "${$guid}" }, { name: "Authorization", value: "Bearer ${token}" }],
    body: '{\n  "customerId": "${customerId}",\n  "currency": "EUR",\n  "items": [\n    { "sku": "TEE-0193", "qty": 2 },\n    { "sku": "CAP-0007", "qty": 1 }\n  ],\n  "shipping": {\n    "method": "express",\n    "address": "${$faker.address}"\n  }\n}',
    openapi: { operation: "POST /v2/orders", operationId: "createOrder" },
    postRequestScript: 'pm.test("Status is 201", () => pm.response.to.have.status(201));\npm.test("Total equals sum of items", () => pm.expect(pm.response.json().total.amount).to.eql(6480));\npm.test("Responds under 300 ms", () => pm.expect(pm.response.responseTime).to.be.below(300));\npm.test("Has a payment link", () => pm.expect(pm.response.json().links.pay).to.be.a("string"));',
  }),
  req(5, "GET", "Get order", "orders/get-order", `${BASE}/v2/orders/${"${orderId}"}`),
  req(6, "PATCH", "Update shipping", "orders/update-shipping", `${BASE}/v2/orders/${"${orderId}"}/shipping`, { body: '{"method": "express"}' }),
  req(7, "DELETE", "Cancel order", "orders/cancel-order", `${BASE}/v2/orders/${"${orderId}"}`),
  req(8, "GET", "Order events", "orders/order-events", `wss://staging.shop.example/v2/orders/events`, { kind: "websocket", websocket: { subprotocols: [], messages: [] } }),
  req(9, "POST", "Catalog query", "catalog/catalog-query", `${BASE}/graphql`, { bodyType: "graphql", body: "query Catalog { products(first: 10) { sku name price } }", graphql: { variables: "{}", operationName: "Catalog" } }),
  req(10, "GET", "Product by SKU", "catalog/product-by-sku", `${BASE}/v2/products/TEE-0193`),
  req(11, "GET", "Stock stream", "catalog/stock-stream", `${BASE}/v2/stock/stream`, { headers: [{ name: "Accept", value: "text/event-stream" }] }),
  req(12, "POST", "Pay order", "payments/pay-order", `${BASE}/v2/orders/${"${orderId}"}/pay`),
  req(13, "GET", "Invoice PDF", "payments/invoice-pdf", `${BASE}/v2/orders/${"${orderId}"}/invoice.pdf`),
];
const collections = [{ id: COL, name: "shop-api", variables: { customerId: "cus_3Jd82" }, requests }];
const environments = [
  { id: "env-staging", name: "Staging", variables: { baseUrl: "https://staging.shop.example", token: "sk_test_4eC39", email: "dev@shop.example", password: "hunter2", orderId: "ord_8KQ2mZ41" }, secrets: ["token", "password"] },
  { id: "env-local", name: "Local", variables: { baseUrl: "http://localhost:8080", token: "dev", email: "dev@shop.example", password: "dev", orderId: "ord_1" } },
];
const response = '{\n  "id": "ord_8KQ2mZ41",\n  "status": "pending_payment",\n  "total": { "amount": 6480, "currency": "EUR" },\n  "items": [\n    { "sku": "TEE-0193", "qty": 2, "price": 2490 },\n    { "sku": "CAP-0007", "qty": 1, "price": 1500 }\n  ],\n  "customer": { "id": "cus_3Jd82", "email": "dev@shop.example", "name": "Dev Shop" },\n  "createdAt": "2026-11-09T10:42:18Z",\n  "links": {\n    "pay": "/v2/orders/ord_8KQ2mZ41/pay"\n  }\n}';
const tabOf = (r, extra = {}) => ({ id: 9000 + r.id, title: r.name, url: r.url, method: r.method, headers: r.headers, requestBody: r.body, body: r.body, preRequestScript: r.preRequestScript, postRequestScript: r.postRequestScript, bodyMode: r.bodyType === "graphql" ? "graphql" : "raw", kind: r.kind, loadedRequestId: r.id, loadedCollectionId: COL, response: null, responseHeaders: null, statusCode: null, responseType: "", ...extra });
const tabs = [
  tabOf(requests[3], {
    response,
    responseType: "json",
    statusCode: 201,
    durationMs: 142,
    timings: { dnsMs: 4, connectMs: 30, waitMs: 96, downloadMs: 12, totalMs: 142, reused: false },
    // As the desktop and the web proxy answer for an HTTPS host speaking HTTP/2.
    httpVersion: "HTTP/2",
    responseScheme: "https",
    responseHeaders: { "content-type": "application/json; charset=utf-8", "content-length": "1268", date: "Mon, 09 Nov 2026 10:42:18 GMT", "x-request-id": "req_01HF8", "cache-control": "no-store", vary: "Accept-Encoding", server: "shop-api/2.14", "strict-transport-security": "max-age=63072000", "x-ratelimit-limit": "600", "x-ratelimit-remaining": "598", "content-encoding": "gzip", connection: "keep-alive" },
    tests: [
      { name: "Status is 201", pass: true, detail: "" },
      { name: "Total equals sum of items", pass: true, detail: "" },
      { name: "Responds under 300 ms", pass: true, detail: "" },
      { name: "Has a payment link", pass: true, detail: "" },
    ],
  }),
  tabOf(requests[2]),
  tabOf(requests[7]),
  tabOf(requests[8]),
];
const runHistory = {
  [COL]: [
    { at: "2026-11-08T10:40:00Z", requests: 13, failed: 0, checks: 4, checksPassed: 4, p95: 211, wallMs: 2500 },
  ],
};
const seed = {
  hitop_collections: JSON.stringify(collections),
  hitop_environments: JSON.stringify(environments),
  hitop_active_environment: "env-staging",
  hitop_tabs: JSON.stringify(tabs),
  hitop_current_tab: "0",
  hitop_history: JSON.stringify([
    { id: 1, timestamp: "2026-11-09T10:42:18Z", url: "https://staging.shop.example/v2/orders", method: "POST", statusCode: 201, success: true, headers: [], body: "", response: { status: 201, headers: {}, body: response, durationMs: 142 } },
    { id: 2, timestamp: "2026-11-09T10:41:02Z", url: "https://staging.shop.example/v2/orders", method: "POST", statusCode: 201, success: true, headers: [], body: "", response: { status: 201, headers: {}, body: response.replace("6480", "6400"), durationMs: 160 } },
    { id: 3, timestamp: "2026-11-09T10:40:40Z", url: "https://staging.shop.example/v2/me", method: "GET", statusCode: 200, success: true, headers: [], body: "" },
  ]),
  hitop_run_history: JSON.stringify(runHistory),
  hitop_contexts: "{}",
  hitop_collapsed_collections: JSON.stringify([`${COL}/auth`, `${COL}/payments`]),
};

// --- a static server for dist/ at /app/ ------------------------------------------------
function serve() {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const url = new URL(req.url, "http://x");
      // The Sign in dialog's server (hitop_settings.syncServer = this origin): GitHub and Google on,
      // no single sign-on, GitHub's start answers with a page of this server.
      if (url.pathname === "/v1/auth/methods") {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end('{"password":true,"emailCode":true,"passkey":false,"sso":false,"google":true,"github":true}');
      }
      if (url.pathname === "/v1/auth/github/start" || url.pathname === "/v1/auth/google/start") {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify({ url: `http://${req.headers.host}/provider`, expiresAt: 0 }));
      }
      if (url.pathname === "/provider") {
        res.writeHead(200, { "content-type": "text/html" });
        return res.end("<p>The provider's sign-in page</p>");
      }
      if (url.pathname.startsWith("/v1/")) {
        res.writeHead(404, { "content-type": "application/json" });
        return res.end('{"error":"not_found","message":"not found"}');
      }
      if (url.pathname.startsWith("/api/")) {
        res.writeHead(502, { "content-type": "application/json" });
        return res.end('{"error":"no proxy in the visual check"}');
      }
      const rel = decodeURIComponent(url.pathname).replace(/^\/app\/?/, "") || "index.html";
      let file = path.join(DIST, rel);
      if (!file.startsWith(DIST) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) file = path.join(DIST, "index.html");
      res.writeHead(200, { "content-type": TYPES[path.extname(file)] || "application/octet-stream" });
      res.end(fs.readFileSync(file));
    });
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

async function main() {
  if (!fs.existsSync(path.join(DIST, "index.html"))) throw new Error("dist/ is missing: run npm run build first");
  fs.mkdirSync(out, { recursive: true });
  const server = await serve();
  const base = `http://127.0.0.1:${server.address().port}/app/`;
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, colorScheme: "dark" });
  await ctx.addInitScript((f) => {
    if (!sessionStorage.getItem("seeded")) {
      for (const [k, v] of Object.entries(f)) localStorage.setItem(k, v);
      sessionStorage.setItem("seeded", "1");
    }
  }, seed);
  const page = await ctx.newPage();
  const shot = async (name) => {
    await page.waitForTimeout(250);
    await page.screenshot({ path: path.join(out, `${name}.png`) });
    console.log(`${name}.png`);
  };
  await page.goto(base);
  await page.waitForSelector(".request-line, hitop-request");
  await page.waitForTimeout(600);
  await shot("workspace");

  // Response tabs: Headers, Checks.
  const rtab = (name) => page.locator('[aria-label="Response details"] lit-material-tab', { hasText: name }).first();
  if (await rtab("Headers").count()) {
    await rtab("Headers").click();
    await shot("workspace-headers");
    await rtab("Body").click();
  }
  // Request tabs: Params, Headers.
  const qtab = (name) => page.locator('[aria-label="Request details"] lit-material-tab', { hasText: name }).first();
  await qtab("Params").click();
  await shot("workspace-params");
  await qtab("Body").click();

  // A fresh tab: the empty response state.
  await page.locator('[aria-label="Open requests"] lit-material-tab', { hasText: "List orders" }).click();
  await shot("workspace-empty-response");
  await page.locator('[aria-label="Open requests"] lit-material-tab', { hasText: "Order events" }).click();
  await shot("workspace-websocket");
  await page.locator('[aria-label="Open requests"] lit-material-tab', { hasText: "Create order" }).click();

  // Palette.
  await page.keyboard.press("Control+k");
  await page.waitForTimeout(300);
  await page.keyboard.type("order");
  await shot("palette");
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);

  // A dialog with fields (New collection).
  await page.locator("aside").getByRole("button", { name: "New collection", exact: true }).click();
  await page.waitForSelector("#dialog lit-material-text-field");
  await shot("dialog");
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);

  // Collection page and its run report.
  await page.locator('.tree-row.collection', { hasText: "shop-api" }).click();
  await shot("collection");
  // A run (every send fails here: the static server has no proxy), for the report's shape.
  await page.getByRole("button", { name: "Run collection" }).click();
  await page.waitForSelector(".run-cli", { timeout: 60000 }).catch(() => {});
  await shot("run-report");
  // Environments, History, Settings.
  for (const [label, name] of [["Environments", "environments"], ["History", "history"], ["Settings", "settings"]]) {
    await page.locator(`lit-material-navigation-rail-item`, { hasText: label }).click();
    await shot(name);
  }

  // Stories audit (S1): a secret with no value on this device, before and after Send.
  await page.evaluate(() => {
    const s = document.querySelector("hitop-app").store;
    s.environments = s.environments.map((e) => (e.id === "env-staging" ? { ...e, secrets: [...new Set([...(e.secrets || []), "API_TOKEN"])], variables: { ...e.variables, API_TOKEN: "" } } : e));
    s.setActiveEnvironment("env-staging");
    s.show("request");
    s.updateTab({ url: "${baseUrl}/v2/orders?token=${API_TOKEN}" });
    s.emit();
  });
  await page.waitForTimeout(400);
  await shot("secret-missing");
  await page.getByRole("button", { name: "Send", exact: true }).click();
  await page.waitForSelector('.response[data-state="not-sent"]', { timeout: 10000 }).catch(() => {});
  await shot("secret-not-sent");
  // Plan usage near a limit and the limit dialog (a made-up org; no server: the owner by org name).
  await page.evaluate(() => {
    const app = document.querySelector("hitop-app");
    const s = app.store;
    s.orgs = [{ id: "o-acme", name: "Acme", role: "member", plan: "free", personal: false }];
    s.currentOrg = "o-acme";
    s.setOrgUsage("o-acme", { plan: "free", planInForce: "free", usage: { seats: 3, workspaces: 2, storageBytes: 1024, devices: 1, runners: 0 }, limits: { seats: 3, guests: 0, workspaces: 3, storageBytes: 100 * 1024 ** 2, devices: 3, runners: 5 } });
    app.openLimit({ limit: "seats", plan: "free", used: 3, max: 3, org: "o-acme", message: "plan limit: the free plan of the organization Acme allows 3 seats; nothing was changed.", headline: "The Free plan has used its 3 seats" }, null);
  });
  await page.waitForTimeout(500);
  await shot("limit-dialog");
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);

  // The Sign in dialog (2026-11-11): providers, waiting for the pop-up, the second step with an
  // inline error (this server has no single sign-on).
  await page.evaluate((server) => {
    const s = JSON.parse(localStorage.getItem("hitop_settings") || "{}");
    localStorage.setItem("hitop_settings", JSON.stringify({ ...s, syncServer: server }));
    const app = document.querySelector("hitop-app");
    app.store.orgs = [];
    app.store.currentOrg = null;
    app.requestUpdate();
  }, `http://127.0.0.1:${server.address().port}`);
  await page.locator("header.topbar").getByRole("button", { name: "Sign in", exact: true }).click();
  await page.waitForSelector('#dialog .signin[data-step="providers"]', { timeout: 10000 });
  await shot("sign-in");
  const popup = ctx.waitForEvent("page");
  await page.locator("#dialog").getByRole("button", { name: "Continue with GitHub" }).click();
  await page.waitForSelector('#dialog .signin[data-step="waiting"]', { timeout: 10000 });
  await (await popup).close();
  await shot("sign-in-waiting");
  await page.locator("#dialog").getByRole("button", { name: "Cancel" }).click();
  await page.locator("#dialog").getByRole("button", { name: "Company SSO or your own server…" }).click();
  await page.locator("#dialog").getByLabel("Work email").fill("ada@acme.example");
  await page.locator("#dialog").getByRole("button", { name: "Continue with SSO" }).click();
  await page.waitForSelector('#dialog #signin-server[error]', { timeout: 10000 }).catch(() => {});
  await page.bringToFront();
  await shot("sign-in-sso-error");
  await browser.close();

  // A newcomer: an empty app with the Get started card.
  const fresh = await chromium.launch();
  const fctx = await fresh.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, colorScheme: "dark" });
  const fpage = await fctx.newPage();
  await fpage.goto(base);
  await fpage.waitForSelector("aside .getting-started", { timeout: 10000 }).catch(() => {});
  await fpage.waitForTimeout(500);
  await fpage.screenshot({ path: path.join(out, "getting-started.png") });
  console.log("getting-started.png");
  await fresh.close();
  server.close();
  console.log(`Screenshots in ${out}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
