// HITOP Cloud's personal library (src/library.js, sync-server library.rs) in the app (web and,
// with HITOP_MODE=tauri, the desktop build with a mocked invoke): one account in two browsers
// against a real sync server (HITOP_SYNC_SERVER_BIN; skipped without it unless CI is set).
// Collections already in the first browser are uploaded at the first sign-in; the second browser
// signs in and has the collection, the environment and its secret value (the server has a host
// key); edits and deletes travel both ways on focus; the same collection edited in both is a
// conflict: the server's version wins and this browser's is kept as "… (this browser)"; secret
// values are never in the server's database in the clear; signing out keeps everything here; a
// different account in the same browser doesn't get the first account's collections.
//   BASE=http://localhost:7879 HITOP_SYNC_SERVER_BIN=../sync-server/target/debug/hitop-sync-server node tests/library.cjs
const { spawn } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { launch, checker, TAURI, BASE } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);
const SECRET = 'lib-e2e-secret-9c4e1a';

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
    console.log('[library] HITOP_SYNC_SERVER_BIN not set: skipped');
    process.exit(0);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-library-e2e-'));
  const port = await freePort();
  const url = `http://127.0.0.1:${port}`;
  let log = '';
  const child = spawn(path.resolve(bin), [], {
    cwd: dir,
    env: {
      ...process.env,
      RWS_CONFIG_IP: '127.0.0.1',
      RWS_CONFIG_PORT: String(port),
      HITOP_SYNC_PUBLIC_URL: url,
      HITOP_SYNC_RP_ID: 'localhost',
      HITOP_SYNC_RP_ORIGINS: `http://localhost:${port}`,
      HITOP_SYNC_DATABASE: `sqlite:${path.join(dir, 'sync.db')}`,
      HITOP_SYNC_MAIL: 'log',
      HITOP_SYNC_ALLOWED_ORIGINS: new URL(BASE).origin,
      HITOP_SYNC_HOST_KEY: crypto.randomBytes(32).toString('hex'),
    },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
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
  const database = () => Buffer.concat(fs.readdirSync(dir).map((f) => fs.readFileSync(path.join(dir, f))));
  const stop = () => {
    child.kill();
    fs.rmSync(dir, { recursive: true, force: true });
  };
  return { url, code, mails, database, stop, log: () => log };
}

(async () => {
  const server = await syncServer();
  const one = await launch({ results });
  const two = await launch({ results });
  const errors = [];
  for (const h of [one, two]) h.page.on('pageerror', (e) => errors.push(e.message));
  const S = (h) => h.page.locator('hitop-sync').last();
  // The Account page: the top bar's Sign in, or the account menu's Account & devices.
  const openAccount = async (h) => {
    const signin = h.page.locator('header.topbar').getByRole('button', { name: 'Sign in', exact: true });
    if (await signin.count()) await signin.click();
    else {
      await h.page.locator('header.topbar .account-button').click();
      await h.page.locator('#account-menu').getByText('Account & devices').click();
    }
    await settle(h.page);
  };
  const alertText = (h) => S(h).locator('lit-material-alert').innerText().catch(() => '');
  const idle = async (h) => {
    await h.page.waitForFunction(() => !document.querySelector('hitop-sync [aria-busy="true"]'), null, { timeout: 30000 });
    await settle(h.page);
  };
  const store = (h, fn, arg) => h.page.evaluate(([f, a]) => new Function('s', 'a', f)(document.querySelector('hitop-app').store, a), [fn, arg]);
  const names = (h) => store(h, 'return s.collections.filter((c) => !s.folders.has(c.id)).map((c) => c.name).sort();');
  // Pushes what is waiting (what the debounce does a second later).
  const saved = (h) => settle(h.page, 300).then(() => store(h, 'return s.library.flush();'));
  // Another browser's changes: what the focus event does.
  const pull = (h) => store(h, 'return s.library.pull();').then(() => settle(h.page));
  const signIn = async (h, email) => {
    await openAccount(h);
    if (!(await S(h).locator('details.sync-more').evaluate((e) => e.open))) await S(h).locator('details.sync-more summary').click();
    await S(h).getByLabel('Sync server').fill(server.url);
    await S(h).getByLabel('Email').fill(email);
    await S(h).getByRole('button', { name: 'Email me a code instead' }).click();
    const before = server.mails(email);
    await S(h).getByRole('button', { name: 'Send code' }).click();
    await idle(h);
    await S(h).getByLabel('Code from the mail').fill(await server.code(email, before));
    await S(h).getByRole('button', { name: 'Sign in', exact: true }).click();
    await idle(h);
    await S(h).getByLabel('Cloud library').waitFor({ timeout: 15000 }).catch(async () => {
      throw new Error(`no library after signing in as ${email}: ${await alertText(h)} ${errors.join(' | ')}`);
    });
  };
  const signOut = async (h) => {
    await openAccount(h);
    await S(h).getByRole('button', { name: 'Sign out' }).click();
    await idle(h);
  };

  try {
    for (const h of [one, two]) await h.page.goto(h.url, { waitUntil: 'networkidle' });

    // Browser one has a collection and an environment with a secret before it ever signs in.
    await store(one, `s.addCollection("Shop API"); const c = s.collections.find((c) => c.name === "Shop API");
      s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: [{ id: 101, name: "List orders", method: "GET", url: "https://shop.e2e-library.test/orders", headers: [], body: "" }] } : x);
      s.environments = [...s.environments, { id: "env-prod", name: "prod", variables: { base: "https://shop.e2e-library.test", token: a }, secrets: ["token"] }];
      s.emit(); s.save();`, SECRET);
    await signIn(one, 'ada@example.com');
    await saved(one);
    check('the Account page says the library is on', (await S(one).getByLabel('Cloud library').innerText()).includes('Saved to HITOP Cloud'), await S(one).getByLabel('Cloud library').innerText());
    check('the secret value is not in the server database in the clear', !server.database().includes(Buffer.from(SECRET)));
    check('the collection is in the server database (the library is readable)', server.database().includes(Buffer.from('shop.e2e-library.test/orders')));

    // Browser two: the same account brings everything.
    await signIn(two, 'ada@example.com');
    await two.page.reload({ waitUntil: 'networkidle' });
    await settle(two.page, 800);
    check('the second browser has the collection after signing in', (await names(two)).includes('Shop API'), JSON.stringify(await names(two)));
    const lib = await (await fetch(`${server.url}/v1/library`, { headers: { Authorization: `Bearer ${await one.page.evaluate(() => document.querySelector('hitop-app').store.library.client.secret().then((x) => x.session))}` } })).json();
    check("each browser's empty Default collection stays out of the library", lib.items.filter((i) => i.kind === 'collection').map((i) => i.data.name).join() === 'Shop API', JSON.stringify(lib.items.map((i) => i.data?.name)));
    const env = await store(two, 'return s.environments.find((e) => e.id === "env-prod");');
    check('...the environment and its secret value', env?.variables?.token === SECRET && env.secrets.includes('token'), JSON.stringify(env));
    const url = await store(two, 'return s.collections.find((c) => c.name === "Shop API").requests[0].url;');
    check('...and the request', url === 'https://shop.e2e-library.test/orders', url);

    // An edit in browser two reaches browser one on focus.
    await store(two, `const c = s.collections.find((c) => c.name === "Shop API"); s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: [...x.requests, { id: 102, name: "Get order", method: "GET", url: "https://shop.e2e-library.test/orders/1", headers: [], body: "" }] } : x); s.emit();`);
    await saved(two);
    await pull(one);
    const reqs = await store(one, 'return s.collections.find((c) => c.name === "Shop API").requests.map((r) => r.name);');
    check('an edit in one browser shows in the other', JSON.stringify(reqs) === JSON.stringify(['List orders', 'Get order']), JSON.stringify(reqs));

    // A new collection and a delete travel too.
    await store(one, 's.addCollection("Billing"); s.emit();');
    await saved(one);
    await pull(two);
    check('a new collection shows in the other browser', (await names(two)).includes('Billing'), JSON.stringify(await names(two)));
    await store(two, 'const c = s.collections.find((c) => c.name === "Billing"); s.deleteCollection(c.id); s.emit();');
    await saved(two);
    await pull(one);
    check('a deleted collection goes from the other browser', !(await names(one)).includes('Billing'), JSON.stringify(await names(one)));

    // Both edit the same collection before either pulls: the server's wins, ours is kept as a copy.
    await store(one, 'const c = s.collections.find((c) => c.name === "Shop API"); s.renameCollection(c.id, "Shop API v1"); s.emit();');
    await saved(one);
    await store(two, 'const c = s.collections.find((c) => c.name === "Shop API"); s.renameCollection(c.id, "Shop API v2"); s.emit();');
    await saved(two);
    const twoNames = await names(two);
    check('a conflict keeps the server version and this browser\'s as a copy', twoNames.includes('Shop API v1') && twoNames.includes('Shop API v2 (this browser)') && !twoNames.includes('Shop API v2'), JSON.stringify(twoNames));
    await pull(one);
    check('...and the copy reaches the first browser too', (await names(one)).includes('Shop API v2 (this browser)'), JSON.stringify(await names(one)));

    // The switch in Settings, under Privacy: off, this browser's changes stay here.
    const T = (h) => h.page.locator('hitop-app-settings');
    await two.page.locator('aside').getByRole('button', { name: 'Settings', exact: true }).click();
    await settle(two.page);
    await T(two).getByRole('switch', { name: 'Save my collections to HITOP Cloud' }).click();
    await settle(two.page);
    await store(two, 's.addCollection("Only here"); s.emit();');
    await settle(two.page, 2000);
    await pull(one);
    check('switched off: a new collection stays in its browser', !(await names(one)).includes('Only here') && (await T(two).locator('.sync-library').innerText()).includes('stay in this browser only'), JSON.stringify(await names(one)));
    await T(two).getByRole('switch', { name: 'Save my collections to HITOP Cloud' }).click();
    await settle(two.page);
    await openAccount(two);
    check('switched on again: the library shows as on', (await S(two).getByLabel('Cloud library').count()) === 1);

    // Signing out keeps everything in this browser and stops saving to the cloud.
    await signOut(one);
    check('signed out: the collections stay in this browser', (await names(one)).includes('Shop API v2 (this browser)'), JSON.stringify(await names(one)));
    // Another account in this browser doesn't take the first account's collections along.
    await signIn(one, 'bob@example.com');
    await settle(one.page, 2500);
    // A sign-in of Bob's own, by code: wait for this mail (not the one the page asked for).
    const bobMails = server.mails('bob@example.com');
    const bob = await (await fetch(`${server.url}/v1/auth/email`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email: 'bob@example.com' }) })).status;
    const bobSession = (await (await fetch(`${server.url}/v1/auth/email/verify`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token: await server.code('bob@example.com', bobMails) }) })).json()).session;
    const bobLib = await (await fetch(`${server.url}/v1/library`, { headers: { Authorization: `Bearer ${bobSession}` } })).json();
    check("a second account's library doesn't get the first account's collections", bob === 202 && !bobLib.items.some((i) => i.kind === 'collection'), JSON.stringify(bobLib.items.map((i) => `${i.kind}/${i.data?.name ?? i.id}`)));

    check('no page errors', errors.length === 0, errors.join(' | '));
  } catch (e) {
    check('the run finished', false, `${e.stack}\n${server.log().slice(-1500)}`);
  } finally {
    server.stop();
    await two.browser.close().catch(() => {});
  }
  // Prints the results, closes browser one and exits 1 on a failure.
  await one.finish();
  console.log(`[library] ${results.filter((r) => r.startsWith('PASS')).length}/${results.length} passed`);
  process.exit(0);
})();
