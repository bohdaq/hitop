// DD3 (HITOP_NEXT_UI.md): Account → Devices against a real sync server (HITOP_SYNC_SERVER_BIN;
// skipped without it unless CI is set), web and Tauri mode: the list (this device marked and
// seen, since the app sends X-Hitop-Device once the server says it reads it), rename, remove
// another device, never this one; the top bar's device chip opens the list.
//   BASE=http://localhost:7879 HITOP_SYNC_SERVER_BIN=../sync-server/target/debug/hitop-sync-server node tests/devices.cjs
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
    console.log('[devices] HITOP_SYNC_SERVER_BIN not set: skipped');
    process.exit(0);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-devices-e2e-'));
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
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  const email = 'dev@example.com';
  const before = server.mails(email);
  await signInByCode(p, server.url, email, () => server.code(email, before));
  await settle(p);
  // The server learns which device this is from the next requests (after /v1/me said it reads the header).
  await inApp(p, 'await c.api("GET", "/v1/me"); await c.api("GET", "/v1/me");');
  // Another device of the same account (a second install), with no workspace keys.
  const k = 'btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32)))).replace(/\\+/g, "-").replace(/\\//g, "_").replace(/=+$/, "")';
  await inApp(p, `await c.call("POST", "/v1/me/keys", { name: "Old laptop", publicKey: ${k} });`);
  // Account → Devices from the account menu.
  await p.locator('header.topbar .account-button').click();
  await p.locator('#account-menu').getByText('Account & devices').click();
  await settle(p, 800);
  const section = p.locator('hitop-sync details.account-devices');
  await section.waitFor({ timeout: 15000 });
  await section.locator('summary').click();
  await settle(p);
  const rows = section.locator('.device-row');
  check('Devices lists both devices', (await rows.count()) === 2, String(await rows.count()));
  const mine = rows.filter({ has: p.locator('.status-chip', { hasText: 'This device' }) });
  check('this device is marked, and was seen (last used)', (await mine.count()) === 1 && (await mine.innerText()).includes('last used'), await mine.innerText().catch(() => ''));
  check('this device has no Remove', (await mine.getByRole('button', { name: /^Remove/ }).count()) === 0);
  const sentHeader = await inApp(p, 'const r = await c.api("GET", "/v1/me/devices"); return r.devices.find((d) => d.thisDevice)?.name ?? null;');
  check('the app says which device it is (X-Hitop-Device)', Boolean(sentHeader.value), JSON.stringify(sentHeader));
  // Rename the other one.
  const other = rows.filter({ hasText: 'Old laptop' });
  await other.getByRole('button', { name: 'Rename' }).click();
  await settle(p, 200);
  const input = section.locator('.device-name-input');
  await input.fill('Lost laptop');
  await input.press('Enter');
  await settle(p, 800);
  check('rename', (await section.innerText()).includes('Lost laptop'), await section.innerText());
  // Remove it (it holds no keys: no rotation).
  p.once('dialog', (d) => d.accept());
  await rows.filter({ hasText: 'Lost laptop' }).getByRole('button', { name: /^Remove/ }).click();
  await p.locator('lit-material-confirm-dialog').getByRole('button', { name: 'Remove' }).click().catch(() => {});
  await settle(p, 900);
  check('remove: the other device is gone', (await rows.count()) === 1 && !(await section.innerText()).includes('Lost laptop'), await section.innerText());
  const left = await inApp(p, 'return (await c.api("GET", "/v1/me")).keys.length;');
  check('...on the server too', left.value === 1, JSON.stringify(left));
  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
  server.stop();
  if (results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
