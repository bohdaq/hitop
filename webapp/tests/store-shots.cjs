// Store listing screenshots (Chrome Web Store and Firefox Add-ons): five 1280×800 PNGs of the web
// app, dark theme, seeded with the "shop-api" workspace of tests/visual.cjs, a fixed clock and
// every request answered by a mock of hitop-server's /api/proxy (nothing leaves the machine).
//   npm run build
//   cp -r dist ../deploy/out/www/app        (served by ../hitop-server on BASE, as the e2e suites do)
//   BASE=http://localhost:7889 node tests/store-shots.cjs [out dir]   (default ../launch/store-assets/)
const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const BASE = process.env.BASE || "http://localhost:7889";
const out = path.resolve(process.argv[2] || path.join(__dirname, "../../launch/store-assets"));
const NOW = new Date("2026-11-16T10:42:00Z");

// --- seed: the shop-api workspace --------------------------------------------------------
const COL = 7001;
const V = (n) => "${" + n + "}";
const req = (id, method, name, file, url, extra = {}) => ({ id, name, method, url, file, headers: [], body: "", preRequestScript: "", postRequestScript: "", ...extra });
const test = (...lines) => lines.join("\n");
const requests = [
  req(1, "POST", "Log in", "auth/log-in", `${V("baseUrl")}/v2/auth/login`, {
    headers: [{ name: "Content-Type", value: "application/json" }],
    body: `{"email": "${V("email")}", "password": "${V("password")}"}`,
    postRequestScript: test('pm.test("Status is 200", () => pm.response.to.have.status(200));', 'pm.environment.set("token", pm.response.json().token);'),
  }),
  req(2, "GET", "Me", "auth/me", `${V("baseUrl")}/v2/me`, { postRequestScript: 'pm.test("Signed in", () => pm.expect(pm.response.json().email).to.eql("dev@shop.example"));' }),
  req(3, "GET", "List orders", "orders/list-orders", `${V("baseUrl")}/v2/orders?expand=items,customer`, { postRequestScript: 'pm.test("Has orders", () => pm.expect(pm.response.json().data.length).to.be.above(0));' }),
  req(4, "POST", "Create order", "orders/create-order", `${V("baseUrl")}/v2/orders?expand=items,customer`, {
    headers: [{ name: "Content-Type", value: "application/json" }, { name: "Idempotency-Key", value: "4f9c2b1e-7d3a-4e8b-9a61-0c5d2f8e7b14" }, { name: "Authorization", value: `Bearer ${V("token")}` }],
    body: '{\n  "customerId": "${customerId}",\n  "currency": "EUR",\n  "items": [\n    { "sku": "TEE-0193", "qty": 2 },\n    { "sku": "CAP-0007", "qty": 1 }\n  ],\n  "shipping": { "method": "express" }\n}',
    openapi: { operation: "POST /v2/orders", operationId: "createOrder" },
    postRequestScript: test(
      'pm.test("Status is 201", () => pm.response.to.have.status(201));',
      'pm.test("Total equals sum of items", () => pm.expect(pm.response.json().total.amount).to.eql(6480));',
      'pm.test("Responds under 300 ms", () => pm.expect(pm.response.responseTime).to.be.below(300));',
      'pm.test("Has a payment link", () => pm.expect(pm.response.json().links.pay).to.be.a("string"));',
    ),
  }),
  req(5, "GET", "Get order", "orders/get-order", `${V("baseUrl")}/v2/orders/${V("orderId")}`, { headers: [{ name: "Accept", value: "application/json" }, { name: "Authorization", value: `Bearer ${V("token")}` }], postRequestScript: 'pm.test("Status is 200", () => pm.response.to.have.status(200));' }),
  req(6, "PATCH", "Update shipping", "orders/update-shipping", `${V("baseUrl")}/v2/orders/${V("orderId")}/shipping`, { headers: [{ name: "Content-Type", value: "application/json" }], body: '{"method": "express"}', postRequestScript: 'pm.test("Express", () => pm.expect(pm.response.json().shipping.method).to.eql("express"));' }),
  req(7, "GET", "Order events", "orders/order-events", "wss://staging.shop.example/v2/orders/events", { kind: "websocket", websocket: { subprotocols: [], messages: [] } }),
  req(8, "POST", "Catalog query", "catalog/catalog-query", `${V("baseUrl")}/graphql`, {
    bodyType: "graphql",
    body: "query Catalog($first: Int!) {\n  products(first: $first) {\n    sku\n    name\n    price { amount currency }\n    inStock\n  }\n}",
    graphql: { variables: '{\n  "first": 3\n}', operationName: "Catalog" },
    postRequestScript: 'pm.test("Three products", () => pm.expect(pm.response.json().data.products.length).to.eql(3));',
  }),
  req(9, "GET", "Product by SKU", "catalog/product-by-sku", `${V("baseUrl")}/v2/products/TEE-0193`, { postRequestScript: 'pm.test("In stock", () => pm.expect(pm.response.json().inStock).to.be.true);' }),
  req(10, "POST", "Pay order", "payments/pay-order", `${V("baseUrl")}/v2/orders/${V("orderId")}/pay`, { postRequestScript: 'pm.test("Paid", () => pm.expect(pm.response.json().status).to.eql("paid"));' }),
  req(11, "GET", "Invoice", "payments/invoice", `${V("baseUrl")}/v2/orders/${V("orderId")}/invoice`, { postRequestScript: 'pm.test("Status is 200", () => pm.response.to.have.status(200));' }),
  req(12, "DELETE", "Cancel order", "orders/cancel-order", `${V("baseUrl")}/v2/orders/${V("orderId")}`, { postRequestScript: 'pm.test("No content", () => pm.response.to.have.status(204));' }),
];
const hooks = [
  req(101, "POST", "Order paid", "events/order-paid", `${V("hooksUrl")}/hooks/order-paid`, { headers: [{ name: "Content-Type", value: "application/json" }], body: '{"type": "order.paid", "orderId": "ord_8KQ2mZ41"}' }),
  req(102, "POST", "Refund issued", "events/refund-issued", `${V("hooksUrl")}/hooks/refund-issued`),
  req(103, "GET", "Health", "health", `${V("hooksUrl")}/healthz`),
];
const collections = [
  { id: COL, name: "shop-api", variables: { customerId: "cus_3Jd82" }, requests, folders: ["auth", "orders", "catalog", "payments"] },
  { id: 7002, name: "webhooks", variables: { hooksUrl: "https://hooks.shop.example" }, requests: hooks, folders: ["events"] },
];
const environments = [
  { id: "env-staging", name: "Staging", variables: { baseUrl: "https://staging.shop.example", token: "demo-staging-token-0193", email: "dev@shop.example", password: "hunter2", orderId: "ord_8KQ2mZ41" }, secrets: ["token", "password"] },
  { id: "env-local", name: "Local", variables: { baseUrl: "http://localhost:8080", token: "dev", email: "dev@shop.example", password: "dev", orderId: "ord_1" } },
  { id: "env-prod", name: "Production", variables: { baseUrl: "https://api.shop.example", token: "", email: "", password: "", orderId: "" }, secrets: ["token", "password"] },
];
const tabOf = (r) => ({ id: 9000 + r.id, title: r.name, url: r.url, method: r.method, headers: r.headers, requestBody: r.body, body: r.body, preRequestScript: r.preRequestScript, postRequestScript: r.postRequestScript, bodyMode: r.bodyType === "graphql" ? "graphql" : "raw", graphqlVariables: r.graphql?.variables, graphqlOperationName: r.graphql?.operationName, kind: r.kind, loadedRequestId: r.id, loadedCollectionId: COL, response: null, responseHeaders: null, statusCode: null, responseType: "" });
const tabs = [tabOf(requests[3]), tabOf(requests[2]), tabOf(requests[7]), tabOf(requests[4])];
const seed = {
  hitop_collections: JSON.stringify(collections),
  hitop_environments: JSON.stringify(environments),
  hitop_active_environment: "env-staging",
  hitop_globals: JSON.stringify({ variables: { userAgent: "hitop-store-shots" }, secrets: [] }),
  hitop_tabs: JSON.stringify(tabs),
  hitop_current_tab: "0",
  hitop_history: "[]",
  hitop_run_history: JSON.stringify({ [COL]: [
    { at: "2026-11-14T09:10:00Z", requests: 11, failed: 1, checks: 13, checksPassed: 12, p95: 238, wallMs: 2900 },
    { at: "2026-11-15T09:10:00Z", requests: 11, failed: 0, checks: 13, checksPassed: 13, p95: 205, wallMs: 2600 },
  ] }),
  hitop_contexts: "{}",
  hitop_collapsed_collections: "[]",
};

