// Plan limits in the app against a real sync server with HITOP_SYNC_LIMITS=plans
// (HITOP_SYNC_SERVER_BIN; skipped without it unless CI is set), web and Tauri modes:
//   U49 usage against limits with a warning before the limit: the title bar's usage chip from
//       the server's usage (2 of 3 shared collections = one left), the Share dialog's room line
//       before sharing, nothing for an org with room;
//   U50 a clear message when a limit stops an action, naming who can fix it: a member's share
//       refused with 402 `limit` names the org's owner and admin from its member list (inline in
//       the open Share dialog); the limit dialog names them too; the owner is offered Plan &
//       billing instead.
// Ada owns the team org Acme (Free: 3 shared collections) and invites Bob as a member; the test
// plays the operator and adds Acme's shared collections straight into the server's database.
//   BASE=http://localhost:7879 HITOP_SYNC_SERVER_BIN=../sync-server/target/debug/hitop-sync-server node tests/limits.cjs
const { spawn } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { launch, checker, BASE } = require('./harness.cjs');
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
    console.log('[limits] HITOP_SYNC_SERVER_BIN not set: skipped');
    process.exit(0);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-limits-e2e-'));
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
      HITOP_SYNC_LIMITS: 'plans',
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
  const sql = () => new (require('node:sqlite').DatabaseSync)(path.join(dir, 'sync.db'));
  const stop = () => {
    child.kill();
    fs.rmSync(dir, { recursive: true, force: true });
  };
  // A session by mailed code, without a browser (Ada works through the API).
  const session = async (email) => {
    const before = mails(email);
    await fetch(`${url}/v1/auth/email`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email }) });
    const r = await fetch(`${url}/v1/auth/email/verify`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token: await code(email, before) }) });
    const token = (await r.json()).session;
    return async (method, p, body) => {
      const res = await fetch(`${url}${p}`, { method, headers: { Authorization: `Bearer ${token}`, ...(body ? { 'Content-Type': 'application/json' } : {}) }, body: body ? JSON.stringify(body) : undefined });
      return { status: res.status, json: await res.json().catch(() => null) };
    };
  };
  return { url, code, mails, sql, session, stop };
}

