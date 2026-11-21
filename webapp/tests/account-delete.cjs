// Account → Delete account (App Store 5.1.1(v), sync-server `DELETE /v1/me`) against a real sync
// server (HITOP_SYNC_SERVER_BIN; skipped without it unless CI is set), web and Tauri mode (and a
// store build with HITOP_STORE_BUILD=1): the confirmation says what goes and what stays; the only
// owner of an organization someone else is in is refused with the server's reason and keeps
// everything; once that's sorted, the account is deleted on the server, this browser forgets it
// (session and device key), shows it signed out, and its collections stay.
//   BASE=http://localhost:7879 HITOP_SYNC_SERVER_BIN=../sync-server/target/debug/hitop-sync-server node tests/account-delete.cjs
const { spawn } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { launch, checker, signInByCode, inApp, BASE } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

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
    console.log('[account-delete] HITOP_SYNC_SERVER_BIN not set: skipped');
    process.exit(0);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-account-delete-e2e-'));
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
  // Another person, signed in outside the app (a session token).
  const session = async (email) => {
    const before = mails(email);
    await fetch(`${url}/v1/auth/email`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email }) });
    const r = await fetch(`${url}/v1/auth/email/verify`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token: await code(email, before) }) });
    return (await r.json()).session;
  };
  const stop = () => {
    child.kill();
    fs.rmSync(dir, { recursive: true, force: true });
  };
  return { url, code, mails, session, stop, log: () => log };
}

(async () => {
  const server = await syncServer();
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  const email = 'ada@example.com';
  const before = server.mails(email);
  await signInByCode(p, server.url, email, () => server.code(email, before));
  await settle(p, 800);
  const collections = async () => (await inApp(p, 'return app.store.collections.map((c) => c.name);')).value ?? [];
  const had = await collections();
  // Ada owns an organization Bob is in: she has to hand it over (or remove him) first.
  const bob = await server.session('bob@example.com');
  const org = (await inApp(p, 'return (await c.api("POST", "/v1/orgs", { name: "Acme" })).id;')).value;
  const added = await inApp(p, 'return (await c.api("POST", `/v1/orgs/${a}/members`, { email: "bob@example.com", role: "member" })).status;', org);
  check('Bob is in Acme', added.value === 'added', JSON.stringify(added));

  // Account & devices → Delete account.
  await p.locator('header.topbar .account-button').click();
  await p.locator('#account-menu').getByText('Account & devices').click();
  await settle(p, 800);
  const del = p.locator('hitop-sync .account-delete').getByRole('button', { name: /Delete account/ });
  await del.waitFor({ timeout: 15000 });
  const hint = await p.locator('hitop-sync .account-delete .hint').innerText();
  check('the page says the collections here stay', /Collections in this (browser|app) stay/.test(hint), hint);
  const dialog = p.locator('lit-material-confirm-dialog[open]');
  // Cancel changes nothing.
  await del.click();
  await settle(p);
  await dialog.waitFor({ state: 'attached', timeout: 10000 });
  const shown = await dialog.evaluate((d) => ({ headline: d.headline, message: d.message }));
  const text = `${shown.headline} ${shown.message}`;
  check('the confirmation names the account, what goes and what stays', text.includes(email) && text.includes('synced library') && text.includes('devices') && text.includes('memberships') && /stay here/.test(text), text);
  check('no price or purchase words', !/\$|price|upgrade|buy|billing|subscription/i.test(text), text);
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await settle(p);
  check('cancel: still signed in', (await inApp(p, 'return (await c.api("GET", "/v1/me")).email;')).value === email);

  // Refused while she is Acme's only owner.
  await del.click();
  await settle(p);
  await dialog.getByRole('button', { name: 'Delete account' }).click();
  await settle(p, 1200);
  const alert = await p.locator('hitop-sync lit-material-alert').innerText().catch(() => '');
  check('refused: the server names Acme and says to hand over or remove the members', alert.includes('the organization Acme') && alert.includes('make another member an owner'), alert);
  check('refused: still signed in', (await inApp(p, 'return (await c.api("GET", "/v1/me")).email;')).value === email);

  // Bob leaves Acme; now the account goes.
  const bobId = (await (await fetch(`${server.url}/v1/me`, { headers: { Authorization: `Bearer ${bob}` } })).json()).id;
  const left = await fetch(`${server.url}/v1/orgs/${org}/members/${bobId}`, { method: 'DELETE', headers: { Authorization: `Bearer ${bob}` } });
  check('Bob left Acme', left.status === 204, String(left.status));
  const session = (await inApp(p, 'return (await c.secret()).session;')).value;
  await del.click();
  await settle(p);
  await dialog.getByRole('button', { name: 'Delete account' }).click();
  await settle(p, 1500);
  const page = await p.locator('hitop-sync').innerText();
  check('deleted: the page says so and is signed out', page.includes('was deleted') && page.includes('Signed out'), page);
  check('deleted: no account here', (await inApp(p, 'return c.account();')).value === null);
  check('deleted: no session or device key kept here', (await inApp(p, 'return await c.secret();')).value === null);
  check('deleted: nothing remembered to sign in as the same device', (await inApp(p, 'return localStorage.getItem("hitop_sync_device");')).value === null);
  const after = await fetch(`${server.url}/v1/me`, { headers: { Authorization: `Bearer ${session}` } });
  check('deleted on the server: the session is dead', after.status === 401, String(after.status));
  const members = await (await fetch(`${server.url}/v1/orgs/${org}`, { headers: { Authorization: `Bearer ${bob}` } })).status;
  check('Acme (Ada alone in it) is gone', members === 404, String(members));
  check('the collections stay here', JSON.stringify(await collections()) === JSON.stringify(had), JSON.stringify(await collections()));
  check('the deletion mail', server.log().includes(`mail to ${email}:`) && server.log().includes('was deleted'));
  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
  server.stop();
  if (results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