// --- the mock proxy: canned answers per method and path ----------------------------------
const order = { id: "ord_8KQ2mZ41", status: "pending_payment", total: { amount: 6480, currency: "EUR" }, items: [{ sku: "TEE-0193", qty: 2, price: 2490 }, { sku: "CAP-0007", qty: 1, price: 1500 }], customer: { id: "cus_3Jd82", email: "dev@shop.example", name: "Dev Shop" }, shipping: { method: "express", eta: "2026-11-18" }, createdAt: "2026-11-16T10:42:00Z", links: { pay: "/v2/orders/ord_8KQ2mZ41/pay", invoice: "/v2/orders/ord_8KQ2mZ41/invoice" } };
const products = [
  { sku: "TEE-0193", name: "Logo tee", price: { amount: 2490, currency: "EUR" }, inStock: true },
  { sku: "CAP-0007", name: "Dad cap", price: { amount: 1500, currency: "EUR" }, inStock: true },
  { sku: "MUG-0042", name: "Enamel mug", price: { amount: 1200, currency: "EUR" }, inStock: false },
];
function answer(method, url, body) {
  const p = new URL(url).pathname;
  if (p === "/graphql") {
    const vars = JSON.parse(body || "{}").variables || {};
    return [200, { data: { products: products.slice(0, vars.first ?? 3) } }, 96];
  }
  if (p === "/v2/auth/login") return [200, { token: "demo-staging-token-0193", expiresIn: 3600 }, 88];
  if (p === "/v2/me") return [200, { id: "usr_51", email: "dev@shop.example", name: "Dev Shop", roles: ["admin"] }, 41];
  if (p === "/v2/orders" && method === "GET") return [200, { data: [order, { ...order, id: "ord_7Hc1xP02", status: "paid" }], hasMore: false }, 64];
  if (p === "/v2/orders" && method === "POST") return [201, order, 142];
  if (p.endsWith("/shipping")) return [200, { ...order, shipping: { method: "express", eta: "2026-11-18" } }, 73];
  if (p.endsWith("/pay")) return [200, { ...order, status: "paid" }, 187];
  if (p.endsWith("/invoice")) return [200, { orderId: order.id, number: "INV-2026-0042", total: order.total, pdf: "/v2/invoices/INV-2026-0042.pdf" }, 52];
  if (p.startsWith("/v2/orders/") && method === "DELETE") return [204, "", 39];
  if (p.startsWith("/v2/orders/")) return [200, order, 58];
  if (p.startsWith("/v2/products/")) return [200, products[0], 33];
  return [404, { error: "not_found" }, 20];
}
async function proxy(route) {
  if (process.env.DEBUG) console.log("proxy", route.request().postData()?.slice(0, 120));
  const r = JSON.parse(route.request().postData() || "{}");
  const [status, json, ms] = answer(r.method, r.url, r.body);
  const body = json === "" ? "" : JSON.stringify(json, null, 2);
  const envelope = {
    status,
    statusText: { 200: "OK", 201: "Created", 204: "No Content", 404: "Not Found" }[status],
    headers: [["content-type", "application/json; charset=utf-8"], ["content-length", String(body.length)], ["date", NOW.toUTCString()], ["x-request-id", "req_01HF8"], ["cache-control", "no-store"], ["server", "shop-api/2.14"], ["strict-transport-security", "max-age=63072000"], ["x-ratelimit-remaining", "598"]],
    body,
    bodyEncoding: "utf8",
    truncated: false,
    timeMs: ms,
    url: r.url,
    timings: { dnsMs: 4, connectMs: 30, waitMs: ms - 46, downloadMs: 12, totalMs: ms, reused: false },
    httpVersion: "HTTP/2",
  };
  await new Promise((res) => setTimeout(res, 60));
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(envelope) });
}

