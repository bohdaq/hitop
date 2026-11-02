// hitop sync in the app (web and, with HITOP_MODE=tauri, the desktop build with a mocked invoke):
// two people in two browser contexts against a real sync server on localhost
// (HITOP_SYNC_SERVER_BIN, the sync-server binary; skipped without it unless CI is set). Sign in,
// share a collection, invite, trust and add, open it on the other side, edit both ways, a
// conflict resolved both ways (View both, Take theirs, Keep mine), delete, a runner credential,
// and removing the member; the owner's audit log; Sign in with SSO (against Keycloak when
// HITOP_SYNC_TEST_KEYCLOAK is set, see sync-server/tests/keycloak.rs). The server's database never
// holds a URL or a request name. A malicious server (this test editing its SQLite database) can't
// make the invitee use a workspace key of its own: an unauthenticated wrap, one forged in the
// owner's name and one from a device the server made up are all refused.
//   BASE=http://localhost:7879 HITOP_SYNC_SERVER_BIN=../sync-server/target/debug/hitop-sync-server node tests/sync.cjs
const { spawn } = require('child_process');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { launch, checker, TAURI, BASE } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

const freePort = () => new Promise((resolve) => {
  const s = net.createServer().listen(0, '127.0.0.1', () => {
    const { port } = s.address();
    s.close(() => resolve(port));
  });
});

