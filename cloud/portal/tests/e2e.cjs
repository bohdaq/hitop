// The portal end to end in Chromium against the real sync server binary (HITOP_SYNC_SERVER_BIN;
// required when CI is set) with HITOP Cloud's plan limits on: sign in with the mailed code, the
// personal org, create an org, invite a member and an address without an account, usage against
// the Free limits and the 402 when seats run out, a role change, the plan page, an access token
// (made, used, revoked), a passkey (added, then signing in with it through a virtual
// authenticator), sign out. Serves dist/ (npm run build) with a few lines of Node.
//   HITOP_SYNC_SERVER_BIN=../../sync-server/target/debug/hitop-sync-server npm test
const { spawn } = require("child_process");
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

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".woff2": "font/woff2", ".svg": "image/svg+xml" };

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

async function syncServer(portalOrigin) {
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
  const code = async (email) => {
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
  return { url, code, api, signIn, stop, log: () => log };
}

(async () => {
  const dist = path.join(__dirname, "..", "dist");
  if (!fs.existsSync(path.join(dist, "index.html"))) throw new Error("build first (npm run build)");
  const portalPort = await freePort();
  // localhost, not 127.0.0.1: WebAuthn needs a domain name as the relying party id.
  const origin = `http://localhost:${portalPort}`;
  const portal = await serve(dist, portalPort);
  const server = await syncServer(origin);
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
    check("signed out", await app.getByRole("button", { name: "Send code" }).isVisible());
    await app.getByLabel("Sync server").fill(server.url);
    await app.getByLabel("Email").fill("ada@example.com");
    await app.getByRole("button", { name: "Sign in with a passkey" }).click();
    await idle();
    await page.getByRole("heading", { name: "Account" }).waitFor({ timeout: 10000 }).catch(() => {});
    check("signed in with the passkey", /Signed in as\s+ada@example\.com/.test(await app.innerText()), `${await alertText()} | ${(await app.innerText()).slice(0, 300)}`);

    const events = (await server.api("GET", `/v1/me/audit?limit=100`, bobSession)).json.events.map((e) => e.event);
    check("the server's audit log has the sign-ins", events.includes("signin"), events.join());
    check("no page errors", errors.length === 0, errors.join(" | "));
  } catch (e) {
    check("the run finished", false, `${e.stack}\n--- server log ---\n${server.log().slice(-3000)}`);
  } finally {
    await browser.close();
    portal.close();
    server.stop();
  }
  const failed = results.filter((r) => !r.ok);
  console.log(`${results.length - failed.length}/${results.length} passed`);
  process.exit(failed.length ? 1 : 0);
})();
