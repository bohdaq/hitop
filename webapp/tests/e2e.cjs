// End-to-end check of the web app against a running hitop-server (the proxy
// must allow the origin). Seeds the storage with data an older version wrote:
// the React web app's localStorage (tests/legacy-storage.json), or in Tauri
// mode the React desktop's and CLI's files (tests/desktop-storage/), so it
// also proves old data still loads and is kept in its shape.
//   BASE=http://localhost:7879 npm run test:e2e  [-- <screenshot dir>]
//   HITOP_MODE=tauri node tests/e2e.cjs
// Needs network: requests go to example.com and jsonplaceholder.typicode.com.
const fs = require('fs');
const path = require('path');
const { launch, checker } = require('./harness.cjs');
const out = process.argv[2] || require('os').tmpdir();
const fixture = JSON.parse(fs.readFileSync(path.join(__dirname, 'legacy-storage.json'), 'utf8'));
const { results, check } = checker();
(async () => {
  const h = await launch({ seed: fixture, tauriSeed: path.join(__dirname, 'desktop-storage'), results });
  const p = h.page;
  // The Billing API collection's id in each fixture.
  const billingId = h.tauri ? '2' : 'col-legacy';
  const errors = []; p.on('pageerror', e => errors.push(e.message)); p.on('console', m => m.type() === 'error' && errors.push(m.text()));
  const proxied = h.sent;
  h.mock?.keychain.set('global/Prod/token', 'tok-1');
  await p.goto(h.url, { waitUntil: 'networkidle' });

  // 1. Old data renders
  const side = await p.locator('aside.sidebar').innerText();
  check('old collections load', side.includes('Default') && side.includes('Billing API') && side.includes('Create post'), side.replace(/\n/g, ' | ').slice(0, 160));

  // 2. Send through the proxy
  await p.getByLabel('URL').fill('https://example.com/');
  await p.getByRole('button', { name: 'Send' }).click();
  await p.locator('.status-chip.ok').first().waitFor({ timeout: 20000 });
  check('send via proxy → 200', proxied.length === 1 && proxied[0].url === 'https://example.com/', `proxied=${proxied.length}`);
  check('response shown', (await p.locator('.response-meta').innerText()).includes('200'));
  await p.screenshot({ path: `${out}/new-request.png` });

  // 3. Open a saved request with variables and run its collection (scripts + context passing)
  await p.locator('aside').getByRole('button', { name: /Billing API/ }).click();
  await p.getByRole('button', { name: /Run collection/ }).click();
  await p.locator('.run h2', { hasText: 'Last run' }).waitFor({ timeout: 30000 });
  const run = await p.locator('.run-list').innerText();
  check('collection run: 2 requests ok', (run.match(/^20\d$/gm) || []).length === 2, run.replace(/\n/g, ' | '));
  const sent2 = proxied.find(x => x.url.endsWith('/posts/1'));
  const prev = sent2?.headers.find(h => h[0] === 'X-Prev');
  check('variables interpolated + context passed between requests', proxied.some(x => x.url === 'https://jsonplaceholder.typicode.com/posts' && x.headers.some(h => h[0] === 'Authorization' && h[1] === 'Bearer abc')) && prev && prev[1] === '101', `X-Prev=${prev?.[1]}`);
  await p.screenshot({ path: `${out}/new-collection.png` });

  // 4. Save the current tab into Billing API with a new name
  await p.locator('aside').getByRole('button', { name: /^POST Create post/ }).click();
  await p.getByRole('button', { name: 'Save request' }).click();
  const name = p.locator('#dialog #name');
  await name.waitFor();
  await name.evaluate((el) => { el.value = 'Create post v2'; });
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await p.waitForTimeout(400);
  check('save renames request', (await p.locator('aside').innerText()).includes('Create post v2'));

  // 5. History (a page since 1.2)
  await p.locator('aside').getByRole('button', { name: 'History' }).click();
  await p.waitForTimeout(400);
  const hist = await p.locator('main .history-page').innerText().catch(() => ''); // grouped by day: one list per day
  check('history lists old + new entries', hist.includes('todos/1') && hist.includes('example.com'), `${hist.split('\n').length} lines`);
  await p.screenshot({ path: `${out}/new-history.png` });
  await p.keyboard.press('Escape');
  await p.waitForTimeout(300);

  // 6. Storage stays in the old shape (React app can read it back)
  await p.waitForTimeout(400);
  const ls = { c: await h.stored.collections(), t: (await h.stored.tabs()).tabs, ctx: await h.stored.contexts() };
  const billing = ls.c.find(c => c.name === 'Billing API');
  check('storage shape unchanged', billing && billing.variables.token === 'abc' && billing.requests.every(r => 'url' in r && 'headers' in r && 'body' in r) && ls.t.every(t => 'requestBody' in t && 'loadedCollectionId' in t), '');
  check('context persisted', ls.ctx[billingId]?.postId === 101, JSON.stringify(ls.ctx));
  if (h.tauri) {
    const hist = await h.stored.history();
    check('[tauri] history kept in the Rust shape, with the old entry', hist.some((x) => x.url.endsWith('/todos/1') && x.status_code === 200) && hist.some((x) => x.url === 'https://example.com/' && typeof x.timestamp === 'number' && x.headers), JSON.stringify(hist.map((x) => x.url)));
    const envs = await h.stored.environments();
    // The picker (next to Send) offers what the store holds.
    const options = await p.evaluate(() => { const s = document.querySelector('hitop-app').store; return s.visibleEnvironments.map((e) => s.environmentLabel(e)); });
    check('[tauri] old desktop environments load, secret values stay in the keychain', options.includes('Prod') && envs.list[0]?.secrets?.[0] === 'token' && !JSON.stringify(envs).includes('tok-1') && h.mock.keychain.get('global/Prod/token') === 'tok-1', `${options} ${JSON.stringify(envs)}`);
  }

  // 7. Export → import round trip (Postman)
  await p.locator('aside').getByRole('button', { name: 'Export' }).click();
  await p.waitForTimeout(400);
  const exported = await p.locator('#dialog lit-material-code-block').innerText();
  check('export has HITOP JSON', exported.includes('Billing API'));
  await p.keyboard.press('Escape');

  // 8. Mobile + dark
  await p.setViewportSize({ width: 390, height: 844 });
  await p.emulateMedia({ colorScheme: 'dark' });
  await p.evaluate(() => { document.documentElement.removeAttribute('color-scheme'); });
  await p.locator('aside').getByRole('button', { name: /Default/ }).click({ force: true }).catch(() => {});
  await p.waitForTimeout(300);
  await p.getByRole('button', { name: 'Open collections' }).click();
  await p.waitForTimeout(700);
  await p.screenshot({ path: `${out}/new-mobile-nav.png` });
  const hscroll = await p.evaluate(() => document.documentElement.scrollWidth > innerWidth);
  check('no horizontal scroll at 390px', !hscroll);

  check('no console errors', errors.length === 0, errors.slice(0, 3).join(' || '));
  await h.finish();
})().catch(e => { console.log(results.join('\n')); console.error('CRASH', e.message); process.exit(1); });
