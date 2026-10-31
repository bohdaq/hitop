// End-to-end check of the web app against a running hitop-server (the proxy
// must allow the origin). Seeds localStorage with data the React app wrote
// (tests/legacy-storage.json), so it also proves the storage format is unchanged.
//   BASE=http://localhost:7879 npm run test:e2e  [-- <screenshot dir>]
// Needs network: requests go to example.com and jsonplaceholder.typicode.com.
const { chromium } = require('playwright');
const fs = require('fs');
const BASE = process.env.BASE || 'http://localhost:7879';
const out = process.argv[2] || require('os').tmpdir();
const fixture = JSON.parse(fs.readFileSync(require('path').join(__dirname, 'legacy-storage.json'), 'utf8'));
const results = [];
const check = (name, ok, extra = '') => { results.push(`${ok ? 'PASS' : 'FAIL'} ${name}${extra ? ' — ' + extra : ''}`); };
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1280, height: 860 } });
  await ctx.addInitScript((f) => { if (!sessionStorage.getItem('seeded')) { for (const [k, v] of Object.entries(f)) localStorage.setItem(k, v); sessionStorage.setItem('seeded', '1'); } }, fixture);
  const p = await ctx.newPage();
  const errors = []; p.on('pageerror', e => errors.push(e.message)); p.on('console', m => m.type() === 'error' && errors.push(m.text()));
  const proxied = []; p.on('request', r => r.url().includes('/api/proxy') && proxied.push(JSON.parse(r.postData())));
  await p.goto(BASE + '/app/', { waitUntil: 'networkidle' });

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
  check('collection run: 2 requests ok', (run.match(/20\d/g) || []).length === 2, run.replace(/\n/g, ' | '));
  const sent2 = proxied.find(x => x.url.endsWith('/posts/1'));
  const prev = sent2?.headers.find(h => h[0] === 'X-Prev');
  check('variables interpolated + context passed between requests', proxied.some(x => x.url === 'https://jsonplaceholder.typicode.com/posts' && x.headers.some(h => h[0] === 'Authorization' && h[1] === 'Bearer abc')) && prev && prev[1] === '101', `X-Prev=${prev?.[1]}`);
  await p.screenshot({ path: `${out}/new-collection.png` });

  // 4. Save the current tab into Billing API with a new name
  await p.locator('aside').getByRole('button', { name: /Create post/ }).click();
  await p.getByRole('button', { name: 'Save request' }).click();
  const name = p.locator('#dialog #name');
  await name.waitFor();
  await name.evaluate((el) => { el.value = 'Create post v2'; });
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await p.waitForTimeout(400);
  check('save renames request', (await p.locator('aside').innerText()).includes('Create post v2'));

  // 5. History dialog
  await p.locator('aside').getByRole('button', { name: 'History' }).click();
  await p.waitForTimeout(400);
  const hist = await p.locator('.history-list').innerText().catch(() => '');
  check('history lists old + new entries', hist.includes('todos/1') && hist.includes('example.com'), `${hist.split('\n').length} lines`);
  await p.screenshot({ path: `${out}/new-history.png` });
  await p.keyboard.press('Escape');
  await p.waitForTimeout(300);

  // 6. Storage stays in the old shape (React app can read it back)
  const ls = await p.evaluate(() => ({ c: JSON.parse(localStorage.getItem('hitop_collections')), t: JSON.parse(localStorage.getItem('hitop_tabs')), ctx: JSON.parse(localStorage.getItem('hitop_contexts')) }));
  const billing = ls.c.find(c => c.name === 'Billing API');
  check('storage shape unchanged', billing && billing.variables.token === 'abc' && billing.requests.every(r => 'url' in r && 'headers' in r && 'body' in r) && ls.t.every(t => 'requestBody' in t && 'loadedCollectionId' in t), '');
  check('context persisted', ls.ctx['col-legacy']?.postId === 101, JSON.stringify(ls.ctx));

  // 7. Export → import round trip (Postman)
  await p.locator('aside').getByRole('button', { name: 'Export' }).click();
  await p.waitForTimeout(400);
  const exported = await p.locator('#dialog lit-material-code-block').innerText();
  check('export has HITOP JSON', exported.includes('Billing API'));
  await p.keyboard.press('Escape');

  // 8. Mobile + dark
  await p.setViewportSize({ width: 390, height: 844 });
  await p.emulateMedia({ colorScheme: 'dark' });
  await p.evaluate(() => { delete document.documentElement.dataset.theme; });
  await p.locator('aside').getByRole('button', { name: /Default/ }).click({ force: true }).catch(() => {});
  await p.waitForTimeout(300);
  await p.getByRole('button', { name: 'Open collections' }).click();
  await p.waitForTimeout(700);
  await p.screenshot({ path: `${out}/new-mobile-nav.png` });
  const hscroll = await p.evaluate(() => document.documentElement.scrollWidth > innerWidth);
  check('no horizontal scroll at 390px', !hscroll);

  check('no console errors', errors.length === 0, errors.slice(0, 3).join(' || '));
  console.log(results.join('\n'));
  await b.close();
  if (results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})().catch(e => { console.log(results.join('\n')); console.error('CRASH', e.message); process.exit(1); });
