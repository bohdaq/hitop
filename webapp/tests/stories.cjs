// User stories closed after the HITOP Next redesign (HITOP_NEXT_UI.md, "Stories audit"):
//   U19 a secret with no value on this device: named before Send, the send stopped with a note
//       that says where to fill it in, the URL chip, the Environments page; filled → sent.
// Local Node server only.
//   BASE=http://localhost:7879 node tests/stories.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

function api() {
  const hits = [];
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    hits.push({ url: req.url, auth: req.headers.authorization ?? null });
    const url = new URL(req.url, 'http://x');
    const status = url.pathname.startsWith('/missing') ? 404 : 200;
    res.writeHead(status, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify({ path: url.pathname, ok: status === 200 }));
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, hits })));
}

(async () => {
  const { server, hits } = await api();
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await settle(p);
  const store = (fn, arg) => p.evaluate(fn, arg);

  // --- U6 --------------------------------------------------------------------------------------
  const gs = p.locator('aside .getting-started');
  const steps = async () => (await gs.locator('.gs-steps li').evaluateAll((els) => els.map((e) => `${e.classList.contains('done') ? 'x' : '-'} ${e.textContent.trim()}`))).join(' | ');
  check('U6 a fresh app shows the three steps, the first one next', (await gs.count()) === 1 && (await steps()) === '- Send a request | - Save a request to a collection | - Run a collection' && (await gs.locator('li[aria-current="step"]').innerText()).includes('Send a request'), await steps().catch(() => ''));
  check('U6 ...with its count and a hint for the next step', (await gs.locator('.gs-count').innerText()) === '0/3' && (await gs.locator('.gs-hint').innerText()).includes('press Send'));
  await gs.getByRole('button', { name: 'Open the sample' }).click();
  await settle(p);
  check('U6 Open the sample opens the sample collection; sending is still the next step', (await p.locator('h1').innerText()).includes('Sample: JSONPlaceholder') && (await gs.getByRole('button', { name: 'Open the sample' }).count()) === 0 && (await gs.locator('.gs-count').innerText()) === '0/3');

  // --- U19 -------------------------------------------------------------------------------------
  await store((b) => {
    const s = document.querySelector('hitop-app').store;
    s.environments = [...s.environments, { id: 'env-u19', name: 'Staging', variables: { baseUrl: b }, secrets: ['TOKEN'] }];
    s.setActiveEnvironment('env-u19');
    s.show('request');
    s.updateTab({ url: '${baseUrl}/orders?t=${TOKEN}', headers: [{ name: 'Authorization', value: 'Bearer ${TOKEN}' }, { name: '', value: '' }] });
  }, base);
  await settle(p);
  const note = p.locator('hitop-request .secret-missing-note');
  check('U19 a secret with no value here is named before Send', (await note.count()) === 1 && (await note.innerText()).includes('TOKEN') && (await note.getAttribute('role')) === 'status', await note.innerText().catch(() => ''));
  check('U19 ...and its URL chip is marked', (await p.locator('.url-vars .var-chip.secret-missing').count()) === 1 && (await p.locator('.url-vars .var-chip.missing').count()) === 1);
  const before = hits.length;
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.locator('.response[data-state="not-sent"]').waitFor({ timeout: 10000 });
  const pane = await p.locator('.response[data-state="not-sent"]').innerText();
  check('U19 Send stops with a note saying where to fill it in', pane.includes('Not sent') && pane.includes('Secret TOKEN has no value on this device') && pane.includes('Environments → Staging → Secrets'), pane);
  check('U19 ...and nothing was sent', hits.length === before && !h.sent.some((x) => JSON.stringify(x).includes('/orders')));
  await p.locator('.url-vars .var-chip.secret-missing').click();
  await p.locator('.var-peek').waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});
  const peek = await p.locator('.var-peek').innerText().catch(() => '');
  check('U19 the variable peek says it is a secret with no value on this device', peek.includes('no value on this device') && peek.includes('Staging'), peek);
  await p.keyboard.press('Escape');

  // The Environments page: the secret has a row to fill and the page says it is empty here.
  await note.getByRole('button', { name: 'Fill in secrets' }).click();
  await settle(p);
  const envNote = p.locator('.env-secrets .secret-missing-note');
  check('U19 Environments names the secret with no value', (await envNote.count()) === 1 && (await envNote.innerText()).includes('TOKEN'), await envNote.innerText().catch(() => ''));
  check('U19 ...and gives it a row to fill in', await p.locator('.env-secrets lit-material-key-value-editor').evaluate((el) => (el.entries || []).some((r) => r.key === 'TOKEN')));

  // Filled in: the note goes and the request is sent with it.
  await store(() => {
    const s = document.querySelector('hitop-app').store;
    s.setEnvironmentVariables('env-u19', { ...s.activeEnvironment.variables, TOKEN: 'tok-u19' }, ['TOKEN']);
  });
  await settle(p);
  check('U19 a filled secret takes the note away', (await envNote.count()) === 0);
  await store(() => document.querySelector('hitop-app').store.show('request'));
  await settle(p);
  check('U19 ...also under the request bar', (await note.count()) === 0);
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.locator('.response-meta').waitFor({ timeout: 10000 });
  const hit = hits.at(-1);
  check('U19 ...and the request goes out with it', hit?.auth === 'Bearer tok-u19' && hit.url.includes('t=tok-u19'), JSON.stringify(hit));
  check('U6 a sent request checks off the first step', (await steps()).startsWith('x Send a request (done) | - Save') && (await gs.locator('.gs-count').innerText()) === '1/3', await steps());

  // --- U39 -------------------------------------------------------------------------------------
  await store((b) => {
    const s = document.querySelector('hitop-app').store;
    s.setActiveEnvironment(null);
    s.addCollection('Runs U39');
    const c = s.collections.find((x) => x.name === 'Runs U39');
    const r = (id, name, url, file) => ({ id, name, method: 'GET', url, headers: [], body: '', ...(file ? { file } : {}) });
    s.collections = s.collections.map((x) => (x.id === c.id ? { ...x, variables: { base: b }, requests: [r(3901, 'List orders', '${base}/orders', 'orders/list'), r(3902, 'Get order', '${base}/missing/7', 'orders/get'), r(3903, 'Health', '${base}/health')] } : x));
    s.show('collection', c.id);
    s.emit();
  }, base);
  await settle(p);
  check('U6 a request saved in a collection of your own checks off the second step', (await gs.locator('.gs-count').innerText().catch(() => '')) === '2/3', await steps().catch(() => ''));
  const history = () => store(() => JSON.parse(localStorage.getItem('hitop_run_history') || '{}'));
  const runsBefore = Object.values(await history()).flat().length;
  await p.locator('hitop-collection').getByRole('button', { name: /Run collection/ }).click();
  const report = p.locator('hitop-run-report');
  await report.locator('.run-report-head h2', { hasText: 'Last run' }).waitFor({ timeout: 15000 });
  const card = await report.locator('.run-failure').innerText();
  check('U39 the failure says expected vs actual and the usual cause of the status', card.includes('Get order') && card.includes('HTTP 404') && card.includes('Likely cause') && card.includes('Nothing answers at this path'), card.replace(/\n/g, ' | '));
  check('U39 ...the URL as sent and the start of the response body', card.includes(`${base}/missing/7`) && card.includes('"ok":false'), card.replace(/\n/g, ' | '));
  check('U6 a run completes the steps and the card goes', (await gs.count()) === 0);
  const notRun = await report.locator('.run-not-run').innerText().catch(() => '');
  check('U39 the report says what was not run after the failure', notRun.includes('1 more request was not run'), notRun);
  check('U39 ...and the hitop run line stops at the first failure too', (await report.locator('.run-command').innerText()).includes('--bail'));
  await report.getByRole('button', { name: 'Open the request' }).click();
  await settle(p);
  check('U39 Open the request opens the failing request', (await store(() => document.querySelector('hitop-app').store.tab.title)) === 'Get order' && (await store(() => document.querySelector('hitop-app').store.view.kind)) === 'request');

  // A folder's run: from the folder's menu, only its requests, no CLI line, not in the run history.
  const runsAfterCollection = Object.values(await history()).flat().length;
  await store(() => {
    const app = document.querySelector('hitop-app');
    const c = app.store.collections.find((x) => x.name === 'Runs U39');
    app.store.collections = app.store.collections.map((x) => (x.id === c.id ? { ...x, requests: x.requests.map((r) => (r.id === 3902 ? { ...r, url: '${base}/orders/7' } : r)) } : x));
    app.store.emit();
  });
  await settle(p);
  await p.locator('aside').getByRole('button', { name: 'More for folder orders' }).click();
  await p.locator('#folder-menu').getByText('Run this folder').click();
  await report.locator('.run-report-head h2', { hasText: 'Last run' }).waitFor({ timeout: 15000 });
  await settle(p);
  const head = await report.locator('.run-report-head').innerText();
  const rows = await report.locator('.run-list .run-row').count();
  check("U39 Run this folder runs only the folder's requests and says so", head.includes('folder') && head.includes('orders') && rows === 2 && head.includes('Passed'), `${head.replace(/\n/g, ' | ')} rows=${rows}`);
  check('U39 ...with no hitop run line (it runs whole collections) and no comparison', (await report.locator('.run-command').count()) === 0 && (await report.locator('.run-cli-none').count()) === 1 && Object.values(await history()).flat().length === runsAfterCollection && runsAfterCollection === runsBefore + 1);

  // Someone who already had requests when this version first started never gets the card.
  await p.evaluate(() => localStorage.removeItem('hitop_getting_started'));
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  check('U6 an app that already has requests shows no getting-started card', (await gs.count()) === 0 && (await p.evaluate(() => JSON.parse(localStorage.getItem('hitop_getting_started')))).fresh === false);

  check('no page errors', errors.length === 0, errors.join(' | '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