async function main() {
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1, colorScheme: "dark", locale: "en-US", timezoneId: "UTC" });
  // Only the app's own files: the proxy is mocked, everything else (sync server, fonts) refused.
  await ctx.route("**/*", (route) => {
    const u = new URL(route.request().url());
    if (u.origin === BASE && u.pathname === "/api/proxy") return proxy(route);
    if (u.origin === BASE && u.pathname.startsWith("/app/")) return route.continue();
    return route.abort();
  });
  await ctx.addInitScript((f) => {
    if (!sessionStorage.getItem("seeded")) {
      for (const [k, v] of Object.entries(f)) localStorage.setItem(k, v);
      sessionStorage.setItem("seeded", "1");
    }
  }, seed);
  const page = await ctx.newPage();
  await page.clock.install({ time: NOW });
  await page.clock.resume();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const shot = async (name) => {
    await page.mouse.move(0, 0).catch(() => {});
    await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(out, `${name}.png`) });
    console.log(`${name}.png`);
  };
  const quiet = () => page.evaluate(() => document.querySelector("#toast")?.close?.());
  const openTab = (name) => page.locator('[aria-label="Open requests"] lit-material-tab', { hasText: name }).first().click();
  const send = async () => {
    await page.getByRole("button", { name: "Send", exact: true }).click();
    await page.waitForSelector(".response-meta", { timeout: 20000 }).catch(async (e) => {
      await page.screenshot({ path: path.join(out, "failed.png") });
      throw e;
    });
    await page.waitForTimeout(600);
  };

  await page.goto(`${BASE}/app/`);
  await page.waitForSelector(".request-line");
  await page.waitForTimeout(800);
  await quiet();

  // 1. The workspace: Create order sent, the response and its passing checks.
  await send();
  await shot("screenshot-1-request");

  // 3. Environments and variables: Get order sent, its headers, the peek on ${orderId} from the Staging environment.
  await openTab("Get order");
  await page.waitForTimeout(300);
  await send();
  await page.getByRole("tablist", { name: "Request details" }).getByRole("tab", { name: /^Headers/ }).click();
  // The editor a little wider (the split's keyboard step), so the peek stays clear of the response.
  const split = page.locator('.split-handle[role="separator"]').first();
  await split.focus();
  for (let i = 0; i < 2; i++) await split.press("ArrowRight");
  await page.waitForTimeout(300);
  await page.locator(".var-chip", { hasText: "orderId" }).first().click();
  await page.waitForSelector(".var-peek:popover-open", { timeout: 5000 }).catch(() => {});
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(out, "screenshot-3-environments.png") });
  console.log("screenshot-3-environments.png");
  await page.keyboard.press("Escape");
  await split.focus();
  for (let i = 0; i < 2; i++) await split.press("ArrowLeft");
  await page.waitForTimeout(300);

  // 2. Collections with folders: two collections, folders open and one folded, the collection page.
  await page.locator(".tree-row.folder", { hasText: "orders" }).first().click();
  await page.waitForTimeout(300);
  await page.locator(".tree-row.collection", { hasText: "shop-api" }).click();
  await page.waitForTimeout(300);
  if (process.env.EXTRA) await shot("extra-collection-top");
  await page.locator("main .page h2", { hasText: "requests" }).first().scrollIntoViewIfNeeded().catch(() => {});
  await page.evaluate(() => {
    const h = [...document.querySelectorAll(".page h2, .page h3")].find((e) => /\d+ requests/.test(e.textContent));
    h?.scrollIntoView({ block: "start" });
  });
  await shot("screenshot-2-collections");

  // 4. The run report: every request, checks, p95, the same run as a hitop command.
  await page.evaluate(() => document.querySelector(".page")?.scrollIntoView({ block: "start" }));
  await page.getByRole("button", { name: "Run collection" }).click();
  await page.waitForSelector(".run-cli", { timeout: 60000 });
  await page.waitForTimeout(500);
  await quiet();
  await shot("screenshot-4-run-report");

  // 5. GraphQL: the Catalog query with its variables, sent.
  await page.locator("lit-material-navigation-rail-item", { hasText: "Collections" }).click();
  await page.waitForTimeout(300);
  await openTab("Catalog query");
  await page.waitForTimeout(300);
  await send();
  await shot("screenshot-5-graphql");

  // Other candidates, not shipped: the Environments page and the command palette.
  if (process.env.EXTRA) {
    await page.locator("lit-material-navigation-rail-item", { hasText: "Environments" }).click();
    await shot("extra-environments");
    await page.keyboard.press("Control+k");
    await page.waitForTimeout(300);
    await page.keyboard.type("order");
    await shot("extra-palette");
  }

  await browser.close();
  await tiles();
  if (errors.length) console.log(`page errors:\n${errors.join("\n")}`);
  console.log(`Screenshots in ${out}`);
}

// The promo tiles from launch/store-assets/promo-tiles.html (the marquee shows screenshot 1),
// plus the 128 px store icon from brand/png. TILES=1 renders only these (no server needed).
async function tiles() {
  const src = path.join(__dirname, "../../launch/store-assets/promo-tiles.html");
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1500, height: 1000 }, deviceScaleFactor: 1 });
  await page.goto(`file://${src}`);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(300);
  for (const [id, name] of [["small", "promo-small-440x280"], ["marquee", "promo-marquee-1400x560"]]) {
    await page.locator(`#${id}`).screenshot({ path: path.join(out, `${name}.png`) });
    console.log(`${name}.png`);
  }
  await browser.close();
  fs.copyFileSync(path.join(__dirname, "../../brand/png/logo-128.png"), path.join(out, "icon-128.png"));
  console.log("icon-128.png");
}

(process.env.TILES ? tiles() : main()).catch((e) => {
  console.error(e);
  process.exit(1);
});