async function syncServer(extraEnv = {}) {
  const bin = process.env.HITOP_SYNC_SERVER_BIN;
  if (!bin) {
    if (process.env.CI) throw new Error('HITOP_SYNC_SERVER_BIN must be set on CI');
    console.log('[sync] HITOP_SYNC_SERVER_BIN not set: skipped');
    process.exit(0);
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-sync-e2e-'));
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
      // The web app's origin (the browser calls the server directly).
      HITOP_SYNC_ALLOWED_ORIGINS: new URL(BASE).origin,
      ...extraEnv,
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
  const code = async (email, marker) => {
    for (let i = 0; i < 100; i++) {
      const parts = log.split(`mail to ${email}:`);
      if (parts.length > 1) {
        const line = parts[parts.length - 1].split('\n').find((l) => l.includes(marker));
        if (line) return line.trim().split(/[#\s]/).pop();
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error(`no mail to ${email}`);
  };
  const database = () => Buffer.concat(fs.readdirSync(dir).map((f) => fs.readFileSync(path.join(dir, f))));
  // The database itself, to play a malicious server (node:sqlite, Node 22.13+).
  const sql = () => new (require('node:sqlite').DatabaseSync)(path.join(dir, 'sync.db'));
  const stop = () => {
    child.kill();
    fs.rmSync(dir, { recursive: true, force: true });
  };
  return { url, code, database, sql, stop, log: () => log };
}

(async () => {
  const server = await syncServer();
  const ada = await launch({ results });
  const bob = await launch({ results });
  const errors = [];
  for (const h of [ada, bob]) h.page.on('pageerror', (e) => errors.push(e.message));
  const S = (h) => h.page.locator('hitop-sync');
  const note = (h) => S(h).locator('.sync-note').innerText().catch(() => '');
  const alertText = (h) => S(h).locator('lit-material-alert').innerText().catch(() => '');
  const idle = async (h) => {
    await h.page.waitForFunction(() => !document.querySelector('hitop-sync [aria-busy="true"]'), null, { timeout: 30000 });
    await settle(h.page);
  };
  const store = (h, fn, arg) => h.page.evaluate(([f, a]) => new Function('s', 'a', f)(document.querySelector('hitop-app').store, a), [fn, arg]);
  const requests = (h, name) => store(h, 'const c = s.collections.find((c) => c.name === a); return c ? c.requests.map((r) => ({ name: r.name, url: r.url })) : null;', name);
  const setUrl = (h, name, url) => store(h, 'const c = s.collections.find((c) => c.name === "Shop API"); const r = c.requests.find((r) => r.name === a[0]); s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: x.requests.map((y) => y.id === r.id ? { ...y, url: a[1] } : y) } : x); s.emit(); s.save();', [name, url]);
  const openSync = async (h) => {
    await h.page.locator('aside').getByRole('button', { name: 'Sync', exact: true }).click();
    await settle(h.page);
  };
  const syncNow = async (h) => {
    await S(h).getByRole('button', { name: /Sync now/ }).first().click();
    await idle(h);
  };
  const signIn = async (h, email) => {
    await openSync(h);
    await S(h).getByLabel('Sync server').fill(server.url);
    await S(h).getByLabel('Email').fill(email);
    await S(h).getByRole('button', { name: 'Send code' }).click();
    await idle(h);
    await S(h).getByLabel('Code from the mail').fill(await server.code(email, '/signin#'));
    await S(h).getByRole('button', { name: 'Sign in', exact: true }).click();
    await idle(h);
    await h.page.waitForFunction(() => document.querySelector('hitop-sync [aria-label="Your fingerprint"]')?.textContent.trim().length > 30, null, { timeout: 15000 }).catch(async (e) => {
      throw new Error(`sign-in as ${email} didn't finish: ${await alertText(h)} ${await note(h)} ${errors.join(' | ')}`);
    });
    return (await S(h).getByLabel('Your fingerprint').innerText()).trim();
  };

  for (const h of [ada, bob]) await h.page.goto(h.url, { waitUntil: 'networkidle' });

  // Ada has a collection with two requests.
  await store(ada, `s.addCollection("Shop API"); const c = s.collections.find((c) => c.name === "Shop API");
    s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: [
      { id: 101, name: "List orders", method: "GET", url: "https://internal.e2e-sync.test/orders?k=7f3a", headers: [], body: "" },
      { id: 102, name: "Get order", method: "GET", url: "https://internal.e2e-sync.test/orders/1", headers: [], body: "" } ] } : x);
    s.emit(); s.save();`);

  // Sign in on both sides.
  const adaFp = await signIn(ada, 'ada@example.com');
  const bobFp = await signIn(bob, 'bob@example.com');
  check('sign in by mailed code shows the device fingerprint', /^[0-9a-f]{4}( [0-9a-f]{4}){7}$/.test(adaFp) && adaFp !== bobFp, `${adaFp} / ${bobFp}`);
  if (TAURI) check('[tauri] the device key is in the keychain, not in localStorage', [...ada.mock.keychain.keys()].some((k) => k.startsWith('hitop-sync/')) && !(await ada.page.evaluate(() => Object.values(localStorage).join('').includes('"secret"'))), JSON.stringify([...ada.mock.keychain.keys()]));

  // Ada shares it.
  const shopId = await store(ada, 'return String(s.collections.find((c) => c.name === "Shop API").id);');
  await S(ada).locator('#sync-share').evaluate((el, v) => (el.value = v), shopId);
  await S(ada).getByRole('button', { name: 'Share', exact: true }).click();
  await idle(ada);
  check('share a collection', (await note(ada)).includes('Shared Shop API'), (await note(ada)) + (await alertText(ada)));

  // Invite Bob by mail; Bob accepts.
  await S(ada).getByRole('button', { name: 'Members' }).click();
  await idle(ada);
  await S(ada).getByLabel('Email (leave empty for a link)').fill('bob@example.com');
  await S(ada).getByRole('button', { name: 'Invite', exact: true }).click();
  await idle(ada);
  check('invite by mail', (await note(ada)).includes('mailed to bob@example.com'), await note(ada));
  // The mail carries the token only; the page shows the code with Ada's fingerprint to send herself.
  const mailed = await server.code('bob@example.com', 'hitop sync accept');
  const inviteCode = (await S(ada).getByLabel('Invite code with your fingerprint').innerText()).trim();
  check("the invite code carries the owner's fingerprint", inviteCode === `${mailed}.${adaFp.replace(/ /g, '')}`, `${inviteCode} vs ${mailed}`);
  await S(bob).getByLabel('Invite code').fill(inviteCode);
  await S(bob).getByRole('button', { name: 'Accept invite' }).click();
  await idle(bob);
  check('accept the invite', (await note(bob)).includes('Invite accepted (editor)') && (await note(bob)).includes(adaFp), (await note(bob)) + (await alertText(bob)));

  // Ada sees Bob's fingerprint, trusts it after comparing, and adds him.
  await S(ada).getByRole('button', { name: 'Members' }).click(); // close
  await idle(ada);
  await S(ada).getByRole('button', { name: 'Members' }).click(); // and open again: the accepted invite
  await idle(ada);
  const shownFp = (await S(ada).locator('.sync-invite .fingerprint').first().innerText()).trim();
  check("the accepted invite shows Bob's fingerprint", shownFp === bobFp, `${shownFp} vs ${bobFp}`);
  check('adding is off until the key is trusted', await S(ada).getByRole('button', { name: 'Add bob@example.com' }).evaluate((b) => b.disabled));
  await S(ada).locator('.sync-invite').getByRole('button', { name: 'Trust…' }).click();
  await settle(ada.page);
  await ada.page.getByRole('button', { name: 'They match, trust it' }).click();
  await idle(ada);
  await S(ada).getByRole('button', { name: 'Add bob@example.com' }).click();
  await idle(ada);
  check('add the member', (await note(ada)).includes('Added bob@example.com as editor'), (await note(ada)) + (await alertText(ada)));

  // A malicious server hands Bob a key of its own instead (the test edits its database); Bob's
  // app refuses each one and opens nothing.
  const { formatFromBytes } = await import('../src/wasm/format.js');
  const w = (await formatFromBytes(fs.readFileSync(path.join(__dirname, '../src/wasm/pkg/hitop_wasm_bg.wasm')))).sync;
  const b64 = (buf) => Buffer.from(buf).toString('base64url');
  const evilDb = server.sql();
  const keyOf = (email) => evilDb.prepare('SELECT k.id, k.public_key, k.user_id FROM device_keys k JOIN users u ON u.id = k.user_id WHERE u.email = ?').get(email);
  const adaKey = keyOf('ada@example.com');
  const bobKey = keyOf('bob@example.com');
  const wsId = evilDb.prepare('SELECT id FROM workspaces').get().id;
  const honest = evilDb.prepare('SELECT wrapped FROM wraps WHERE key_id = ? AND epoch = 1').get(bobKey.id).wrapped;
  const setWrap = (b) => evilDb.prepare('UPDATE wraps SET wrapped = ? WHERE key_id = ? AND epoch = 1').run(Buffer.from(b, 'base64url'), bobKey.id);
  const theirs = w.newKey();
  const evil = w.keygen();
  const refused = async (what, wrapped, needle) => {
    setWrap(wrapped);
    await S(bob).getByRole('button', { name: 'Open here' }).click();
    await idle(bob);
    const text = await alertText(bob);
    check(`a malicious server's key is refused: ${what}`, text.includes(needle) && (await requests(bob, 'Shop API')) === null, text);
  };
  await bob.page.reload({ waitUntil: 'networkidle' });
  await openSync(bob);
  await idle(bob);
  await refused('an unauthenticated wrap', w.wrapLegacy(b64(bobKey.public_key), wsId, 1, theirs), 'without authentication');
  await refused("a wrap forged in the owner's name", w.sealWrap(evil.secret, adaKey.id, bobKey.id, b64(bobKey.public_key), wsId, 1, 1, theirs), "Security warning: the workspace key (epoch 1) claims to come from device");
  evilDb.prepare("INSERT INTO device_keys (id, user_id, name, public_key, created_at) VALUES ('ffffffffffffffffffffffffffffffff', ?, 'Ada new laptop', ?, 0)").run(adaKey.user_id, Buffer.from(evil.publicKey, 'base64url'));
  const evilFp = w.fingerprint(evil.publicKey);
  await refused('a wrap from a device the server made up', w.sealWrap(evil.secret, 'ffffffffffffffffffffffffffffffff', bobKey.id, b64(bobKey.public_key), wsId, 1, 1, theirs), evilFp);
  check('a key from an unknown device offers to trust it only after comparing its fingerprint', (await S(bob).locator('.sync-untrusted').innerText()).includes(evilFp) && (await S(bob).getByRole('button', { name: 'Trust this device…' }).count()) === 1);
  evilDb.prepare("DELETE FROM device_keys WHERE id = 'ffffffffffffffffffffffffffffffff'").run();
  evilDb.prepare('UPDATE wraps SET wrapped = ? WHERE key_id = ? AND epoch = 1').run(honest, bobKey.id);
  evilDb.close();

  // Bob opens it: the key came from the device his invite code named.
  await S(bob).getByRole('button', { name: 'Open here' }).click();
  await idle(bob);
  check("the inviter's device is trusted from the invite code", (await S(bob).locator('.sync-warnings').innerText().catch(() => '')).includes(adaFp), await S(bob).locator('.sync-warnings').innerText().catch(() => ''));
  const bobReqs = await requests(bob, 'Shop API');
  check('the other side opens the shared collection', JSON.stringify(bobReqs) === JSON.stringify(await requests(ada, 'Shop API')), JSON.stringify(bobReqs) + (await alertText(bob)));

  // Bob edits, Ada pulls.
  await setUrl(bob, 'List orders', 'https://internal.e2e-sync.test/v2/orders');
  await syncNow(bob);
  check('push an edit', (await note(bob)).includes('1 pushed'), await note(bob));
  await syncNow(ada);
  check('pull the edit', (await requests(ada, 'Shop API')).find((r) => r.name === 'List orders').url === 'https://internal.e2e-sync.test/v2/orders', JSON.stringify(await requests(ada, 'Shop API')) + (await note(ada)));

  // Both edit the same request: a conflict on Bob's side.
  await setUrl(ada, 'Get order', 'https://ada.e2e-sync.test/');
  await setUrl(bob, 'Get order', 'https://bob.e2e-sync.test/');
  await syncNow(ada);
  await syncNow(bob);
  const conflicts = S(bob).locator('.sync-conflict');
  check('a request changed on both sides is a conflict', (await conflicts.count()) === 1 && (await note(bob)).includes('1 conflict'), await note(bob));
  await conflicts.getByRole('button', { name: 'View both' }).click();
  await settle(bob.page);
  const both = await S(bob).locator('.sync-both').innerText();
  check('View both shows mine, theirs and the base', both.includes('bob.e2e-sync.test') && both.includes('ada.e2e-sync.test') && both.includes('orders/1'), both);
  await conflicts.getByRole('button', { name: 'Take theirs' }).click();
  await idle(bob);
  check('Take theirs', (await requests(bob, 'Shop API')).find((r) => r.name === 'Get order').url === 'https://ada.e2e-sync.test/' && (await S(bob).locator('.sync-conflict').count()) === 0, JSON.stringify(await requests(bob, 'Shop API')));

  await setUrl(ada, 'Get order', 'https://ada2.e2e-sync.test/');
  await setUrl(bob, 'Get order', 'https://bob2.e2e-sync.test/');
  await syncNow(ada);
  await syncNow(bob);
  await S(bob).locator('.sync-conflict').getByRole('button', { name: 'Keep mine' }).click();
  await idle(bob);
  await syncNow(ada);
  check('Keep mine reaches the other side', (await requests(ada, 'Shop API')).find((r) => r.name === 'Get order').url === 'https://bob2.e2e-sync.test/', JSON.stringify(await requests(ada, 'Shop API')));

  // Bob deletes a request; Ada pulls the delete.
  await store(bob, 'const c = s.collections.find((c) => c.name === "Shop API"); s.deleteRequest(c.requests.find((r) => r.name === "List orders"), c.id); s.save();');
  await syncNow(bob);
  await syncNow(ada);
  check('a delete reaches the other side', JSON.stringify((await requests(ada, 'Shop API')).map((r) => r.name)) === '["Get order"]', JSON.stringify(await requests(ada, 'Shop API')));

  // Reload: the shared collection is still there and still shared.
  await ada.page.reload({ waitUntil: 'networkidle' });
  await openSync(ada);
  await idle(ada);
  check('a shared collection survives a reload', (await requests(ada, 'Shop API'))?.length === 1 && (await S(ada).getByRole('button', { name: /Sync now/ }).count()) === 1);

  // A runner credential for scheduled runs, and the trade-off is on the page.
  await S(ada).getByRole('button', { name: 'Members' }).click();
  await idle(ada);
  const runs = await S(ada).locator('.sync-team').innerText();
  check('the page explains runner vs server-side runs', runs.includes('Scheduled runs') && runs.includes('server still sees only ciphertext') && runs.includes('handing the server a key'), runs.slice(0, 200));
  await S(ada).getByRole('button', { name: 'Create runner credential' }).click();
  await idle(ada);
  const cred = (await S(ada).getByLabel('Runner credential', { exact: true }).innerText()).trim();
  check('a runner credential', cred.startsWith('hitop-runner-1.'), cred.slice(0, 20));
  // The CLI pulls with it (when a hitop binary is at hand: HITOP_BIN).
  if (process.env.HITOP_BIN) {
    const out = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-runner-'));
    const r = require('child_process').spawnSync(process.env.HITOP_BIN, ['sync', 'pull', '-c', out], { env: { ...process.env, HITOP_SYNC_RUNNER: cred, HITOP_KEYCHAIN: 'off', HOME: out, XDG_CONFIG_HOME: path.join(out, '.config') }, encoding: 'utf8' });
    const files = fs.existsSync(path.join(out, 'requests')) ? fs.readdirSync(path.join(out, 'requests')) : [];
    check('the CLI pulls the workspace with the runner credential', r.status === 0 && files.join() === 'get-order.yaml' && fs.existsSync(path.join(out, 'hitop.yaml')), `${r.status} ${r.stderr} ${files}`);
    fs.rmSync(out, { recursive: true, force: true });
  }

  // Ada removes Bob: new key, files re-encrypted; Bob can't sync any more.
  const bobRow = S(ada).locator('.sync-member', { hasText: 'bob@example.com' });
  await bobRow.getByRole('button', { name: 'Remove' }).click();
  await settle(ada.page);
  await ada.page.getByRole('button', { name: 'Remove', exact: true }).last().click();
  await idle(ada);
  check('remove a member rotates the key', /Removed bob@example.com: new key \(epoch 2\), \d+ file\(s\) encrypted again/.test(await note(ada)), (await note(ada)) + (await alertText(ada)));
  await syncNow(bob);
  check('a removed member can no longer sync', (await alertText(bob)).includes('no such workspace'), await alertText(bob));

  // The owner's audit log shows the team actions above, and its chain checks out.
  await S(ada).getByRole('button', { name: 'Show audit log' }).click();
  await idle(ada);
  const auditText = await S(ada).locator('.sync-audit').innerText().catch(() => '');
  check('the owner sees the audit log', ['workspace_created', 'invite_accepted', 'member_added', 'runner_created', 'member_removed', 'key_rotated'].every((e) => auditText.includes(e)) && auditText.includes('bob@example.com'), auditText.slice(0, 400) + (await alertText(ada)));
  check('the audit chain is intact', (await S(ada).getByLabel('Audit chain').innerText()).includes('Chain intact'), await S(ada).getByLabel('Audit chain').innerText().catch(() => ''));

  // Bob deletes his (now dead) copy: it stays gone after a reload.
  await store(bob, 'const c = s.collections.find((c) => c.name === "Shop API"); s.deleteCollection(c.id); s.save();');
  await bob.page.reload({ waitUntil: 'networkidle' });
  await openSync(bob);
  await settle(bob.page);
  check('a deleted shared collection stays deleted', (await requests(bob, 'Shop API')) === null && (await S(bob).getByRole('button', { name: /Sync now/ }).count()) === 0 && (await bob.page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('hitop_sync_files:')).length)) === 0, JSON.stringify(await requests(bob, 'Shop API')));

  // Sign in with SSO: this server has none.
  await S(bob).getByRole('button', { name: 'Sign out' }).click();
  await idle(bob);
  await S(bob).getByLabel('Sync server').fill(server.url);
  await S(bob).getByRole('button', { name: 'Sign in with SSO' }).click();
  await idle(bob);
  check('Sign in with SSO on a server without it says so', (await alertText(bob)).includes('no single sign-on'), await alertText(bob));
  const kc = process.env.HITOP_SYNC_TEST_KEYCLOAK;
  if (kc) {
    // A second server signing in with Keycloak; the sign-in page runs in a real browser tab.
    const secretFile = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-sso-')), 'secret');
    fs.writeFileSync(secretFile, 'hitop-sync-test-secret\n');
    const sso = await syncServer({ HITOP_SYNC_OIDC_ISSUER: `${kc.replace(/\/+$/, '')}/realms/hitop`, HITOP_SYNC_OIDC_CLIENT_ID: 'hitop-sync', HITOP_SYNC_OIDC_CLIENT_SECRET_FILE: secretFile });
    await S(bob).getByLabel('Sync server').fill(sso.url);
    const popup = TAURI ? null : bob.page.context().waitForEvent('page');
    await S(bob).getByRole('button', { name: 'Sign in with SSO' }).click();
    await idle(bob);
    const link = (await S(bob).getByLabel('SSO sign-in link').innerText()).trim();
    let tab;
    if (popup) tab = await popup;
    else {
      tab = await bob.page.context().newPage();
      await tab.goto(link);
    }
    await tab.waitForSelector('#username', { timeout: 30000 });
    await tab.fill('#username', 'alice');
    await tab.fill('#password', 'alice-test-pw');
    await tab.click('#kc-login');
    await tab.waitForSelector('code', { timeout: 30000 });
    const ssoCode = (await tab.locator('code').innerText()).trim();
    check('the provider sends the browser back to the server, which shows a one-time code', ssoCode.length > 30 && (await tab.content()).includes('alice@corp.test'), ssoCode.slice(0, 10));
    await tab.close();
    await S(bob).getByLabel('Code shown after signing in').fill(ssoCode);
    await S(bob).getByRole('button', { name: 'Sign in', exact: true }).click();
    await idle(bob);
    check('Sign in with SSO', (await S(bob).locator('.page-lead').first().innerText()).includes('alice@corp.test'), (await alertText(bob)) + (await S(bob).locator('.page-lead').first().innerText()));
    sso.stop();
  } else {
    console.log('[sync] HITOP_SYNC_TEST_KEYCLOAK not set: the SSO sign-in with Keycloak is skipped');
  }

  // HITOP Cloud C2 recovery phrase: Ada makes one, loses this browser, and on a new one signs in
  // and recovers with it: the same device (fingerprint), reading the workspace. A wrong or short
  // phrase recovers nothing.
  await openSync(ada);
  check('the page warns that the device key lives in this browser', TAURI || (await S(ada).getByLabel('Recovery phrase').innerText()).includes("browser's storage"));
  await S(ada).getByRole('button', { name: 'Create recovery phrase' }).click();
  await idle(ada);
  const phrase = (await S(ada).getByLabel('Your recovery phrase').innerText()).trim();
  check('create a recovery phrase (24 words, shown once)', phrase.split(' ').length === 24 && (await S(ada).getByLabel('Recovery status').innerText()).includes('Recovery phrase set'), phrase.slice(0, 20) + (await alertText(ada)));
  await S(ada).getByRole('button', { name: 'I wrote it down' }).click();
  check('the phrase is gone from the page', (await S(ada).getByLabel('Your recovery phrase').count()) === 0);
  const ada2 = await launch({ results });
  ada2.page.on('pageerror', (e) => errors.push(e.message));
  await ada2.page.goto(ada2.url, { waitUntil: 'networkidle' });
  const freshFp = await signIn(ada2, 'ada@example.com');
  check('a new device signs in with a new key', freshFp !== adaFp);
  const recover = async (words) => {
    if ((await S(ada2).getByLabel('Recovery phrase (24 words)').count()) === 0) await S(ada2).getByRole('button', { name: 'Recover a lost device' }).click();
    await S(ada2).getByLabel('Recovery phrase (24 words)').fill(words);
    await S(ada2).getByRole('button', { name: 'Recover', exact: true }).click();
    await idle(ada2);
  };
  const other = (await (await import('../src/wasm/format.js')).formatFromBytes(fs.readFileSync(path.join(__dirname, '../src/wasm/pkg/hitop_wasm_bg.wasm')))).sync.recoveryNew().words;
  await recover(other);
  check('a wrong recovery phrase recovers nothing', (await alertText(ada2)).includes("doesn't open") && (await S(ada2).getByLabel('Your fingerprint').innerText()).trim() === freshFp, await alertText(ada2));
  await recover(phrase.split(' ').slice(0, 23).join(' '));
  check('a short phrase is refused', (await alertText(ada2)).includes('24 words'), await alertText(ada2));
  await recover(phrase.toUpperCase());
  check('the right phrase brings the lost device back', (await note(ada2)).includes('Recovered the device') && (await S(ada2).getByLabel('Your fingerprint').innerText()).trim() === adaFp, (await note(ada2)) + (await alertText(ada2)));
  const adaKeys = server.sql().prepare("SELECT COUNT(*) AS n FROM device_keys k JOIN users u ON u.id = k.user_id WHERE u.email = 'ada@example.com'").get().n;
  check("the new sign-in's key is gone from the account", Number(adaKeys) === 1, String(adaKeys));
  await S(ada2).getByRole('button', { name: 'Open here' }).first().click();
  await idle(ada2);
  check('the recovered device reads the workspace', JSON.stringify(await requests(ada2, 'Shop API')) === JSON.stringify(await requests(ada, 'Shop API')), JSON.stringify(await requests(ada2, 'Shop API')) + (await alertText(ada2)));
  await ada2.browser.close();
  ada2.mock?.cleanup();

  // The server never saw plaintext.
  const db = server.database();
  const leaked = ['e2e-sync.test', 'List orders', 'Get order', 'Shop API'].filter((s) => db.includes(Buffer.from(s)));
  check('the server holds no plaintext', leaked.length === 0, leaked.join(', '));
  check('no page errors', errors.length === 0, errors.join(' | '));

  await bob.browser.close();
  bob.mock?.cleanup();
  server.stop();
  await ada.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
