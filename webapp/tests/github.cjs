// HITOP Cloud Git sync in the web app (views/github-view.js, sync-server gitsync.rs): the real
// sync server (HITOP_SYNC_SERVER_BIN; skipped without it unless CI is set) with the test GitHub
// App, against a fake GitHub (tests/fake-github.cjs). Sign in, Connect GitHub (the App's install
// page sends the browser back connected), link a collection to a folder (its files land in the
// repository, nothing else there changes), an edit in the app is pushed, a colleague's commit
// (webhook, signed) comes back into the app, the same request changed on both sides shows both
// versions and Keep both keeps both, here and on GitHub, and Unlink leaves the repository alone.
// Web mode only: the desktop build has its own Git panel for folders.
//   BASE=http://localhost:7879 HITOP_SYNC_SERVER_BIN=../sync-server/target/debug/hitop-sync-server node tests/github.cjs
const { spawn } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { launch, checker, TAURI, BASE } = require('./harness.cjs');
const { fakeGithub } = require('./fake-github.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);
// Screenshots of the GitHub section (GITHUB_SHOTS=<dir>), for a look at the layout.
const shot = (page, name) => process.env.GITHUB_SHOTS && page.screenshot({ path: path.join(process.env.GITHUB_SHOTS, `${name}.png`) }).catch(() => {});

if (TAURI) {
  console.log('[github] web mode only: skipped in Tauri mode');
  process.exit(0);
}

const freePort = () => new Promise((resolve) => {
  const s = net.createServer().listen(0, '127.0.0.1', () => {
    const { port } = s.address();
    s.close(() => resolve(port));
  });
});

const REPO = 'ada/apis';
const WEBHOOK = 'whsec-e2e';

(async () => {
  const bin = process.env.HITOP_SYNC_SERVER_BIN;
  if (!bin) {
    if (process.env.CI) throw new Error('HITOP_SYNC_SERVER_BIN must be set on CI');
    console.log('[github] HITOP_SYNC_SERVER_BIN not set: skipped');
    process.exit(0);
  }
  const port = await freePort();
  const syncUrl = `http://127.0.0.1:${port}`;
  const gh = fakeGithub({ appId: '4242', installation: 7, callback: () => `${syncUrl}/v1/git/github/callback` });
  const ghPort = await freePort();
  await new Promise((r) => gh.server.listen(ghPort, '127.0.0.1', r));
  const ghUrl = `http://127.0.0.1:${ghPort}`;
  gh.repo(REPO, { 'README.md': '# APIs\n' });

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-github-e2e-'));
  let log = '';
  const child = spawn(path.resolve(bin), [], {
    cwd: dir,
    env: {
      ...process.env,
      RWS_CONFIG_IP: '127.0.0.1',
      RWS_CONFIG_PORT: String(port),
      HITOP_SYNC_PUBLIC_URL: syncUrl,
      HITOP_SYNC_RP_ID: 'localhost',
      HITOP_SYNC_RP_ORIGINS: `http://localhost:${port}`,
      HITOP_SYNC_DATABASE: `sqlite:${path.join(dir, 'sync.db')}`,
      HITOP_SYNC_MAIL: 'log',
      HITOP_SYNC_ALLOWED_ORIGINS: new URL(BASE).origin,
      HITOP_SYNC_HOST_KEY: crypto.randomBytes(32).toString('hex'),
      HITOP_SYNC_GITHUB_APP_ID: '4242',
      HITOP_SYNC_GITHUB_APP_SLUG: 'hitop-cloud-test',
      HITOP_SYNC_GITHUB_APP_KEY_FILE: path.join(__dirname, '../../sync-server/tests/fixtures/github-app/test-app.pem'),
      HITOP_SYNC_GITHUB_APP_CLIENT_ID: 'Iv1.app',
      HITOP_SYNC_GITHUB_APP_CLIENT_SECRET: 'app-secret',
      HITOP_SYNC_GITHUB_APP_WEBHOOK_SECRET: WEBHOOK,
      HITOP_SYNC_GITHUB_URL: ghUrl,
      HITOP_SYNC_GITHUB_API_URL: ghUrl,
      HITOP_SYNC_GIT_DEBOUNCE: '0',
    },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  child.stderr.on('data', (d) => (log += d));
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${syncUrl}/healthz`)).ok) break;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  const mails = (email) => log.split(`mail to ${email}:`).length - 1;
  const code = async (email, after) => {
    for (let i = 0; i < 100 && mails(email) <= after; i++) await new Promise((r) => setTimeout(r, 100));
    const parts = log.split(`mail to ${email}:`);
    return parts[parts.length - 1].split('\n').find((l) => l.includes('/signin#')).trim().split('#').pop();
  };
  const waitFor = async (what, fn, ms = 15000) => {
    for (const end = Date.now() + ms; Date.now() < end; await new Promise((r) => setTimeout(r, 250))) if (await fn()) return true;
    return false;
  };
  const webhook = async () => {
    const raw = JSON.stringify({ ref: 'refs/heads/main', after: gh.refs.get(REPO).get('main'), repository: { full_name: REPO }, installation: { id: 7 } });
    const sig = crypto.createHmac('sha256', WEBHOOK).update(raw).digest('hex');
    return (await fetch(`${syncUrl}/v1/git/github/webhook`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-GitHub-Event': 'push', 'X-Hub-Signature-256': `sha256=${sig}` }, body: raw })).status;
  };

  const h = await launch({ results });
  const errors = [];
  h.page.on('pageerror', (e) => errors.push(e.message));
  const page = h.page;
  const S = () => page.locator('hitop-sync');
  const G = () => page.locator('hitop-github');
  const store = (fn, arg) => page.evaluate(([f, a]) => new Function('s', 'a', f)(document.querySelector('hitop-app').store, a), [fn, arg]);
  const openSync = async () => {
    const signin = page.locator('header.topbar').getByRole('button', { name: 'Sign in', exact: true });
    if (await signin.count()) await signin.click();
    else {
      await page.locator('header.topbar .account-button').click();
      await page.locator('#account-menu').getByText('Account & devices').click();
    }
    await settle(page);
  };
  // A collection's Share dialog, on the GitHub tab.
  const shareGithub = async () => {
    await page.locator('hitop-collection').getByRole('button', { name: 'Share', exact: true }).click();
    await page.locator('#dialog').getByRole('tab', { name: /GitHub/ }).click();
    await settle(page);
  };
  const shopFiles = () => Object.entries(gh.files(REPO)).filter(([p]) => p.startsWith('shop/'));
  try {
    await page.goto(h.url, { waitUntil: 'networkidle' });
    await store(`s.addCollection("Shop"); const c = s.collections.find((c) => c.name === "Shop");
      s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: [
        { id: 501, name: "List orders", method: "GET", url: "https://shop.e2e-git.test/orders", headers: [], body: "" },
        { id: 502, name: "Get order", method: "GET", url: "https://shop.e2e-git.test/orders/1", headers: [], body: "" } ] } : x);
      s.emit(); s.save();`);
    await openSync();
    await S().locator('details.sync-more summary').click();
    await S().getByLabel('Sync server').fill(syncUrl);
    await S().getByLabel('Email').fill('ada@example.com');
    await S().getByRole('button', { name: 'Email me a code instead' }).click();
    const before = mails('ada@example.com');
    await S().getByRole('button', { name: 'Send code' }).click();
    await settle(page);
    await S().getByLabel('Code from the mail').fill(await code('ada@example.com', before));
    await S().getByRole('button', { name: 'Sign in', exact: true }).click();
    await G().getByRole('button', { name: 'Connect GitHub' }).waitFor({ timeout: 15000 });
    await store('return s.library.flush();');
    await shot(page, 'connect');
    check('signed in: the GitHub section offers Connect', true);

    // The collection page says where Shop is kept and offers Save to GitHub.
    const openShop = async () => {
      await page.locator('aside').getByRole('button', { name: /^Shop/ }).click();
      await settle(page);
    };
    await openShop();
    const chip = () => page.locator('hitop-collection .storage-chip');
    check('the collection page says Shop is on HITOP Cloud', (await chip().innerText()).includes('HITOP Cloud'), await chip().innerText());
    check('...and so does its sidebar icon', ((await page.locator('aside .tree-row.collection', { hasText: 'Shop' }).locator('.storage-icon').getAttribute('title')) || '').includes('HITOP Cloud'));
    const D = () => page.locator('hitop-git-panel');
    await shareGithub();
    await D().getByRole('button', { name: 'Connect GitHub' }).waitFor({ timeout: 10000 });
    await shot(page, 'dialog-connect');
    check('the dialog starts with Connect GitHub', true);

    // Connect: the fake install page sends the browser back; Shop's dialog opens again.
    await D().getByRole('button', { name: 'Connect GitHub' }).click();
    await page.waitForURL((u) => u.href.startsWith(h.url) && gh.visits.length > 0, { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    await D().getByText('Connected as').waitFor({ timeout: 15000 }).catch(() => {});
    await shot(page, 'back');
    check('back from GitHub: the dialog is open again, connected', (await D().count()) === 1 && /Connected as\s+ada/.test(await D().innerText()), (await D().count()) ? await D().innerText() : 'no dialog');

    // More repositories: the App's install page again (where GitHub lets you pick an account or
    // configure the repositories), not the authorization screen that bounces straight back.
    const seen = gh.visits.length;
    await D().getByRole('button', { name: /Give HITOP access/ }).click();
    await page.waitForURL((u) => u.href.startsWith(h.url) && gh.visits.length > seen, { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    check('Give HITOP access opens the install page', gh.visits.slice(seen).join() === 'install', gh.visits.join());
    await D().getByLabel('Folder').waitFor({ timeout: 15000 });

    // Save Shop to the folder "shop", after the preview of the files.
    await D().getByLabel('Folder').fill('shop');
    await settle(page, 200);
    const preview = await D().getByLabel('Files HITOP will write').innerText();
    check('the dialog previews the files it will write', preview.includes(REPO) && preview.includes('shop/hitop.yaml') && preview.includes('shop/requests/list-orders.yaml'), preview);
    await shot(page, 'dialog-form');
    await D().getByRole('button', { name: 'Save to GitHub' }).click();
    await D().getByLabel('GitHub link of Shop').waitFor({ timeout: 15000 });
    check('linked: the collection is in the repository', shopFiles().some(([p]) => p === 'shop/hitop.yaml') && shopFiles().filter(([p]) => p.startsWith('shop/requests/')).length === 2, JSON.stringify(Object.keys(gh.files(REPO))));
    check('...and the rest of the repository is untouched', gh.files(REPO)['README.md'] === '# APIs\n');
    await shot(page, 'linked');
    check('...the dialog says so', (await D().getByLabel('GitHub state of Shop').innerText()).includes('In sync with GitHub'), await D().getByLabel('GitHub state of Shop').innerText());
    await page.locator('#dialog').getByRole('button', { name: 'Close' }).click();
    await settle(page);
    check('...and so does the chip', (await chip().innerText()).includes(`${REPO}/shop · main`), await chip().innerText());

    // An edit in the app reaches GitHub.
    await store(`const c = s.collections.find((c) => c.name === "Shop"); s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: x.requests.map((r) => r.id === 501 ? { ...r, url: "https://shop.e2e-git.test/v2/orders" } : r) } : x); s.emit();`);
    await store('return s.library.flush();');
    check('an edit in the app is pushed to GitHub', await waitFor('push', () => shopFiles().some(([, t]) => t.includes('/v2/orders'))), JSON.stringify(shopFiles()));
    check('...as a hitop commit', gh.head(REPO).message.startsWith('hitop: update'), gh.head(REPO).message);

    // A colleague's commit comes back into the app.
    const [getPath, getText] = shopFiles().find(([, t]) => t.includes('/orders/1'));
    gh.push(REPO, { [getPath]: getText.replace('/orders/1', '/orders/{id}') });
    check('the signed webhook is accepted', (await webhook()) === 200);
    const pulled = await waitFor('pull', async () => {
      await store('return s.library.pull();');
      return store('return s.collections.find((c) => c.name === "Shop").requests.find((r) => r.id === 502)?.url;').then((u) => u === 'https://shop.e2e-git.test/orders/{id}');
    });
    check("a colleague's commit shows in the app", pulled, JSON.stringify(await store('return s.collections.find((c) => c.name === "Shop").requests.map((r) => r.url);')));

    // Both change "List orders": both versions are shown; Keep both keeps both.
    const [listPath, listText] = shopFiles().find(([, t]) => t.includes('/v2/orders'));
    gh.push(REPO, { [listPath]: listText.replace('/v2/orders', '/v3/orders') });
    await store(`const c = s.collections.find((c) => c.name === "Shop"); s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: x.requests.map((r) => r.id === 501 ? { ...r, url: "https://shop.e2e-git.test/v4/orders" } : r) } : x); s.emit();`);
    await store('return s.library.flush();');
    await webhook();
    await waitFor('conflict', async () => {
      await store('return s.git.loadLinks();');
      return (await page.locator('aside .storage-dot').count()) === 1;
    });
    check('a clash puts a dot on the sidebar icon', (await page.locator('aside .storage-dot').count()) === 1);
    await shot(page, 'clash');
    await chip().click();
    await D().getByLabel('GitHub conflicts in Shop').waitFor({ timeout: 10000 }).catch(() => {});
    const conflict = await D().getByLabel('GitHub conflicts in Shop').innerText().catch(() => '');
    await shot(page, 'conflict');
    check('a request changed on both sides shows both versions', conflict.includes('List orders') && conflict.includes('/v4/orders') && conflict.includes('/v3/orders'), conflict);
    await D().getByRole('group', { name: 'Keep for List orders' }).getByRole('button', { name: 'Keep both' }).click();
    await D().getByRole('button', { name: 'Apply and sync' }).click();
    await D().getByText('in sync with GitHub again').waitFor({ timeout: 15000 }).catch(() => {});
    const names = await store('return s.collections.find((c) => c.name === "Shop").requests.map((r) => r.name);');
    check('Keep both: both in the app', names.includes('List orders') && names.includes('List orders (mine)'), JSON.stringify(names));
    check('...and both on GitHub', shopFiles().some(([, t]) => t.includes('/v3/orders')) && shopFiles().some(([, t]) => t.includes('/v4/orders') && t.includes('(mine)')), JSON.stringify(shopFiles().map(([p]) => p)));

    // Unlink: saves stop going to GitHub, the files stay.
    const kept = JSON.stringify(gh.files(REPO));
    await D().getByRole('button', { name: 'Stop saving to GitHub' }).click();
    await page.locator('lit-material-confirm-dialog').getByRole('button', { name: 'Stop saving to GitHub' }).click();
    await D().getByLabel('GitHub link of Shop').waitFor({ state: 'detached', timeout: 10000 }).catch(() => {});
    check('unlinked: the dialog offers Save to GitHub again', (await D().getByLabel('Folder').count()) === 1);
    await page.locator('#dialog').getByRole('button', { name: 'Close' }).click();
    await settle(page);
    check('...and the chip says HITOP Cloud', (await chip().innerText()).includes('HITOP Cloud'), await chip().innerText());
    await store('s.addCollection("After"); s.emit();');
    await store('return s.library.flush();');
    await settle(page, 2500);
    check('unlinked: the repository keeps its files and gets nothing new', JSON.stringify(gh.files(REPO)) === kept);
    // Default was here before the first sign-in, so it is kept in this browser only; the dialog
    // saves it to HITOP Cloud, then offers GitHub.
    await store(`const c = s.collections.find((c) => c.name === "Default"); s.collections = s.collections.map((x) => x.id === c.id ? { ...x, requests: [
        { id: 601, name: "Same", method: "GET", url: "", headers: [], body: "" }, { id: 602, name: "Same", method: "GET", url: "", headers: [], body: "" } ] } : x); s.emit();`);
    await page.locator('aside .tree-row.collection', { hasText: 'Default' }).click();
    await settle(page);
    check('Default is kept in this browser only', (await chip().innerText()).includes('This browser only'), await chip().innerText());
    await shareGithub();
    await D().getByRole('button', { name: 'Save to HITOP Cloud' }).click();
    await D().getByLabel('Folder').waitFor({ timeout: 15000 }).catch(() => {});
    check('Save to HITOP Cloud: then the GitHub form', (await D().getByLabel('Folder').count()) === 1, await D().innerText());
    const files = await D().getByLabel('Files HITOP will write').innerText();
    check('the preview numbers requests with the same name', files.includes('default/requests/same.yaml') && files.includes('default/requests/same-2.yaml'), files);

    // Create a new repository: GitHub's own page, filled in; back in this tab the new one is chosen.
    const [popup] = await Promise.all([page.waitForEvent('popup'), D().getByRole('button', { name: /Create a new repository/ }).click()]);
    const nu = new URL(popup.url());
    check('Create a new repository opens GitHub\'s new repository page, filled in', nu.pathname === '/new' && nu.searchParams.get('name') === 'hitop-collections' && nu.searchParams.get('owner') === 'ada' && nu.searchParams.get('visibility') === 'private', popup.url());
    await popup.close();
    gh.repo('ada/hitop-collections', { 'README.md': '# new\n' });
    await page.evaluate(() => dispatchEvent(new Event('focus')));
    await D().getByText('ada/hitop-collections is chosen').waitFor({ timeout: 10000 }).catch(() => {});
    check('...and back here the new repository is chosen', (await D().getByLabel('Files HITOP will write').innerText()).includes('ada/hitop-collections'), await D().innerText());
    const up = await store(`return s.library.client.api("GET", "/v1/library").then((r) => r.items.some((i) => i.kind === "collection" && i.data?.name === "Default"));`);
    check('...and Default is in the library', up === true);
    await page.locator('#dialog').getByRole('button', { name: 'Close' }).click();
    await settle(page);
    check('...and the chip says HITOP Cloud', (await chip().innerText()).includes('HITOP Cloud'), await chip().innerText());
    check('no page errors', errors.length === 0, errors.join(' | '));
  } catch (e) {
    check('the run finished', false, `${e.stack}\n${log.slice(-1500)}`);
  } finally {
    child.kill();
    gh.server.close();
    fs.rmSync(dir, { recursive: true, force: true });
  }
  await h.finish();
  process.exit(0);
})();
