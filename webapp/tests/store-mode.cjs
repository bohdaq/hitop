// ST1 free store apps (HITOP_NEXT_UI.md ST1): the desktop's store builds (Mac App Store, Microsoft
// Store; Rust's `build_info` says `store: true`, the mock's `storeBuild`) show no purchase call to
// action (App Store 3.1.1/3.1.3), the GitHub Release builds keep today's screens. Tauri mode only
// (the web app is never a store build); a real sync server with HITOP_SYNC_LIMITS=plans
// (HITOP_SYNC_SERVER_BIN; skipped without it unless CI is set).
// Per mode: signed out, the palette; signed in on Free (the personal org) and as the owner of Acme,
// a Team plan that has ended (set-plan team, then status expired); the Organization page's plan
// section and General, the New organization dialog, the account menu, the Account page, the limit
// dialog from a real 402 (a fourth device on Free) and the sign-in dialog's device-limit step from a
// real refused sign-in, and the palette signed in.
//   HITOP_MODE=tauri HITOP_SYNC_SERVER_BIN=../sync-server/target/debug/hitop-sync-server node tests/store-mode.cjs
const { spawn, spawnSync } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { launch, checker, signInByCode, inApp, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

// What a store build may never show: the task's list, plus any address and any word sending people
// to buy elsewhere. The plan's own name and "Free" don't match.
const SALES = /pay|price|upgrade|checkout|pricing|plans|trial|buy|\$|€|billing/i;
const ELSEWHERE = /https?:\/\/|on the web|in a browser|website/i;

const freePort = () => new Promise((resolve) => {
  const s = net.createServer().listen(0, '127.0.0.1', () => {
    const { port } = s.address();
    s.close(() => resolve(port));
  });
});

async function syncServer() {
  const bin = process.env.HITOP_SYNC_SERVER_BIN;
  if (!bin) {
    if (process.env.CI) throw new Error('HITOP_SYNC_SERVER_BIN must be set on CI');
    console.log('[store-mode] HITOP_SYNC_SERVER_BIN not set: skipped');
    process.exit(0);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-store-e2e-'));
  const port = await freePort();
  const url = `http://127.0.0.1:${port}`;
  const env = {
    ...process.env,
    RWS_CONFIG_IP: '127.0.0.1',
    RWS_CONFIG_PORT: String(port),
    HITOP_SYNC_PUBLIC_URL: url,
    HITOP_SYNC_RP_ID: 'localhost',
    HITOP_SYNC_RP_ORIGINS: `http://localhost:${port}`,
    HITOP_SYNC_DATABASE: `sqlite:${path.join(dir, 'sync.db')}`,
    HITOP_SYNC_MAIL: 'log',
    HITOP_SYNC_ALLOWED_ORIGINS: 'http://tauri.localhost',
    HITOP_SYNC_HOST_KEY: crypto.randomBytes(32).toString('hex'),
    HITOP_SYNC_LIMITS: 'plans',
  };
  let log = '';
  const child = spawn(path.resolve(bin), [], { cwd: dir, env, stdio: ['ignore', 'ignore', 'pipe'] });
  child.stderr.on('data', (d) => (log += d));
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${url}/healthz`)).ok) break;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  const mails = (email) => log.split(`mail to ${email}:`).length - 1;
  const code = async (email, after) => {
    for (let i = 0; i < 100 && mails(email) <= after; i++) await new Promise((r) => setTimeout(r, 100));
    const parts = log.split(`mail to ${email}:`);
    const line = parts[parts.length - 1].split('\n').find((l) => l.includes('/signin#'));
    if (!line) throw new Error(`no mail to ${email}`);
    return line.trim().split('#').pop();
  };
  // The operator: a Team plan for the org, then ended (expired), as billing::expire leaves it.
  const lapsedTeam = (org) => {
    const r = spawnSync(path.resolve(bin), ['set-plan', org, 'team', '5'], { env, encoding: 'utf8' });
    if (r.status !== 0) throw new Error(`set-plan: ${r.stderr}`);
    const db = new (require('node:sqlite').DatabaseSync)(path.join(dir, 'sync.db'));
    db.exec('PRAGMA busy_timeout = 10000'); // the server writes the same file
    db.prepare("UPDATE entitlements SET status = 'expired' WHERE org_id = ?").run(org);
    db.close();
  };
  const stop = () => {
    child.kill();
    fs.rmSync(dir, { recursive: true, force: true });
  };
  return { url, code, mails, lapsedTeam, stop };
}

// Every text a view shows (shadow roots too, and the attributes that become text: label,
// headline, title, aria-label, placeholder) and every href in it.
function deep(selectors) {
  const text = [];
  const hrefs = [];
  const visit = (node) => {
    if (node.nodeType === 3) return text.push(node.textContent);
    if (node.nodeType === 1) {
      if (['STYLE', 'SCRIPT', 'TEMPLATE'].includes(node.tagName)) return;
      for (const a of ['label', 'headline', 'title', 'aria-label', 'placeholder', 'error-text']) if (node.getAttribute(a)) text.push(node.getAttribute(a));
      if (node.getAttribute('href')) hrefs.push(node.getAttribute('href'));
      if (node.shadowRoot) visit(node.shadowRoot);
    } else if (node.nodeType !== 11) return;
    for (const c of node.childNodes) visit(c);
  };
  for (const s of selectors) document.querySelectorAll(s).forEach(visit);
  return { text: text.join(' ').replace(/\s+/g, ' ').trim(), hrefs };
}

async function runMode(server, store) {
  const tag = store ? '[store]' : '[github]';
  const h = await launch({ results });
  h.mock.storeBuild = store;
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const view = async (...selectors) => p.evaluate(deep, selectors);
  const clean = (name, v, { allowHrefs = [] } = {}) => {
    const sales = v.text.match(SALES)?.[0];
    const away = v.text.match(ELSEWHERE)?.[0];
    const hrefs = v.hrefs.filter((x) => !allowHrefs.some((a) => x.endsWith(a)));
    check(`${tag} ${name}: no price, purchase word, link or "elsewhere"`, !sales && !away && hrefs.length === 0, `${sales ?? ''} ${away ?? ''} ${hrefs.join(' ')} | ${v.text.slice(0, 400)}`);
  };
  const app = (body, arg) => inApp(p, body, arg);
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control';
  const palette = p.locator('lit-material-command-palette');
  const paletteItems = async () => {
    await p.locator('main').first().click({ position: { x: 5, y: 5 } }).catch(() => {});
    await p.keyboard.press(`${mod}+k`);
    await palette.getByRole('combobox').waitFor({ timeout: 5000 });
    // Actions with their search words; requests and environments (the person's own) by label only.
    const items = await palette.evaluate((el) => el.items.map((i) => ({ label: String(i.label ?? ''), keywords: i.section === 'Actions' ? (i.keywords ?? []).join(' ') : '' })));
    await p.keyboard.press('Escape');
    await settle(p, 200);
    return items;
  };
  const closeDialog = async () => {
    await app('app.closeDialog();');
    await settle(p, 300);
  };

  await p.goto(h.url, { waitUntil: 'networkidle' });
  check(`${tag} platform().storeBuild follows build_info`, (await p.evaluate(() => document.querySelector('hitop-app').storeBuild())) === store);
  await p.evaluate((u) => localStorage.setItem('hitop_settings', JSON.stringify({ ...(JSON.parse(localStorage.getItem('hitop_settings') || '{}')), syncServer: u })), server.url);

  // Signed out: the palette's way to organizations.
  const outItems = await paletteItems();
  const orgItem = outItems.find((i) => /Organization/.test(i.label));
  if (store) check(`${tag} palette signed out: nothing about billing or buying`, outItems.every((i) => !SALES.test(i.label) && !SALES.test(i.keywords)), JSON.stringify(outItems.filter((i) => SALES.test(i.label + i.keywords))));
  else check(`${tag} palette signed out: the organization item still finds "billing"`, Boolean(orgItem?.keywords.includes('billing')), JSON.stringify(orgItem));

  // Signed in on Free (the personal org), as the owner of Acme, a Team plan that has ended.
  const email = `${store ? 'store' : 'github'}@example.com`;
  const before = server.mails(email);
  await signInByCode(p, server.url, email, () => server.code(email, before));
  await settle(p);
  const acme = (await app('return (await c.api("POST", "/v1/orgs", { name: "Acme" })).id;')).value;
  server.lapsedTeam(acme);
  await app('await app.reloadOrgs();');
  await p.waitForFunction((id) => document.querySelector('hitop-app').store.orgs.some((o) => o.id === id), acme, { timeout: 15000 });
  const personal = (await app('return app.store.orgs.find((o) => o.personal)?.id;')).value;

  const planSection = async (org) => {
    await app('app.store.setCurrentOrg(a); await app.openOrg("billing");', org);
    await p.waitForSelector('hitop-org section[aria-label^="Plan and"] .org-meters', { timeout: 15000 });
    await settle(p, 500);
    return view('hitop-org', 'header.topbar');
  };
  const free = await planSection(personal);
  check(`${tag} Free: the plan's name is shown`, /Free/.test(free.text), free.text.slice(0, 200));
  const lapsed = await planSection(acme);
  check(`${tag} lapsed Team: the plan's name and usage meters are shown`, /Team/.test(lapsed.text) && (await p.locator('hitop-org [role="meter"]').count()) > 0, lapsed.text.slice(0, 300));
  if (store) {
    clean('Organization → plan, Free', free);
    clean('Organization → plan, lapsed Team', lapsed);
    check(`${tag} lapsed Team reads "This organization's plan has ended"`, lapsed.text.includes("This organization's plan has ended"), lapsed.text.slice(0, 400));
    check(`${tag} the section is "Plan & usage"`, lapsed.text.includes('Plan & usage') && !lapsed.text.includes('Plan & billing'));
    check(`${tag} no button in the plan section`, (await p.locator('hitop-org section[aria-label^="Plan and"] lit-material-button').count()) === 0);
  } else {
    check(`${tag} lapsed Team keeps "until it's paid again" and where plans are changed`, lapsed.text.includes("until it's paid again") && lapsed.text.includes('Plans and payment are changed on the web'), lapsed.text.slice(0, 400));
    check(`${tag} the section is "Plan & billing"`, lapsed.text.includes('Plan & billing'));
  }
  // Every other section of the lapsed org, as its owner (the test server's own address taken out).
  const sections = { general: 'General', members: 'Members', tokens: 'Access tokens', runs: 'Runs', security: 'Security' };
  for (const [section, name] of Object.entries(sections)) {
    await app('await app.openOrg(a);', section);
    await settle(p, section === 'security' ? 1500 : 600);
    const v = await view('hitop-org');
    if (store) clean(`Organization → ${name}`, { ...v, text: v.text.split(server.url).join('') });
  }

  // New organization: no trial in a store build.
  await app('app.openDialog("new-org");');
  await settle(p);
  const newOrg = await view('#dialog');
  if (store) clean('New organization dialog', newOrg);
  else check(`${tag} New organization still offers the Team trial`, newOrg.text.includes('Team trial'), newOrg.text.slice(0, 200));
  await closeDialog();

  // The account menu and the Account page.
  await p.locator('header.topbar .account-button').click();
  await settle(p, 300);
  const menu = await view('#account-menu');
  if (store) {
    clean('account menu', menu);
    check(`${tag} account menu: "Plan & usage"`, menu.text.includes('Plan & usage'), menu.text);
  } else check(`${tag} account menu: "Plan & billing"`, menu.text.includes('Plan & billing'), menu.text);
  await p.keyboard.press('Escape');
  await settle(p, 200);
  await app('app.go("sync");');
  await p.waitForSelector('hitop-sync .account-rows', { timeout: 15000 });
  await settle(p, 500);
  const account = await view('hitop-sync');
  // The test server's own address is shown (not HITOP Cloud): that one is not a plan link.
  if (store) clean('Account page', { ...account, text: account.text.split(server.url).join('') });

  // The limit dialog from a real 402: a fourth device on Free (3 devices), refused on the Organization page.
  await app('app.store.setCurrentOrg(a); await app.openOrg("billing");', personal);
  await p.waitForSelector('hitop-org section[aria-label^="Plan and"]', { timeout: 15000 });
  const key = 'btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32)))).replace(/\\+/g, "-").replace(/\\//g, "_").replace(/=+$/, "")';
  await app(`for (let i = 0; i < 2; i++) await c.call("POST", "/v1/me/keys", { name: "Old laptop " + i, publicKey: ${key} }, { allow: [402] });`);
  await app(`await document.querySelector("hitop-org").act(() => c.api("POST", "/v1/me/keys", { name: "One too many", publicKey: ${key} }));`);
  await p.waitForSelector('#dialog[open] hitop-limit-note .limit-fixers', { timeout: 15000 });
  await settle(p, 500);
  const limit = await view('#dialog');
  check(`${tag} the limit dialog opened for the device limit`, /devices/.test(limit.text), limit.text.slice(0, 300));
  if (store) {
    clean('limit dialog', limit);
    check(`${tag} limit dialog: no server plan link, a neutral "Plan & usage"`, !limit.text.includes('See or change the plan') && limit.text.includes('Plan & usage'), limit.text.slice(0, 400));
  } else check(`${tag} limit dialog keeps the server's text and "Plan & billing"`, limit.text.includes('See or change the plan') && limit.text.includes('Plan & billing'), limit.text.slice(0, 400));
  await closeDialog();

  // The sign-in dialog's device-limit step from a real refused sign-in (this device forgotten).
  const before2 = server.mails(email);
  await app('await app.signOut(); localStorage.removeItem("hitop_sync_device"); await c.saveSecret(a, null);', server.url);
  await app('await c.startSignIn(a[0], a[1]);', [server.url, email]);
  const refused = await app('app.openSignIn(); await app.updateComplete; await new Promise((r) => setTimeout(r, 300)); try { await c.finishSignIn(a); return "signed in"; } catch (e) { document.querySelector("#dialog hitop-sign-in").atLimit(e); return e.keys ? "device limit" : String(e.message); }', await server.code(email, before2));
  await p.waitForSelector('#dialog hitop-sign-in .signin[data-step="devices"]', { timeout: 15000 }).catch(() => {});
  await settle(p, 300);
  const step = await view('#dialog');
  check(`${tag} the device-limit step lists the devices`, refused.value === 'device limit' && (await p.locator('#dialog .signin-devices li').count()) === 3, `${JSON.stringify(refused)} ${step.text.slice(0, 300)}`);
  if (store) {
    clean('device-limit sign-in step', step);
    check(`${tag} device-limit step: no "see plans"`, !/see plans/i.test(step.text));
  } else check(`${tag} device-limit step keeps "see plans"`, /see plans/.test(step.text) && step.hrefs.some((x) => x.includes('/plan')), step.hrefs.join(' '));
  await closeDialog();

  // The palette (signed in on this server's account; the dialog left at its device step).
  const inItems = await paletteItems();
  if (store) check(`${tag} palette: no item that opens billing or buys`, inItems.every((i) => !SALES.test(i.label) && !SALES.test(i.keywords)), JSON.stringify(inItems.filter((i) => SALES.test(i.label + i.keywords))));
  check(`${tag} no page errors`, errors.length === 0, errors.join(' | '));
  return h;
}

(async () => {
  if (!TAURI) {
    console.log('[store-mode] desktop builds only (HITOP_MODE=tauri): skipped in web mode');
    process.exit(0);
  }
  const server = await syncServer();
  try {
    const github = await runMode(server, false);
    await github.browser.close();
    github.mock.cleanup();
    const store = await runMode(server, true);
    await store.finish();
  } finally {
    server.stop();
  }
  if (results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})().catch((e) => {
  console.error(e);
  console.log(results.join('\n'));
  process.exit(1);
});