(async () => {
  const server = await syncServer();
  const bob = await launch({ results });
  const p = bob.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const S = () => p.locator('hitop-sync').last();
  const idle = async () => {
    await p.waitForFunction(() => !document.querySelector('hitop-sync [aria-busy="true"]'), null, { timeout: 30000 });
    await settle(p);
  };
  const store = (fn, arg) => p.evaluate(([f, a]) => new Function('s', 'a', f)(document.querySelector('hitop-app').store, a), [fn, arg]);
  const addWorkspaces = (org, n) => {
    const db = server.sql();
    const now = Math.floor(Date.now() / 1000);
    for (let i = 0; i < n; i++) db.prepare('INSERT INTO workspaces (id, epoch, seq, bytes, created_at, org_id) VALUES (?, 1, 0, 0, ?, ?)').run(crypto.randomUUID(), now, org);
    db.close();
  };
  try {
    // Ada: the team org Acme on Free, Bob invited as a member, two of three shared collections used.
    const ada = await server.session('ada@example.com');
    const acme = (await ada('POST', '/v1/orgs', { name: 'Acme' })).json.id;
    const invited = await ada('POST', `/v1/orgs/${acme}/members`, { email: 'bob@example.com', role: 'member' });
    addWorkspaces(acme, 2);

    await p.goto(bob.url, { waitUntil: 'networkidle' });
    await p.locator('header.topbar').getByRole('button', { name: 'Sign in', exact: true }).click();
    await settle(p);
    if (!(await S().locator('details.sync-more').evaluate((e) => e.open))) await S().locator('details.sync-more summary').click();
    await S().getByLabel('Sync server').fill(server.url);
    await S().getByLabel('Email').fill('bob@example.com');
    await S().getByRole('button', { name: 'Email me a code instead' }).click();
    const before = server.mails('bob@example.com');
    await S().getByRole('button', { name: 'Send code' }).click();
    await idle();
    await S().getByLabel('Code from the mail').fill(await server.code('bob@example.com', before));
    await S().getByRole('button', { name: 'Sign in', exact: true }).click();
    await idle();
    await p.waitForFunction((id) => document.querySelector('hitop-app').store.orgs.some((o) => o.id === id), acme, { timeout: 15000 });
    check('Bob is a member of Acme after signing in', invited.status < 300 && (await store('return s.orgs.find((o) => o.id === a)?.role;', acme)) === 'member');

    // U49: an org with room shows nothing; Acme at 2 of 3 warns in the title bar.
    await store('s.setCurrentOrg(s.personalOrg);');
    await settle(p);
    const chip = p.locator('header.topbar .usage-chip');
    check('U49 an org with room shows no usage warning', (await chip.count()) === 0);
    await store('s.setCurrentOrg(a);', acme);
    await p.waitForFunction(() => document.querySelector('header.topbar .usage-chip'), null, { timeout: 10000 }).catch(() => {});
    const chipLabel = await chip.getAttribute('aria-label').catch(() => '');
    // Ada and Bob are 2 of Free's 3 seats as well.
    check('U49 the title bar warns before the limit: 2 of 3 shared collections (and seats)', (await chip.count()) === 1 && chipLabel.includes('Shared collections: 2 of 3, nearly full') && chipLabel.includes('Seats: 2 of 3, nearly full') && (await chip.evaluate((el) => el.classList.contains('warn'))), chipLabel);
    await chip.click();
    await p.locator('hitop-org section[aria-label="Plan and billing"]').waitFor({ timeout: 10000 }).catch(() => {});
    const meterNote = await p.locator('hitop-org .org-meter', { hasText: 'Shared collections' }).innerText().catch(() => '');
    check('U49 ...and opens Plan & billing, where the meter says one is left', meterNote.includes('2 of 3') && meterNote.includes('Nearly full: 1 left'), meterNote);

    // The Share dialog says what sharing this one takes, before it is shared.
    await store('s.addCollection("Acme API"); const c = s.collections.find((c) => c.name === "Acme API"); s.show("collection", c.id);');
    await settle(p, 800);
    await p.evaluate(() => {
      const app = document.querySelector('hitop-app');
      app.openShare(app.store.collections.find((c) => c.name === 'Acme API'), 'people');
    });
    const room = p.locator('#dialog .share-room');
    await room.waitFor({ timeout: 10000 }).catch(() => {});
    check('U49 the Share dialog says sharing this one takes the last one', (await room.innerText().catch(() => '')).includes('2 of 3 shared collections of Acme (Free) are used: sharing this one takes the last one'), await room.innerText().catch(() => ''));

    // U50: full now (the operator added one more); Bob's share is refused and the dialog says who can fix it.
    addWorkspaces(acme, 1);
    await p.evaluate(() => document.querySelector('hitop-app').refreshUsage());
    await settle(p);
    check('U49 ...and that it will be refused once the plan is full', (await room.innerText().catch(() => '')).includes('All 3 shared collections of Acme (Free) are used'), await room.innerText().catch(() => ''));
    check('U49 the title bar says full', (await chip.evaluate((el) => el.classList.contains('full')).catch(() => false)) && (await chip.getAttribute('aria-label')).includes('3 of 3, full'));
    await p.locator('#dialog').getByRole('button', { name: /Share with people/ }).click();
    const fixNote = p.locator('#dialog hitop-limit-note');
    await fixNote.locator('.limit-fixers').waitFor({ timeout: 15000 }).catch(() => {});
    await p.waitForFunction(() => /ada@example\.com/.test(document.querySelector('#dialog hitop-limit-note')?.textContent ?? ''), null, { timeout: 10000 }).catch(() => {});
    const fixText = await fixNote.innerText().catch(() => '');
    check('U50 a refused share says who can fix it: the owner, from the member list', fixText.includes('Only owners and admins change the plan') && fixText.includes('ada@example.com (owner)') && fixText.includes('Acme'), fixText);
    check('U50 ...next to the server\'s message, no Plan & billing for a member', (await p.locator('#dialog lit-material-alert').innerText().catch(() => '')).includes('plan limit') && (await fixNote.getByRole('button', { name: 'Plan & billing' }).count()) === 0);
    const count = (() => {
      const db = server.sql();
      const n = db.prepare('SELECT COUNT(*) AS n FROM workspaces WHERE org_id = ?').get(acme).n;
      db.close();
      return n;
    })();
    check('U50 ...and nothing was shared', count === 3, String(count));
    await p.locator('#dialog .dialog-actions').getByRole('button', { name: 'Close' }).click().catch(() => {});
    await settle(p);

    // The limit dialog (any other refused create) names them too; it is opened for the refused org.
    await p.evaluate((org) => document.querySelector('hitop-app').openLimit({ limit: 'workspaces', plan: 'free', used: 3, max: 3, org, message: 'plan limit: the free plan of the organization Acme allows 3 shared workspaces; nothing was changed.', headline: 'The Free plan has used its 3 shared collections' }, null), acme);
    await p.waitForFunction(() => /ada@example\.com/.test(document.querySelector('#dialog hitop-limit-note')?.textContent ?? ''), null, { timeout: 10000 }).catch(() => {});
    const dlg = await p.locator('#dialog').innerText();
    check('U50 the limit dialog names who can upgrade', dlg.includes('The Free plan has used its 3 shared collections') && dlg.includes('ada@example.com (owner)') && (await p.locator('#dialog').getByRole('button', { name: 'See who can upgrade' }).count()) === 1, dlg.replace(/\n/g, ' | ').slice(0, 500));
    await p.locator('#dialog').getByRole('button', { name: 'Close' }).first().click();
    await settle(p);
    // Bob's own personal plan: he can change it himself.
    await p.evaluate((org) => document.querySelector('hitop-app').openLimit({ limit: 'workspaces', plan: 'free', used: 3, max: 3, org, message: 'plan limit', headline: 'The Free plan has used its 3 shared collections' }, null), await store('return s.personalOrg;'));
    await p.locator('#dialog hitop-limit-note .limit-fixers').waitFor({ timeout: 5000 }).catch(() => {});
    const own = await p.locator('#dialog').innerText();
    check('U50 a personal plan: you can change it, Plan & billing offered', own.includes("It's your personal account's plan") && (await p.locator('#dialog').getByRole('button', { name: 'Plan & billing' }).count()) === 1, own.replace(/\n/g, ' | ').slice(0, 400));
    await p.locator('#dialog').getByRole('button', { name: 'Close' }).first().click();

    check('no page errors', errors.length === 0, errors.join(' | '));
  } catch (e) {
    check(`no exception: ${e.message}`, false, e.stack?.split('\n').slice(0, 3).join(' '));
  } finally {
    server.stop();
    await bob.finish();
  }
})();
