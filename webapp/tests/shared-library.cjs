// A shared collection is end-to-end encrypted; the personal library is readable by the server. With
// both on (as people have them) nothing of a shared collection may reach the library: while the app
// starts, when the app's own list of shared collections is damaged or gone, when its files are.
// Both modes; a real sync server (HITOP_SYNC_SERVER_BIN).
const { spawn } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { launch, checker, inApp, signInByCode, BASE } = require('./harness.cjs');
const { results, check } = checker();
const MARK = 'shared-e2e-mark.test';

const freePort = () => new Promise((resolve) => {
  const s = net.createServer().listen(0, '127.0.0.1', () => {
    const { port } = s.address();
    s.close(() => resolve(port));
  });
});

(async () => {
  const bin = process.env.HITOP_SYNC_SERVER_BIN;
  if (!bin) {
    if (process.env.CI) throw new Error('HITOP_SYNC_SERVER_BIN must be set on CI');
    console.log('[shared-library] HITOP_SYNC_SERVER_BIN not set: skipped');
    return;
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-shared-library-'));
  const port = await freePort();
  const url = `http://127.0.0.1:${port}`;
  let log = '';
  const child = spawn(path.resolve(bin), [], {
    cwd: dir,
    env: {
      ...process.env, RWS_CONFIG_IP: '127.0.0.1', RWS_CONFIG_PORT: String(port), HITOP_SYNC_PUBLIC_URL: url, HITOP_SYNC_RP_ID: 'localhost',
      HITOP_SYNC_RP_ORIGINS: `http://localhost:${port}`, HITOP_SYNC_DATABASE: `sqlite:${path.join(dir, 'sync.db')}`, HITOP_SYNC_MAIL: 'log',
      HITOP_SYNC_ALLOWED_ORIGINS: new URL(BASE).origin, HITOP_SYNC_HOST_KEY: crypto.randomBytes(32).toString('hex'),
    },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  child.stderr.on('data', (d) => (log += d));
  for (let i = 0; i < 100; i++) {
    if (await fetch(`${url}/healthz`).then((r) => r.ok, () => false)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  const code = async () => {
    for (let i = 0; i < 100 && !log.includes('/signin#'); i++) await new Promise((r) => setTimeout(r, 100));
    return log.split('\n').filter((l) => l.includes('/signin#')).pop().trim().split('#').pop();
  };
  // Everything the server keeps, in the clear or not: plaintext of a shared collection must not be there.
  const leaked = () => Buffer.concat(fs.readdirSync(dir).map((f) => fs.readFileSync(path.join(dir, f)))).includes(Buffer.from(MARK));

  const h = await launch({ results });
  const errors = [];
  h.page.on('pageerror', (e) => errors.push(e.message));
  const store = (fn, arg) => h.page.evaluate(([f, a]) => new Function('s', 'a', f)(document.querySelector('hitop-app').store, a), [fn, arg]);
  // Every library write, as the browser sent it (web; the desktop's go through Rust: the database check).
  const puts = [];
  h.page.on('request', (r) => r.method() === 'PUT' && r.url().includes('/v1/library/collection/') && puts.push(r.postData() || ''));
  const quiet = async () => {
    await h.page.waitForFunction(() => document.querySelector('hitop-app')?.store?.library?.state, null, { timeout: 20000 });
    await store('return s.ready;');
    await store('return s.library.flush();');
  };
  const reload = async () => {
    await h.page.goto('about:blank');
    await h.page.goto(h.url, { waitUntil: 'networkidle' });
    await quiet();
  };
  const clean = (name) => check(name, !leaked() && !puts.some((p) => p.includes(MARK)), `${puts.filter((p) => p.includes(MARK)).length} writes`);
  try {
    await h.page.goto(h.url, { waitUntil: 'networkidle' });
    await signInByCode(h.page, url, 'ada@example.com', code);
    await quiet();
    // Shared before its content is written, so the mark was never in the library.
    await store('s.addCollection("Shared API");');
    const id = await store('return s.collections.find((c) => c.name === "Shared API").id;');
    await quiet();
    const shared = await inApp(h.page, 'await c.share(a);', id);
    if (!shared.ok) throw new Error(shared.error);
    await quiet();
    await store('s.collections = s.collections.map((x) => x.id === a[0] ? { ...x, requests: [{ id: 901, name: "Secret request", method: "GET", url: "https://" + a[1] + "/x", headers: [], body: "" }] } : x); s.emit(); s.save();', [id, MARK]);
    await quiet();
    check('it is shared', await store('return s.folders.has(a);', id));
    clean('nothing of it in the library while it is open');
    for (let i = 0; i < 3; i++) await reload();
    check('still shared after a reload', await store('return s.folders.has(a);', id));
    clean('nor while the app starts');

    const files = () => h.page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('hitop_sync_files:')));
    const save = await h.page.evaluate(() => ({ ...localStorage }));
    const damage = async (key, value) => {
      await h.page.evaluate(([k, v]) => (v === null ? localStorage.removeItem(k) : localStorage.setItem(k, v)), [key, value]);
      await reload();
    };
    for (const [what, key, value] of [['its files are damaged', (await files())[0], '{'], ['its files are gone', (await files())[0], null], ['the list is damaged', 'hitop_sync', '{'], ['the list is gone', 'hitop_sync', null]]) {
      await damage(key, value);
      await reload();
      clean(`nor when ${what}`);
      await h.page.evaluate((all) => { localStorage.clear(); for (const [k, v] of Object.entries(all)) localStorage.setItem(k, v); }, save);
    }
    check('no page errors', errors.length === 0, errors.join(' | '));
  } catch (e) {
    check('the suite ran', false, `${e.stack} ${errors.join(' | ')}`);
  }
  child.kill();
  await h.finish();
  fs.rmSync(dir, { recursive: true, force: true });
})();
