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

  // --- U19 -------------------------------------------------------------------------------------
  await store((b) => {
    const s = document.querySelector('hitop-app').store;
    s.environments = [...s.environments, { id: 'env-u19', name: 'Staging', variables: { baseUrl: b }, secrets: ['TOKEN'] }];
    s.setActiveEnvironment('env-u19');
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

  check('no page errors', errors.length === 0, errors.join(' | '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
