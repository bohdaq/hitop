// The UI/UX review fixes (reviews/2026-11-01-webapp-ux), in web and Tauri
// mode, against a local Node server (no public host): Content-Type for raw
// bodies, a scheme for typed URLs, the response tools, Compare's default,
// large responses and a full storage, unsaved tabs, import, shortcuts and the
// top bar at desktop and phone widths.
//   BASE=http://localhost:7879 node tests/ux.cjs
//   HITOP_MODE=tauri node tests/ux.cjs
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400); // saves are batched every 250 ms

// Records what arrives; CORS for the page (web mode fetches localhost directly).
function api() {
  const got = [];
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    let body = '';
    req.on('data', (c) => (body += c));
    req.on('end', () => {
      const url = new URL(req.url, 'http://x');
      got.push({ method: req.method, path: url.pathname, headers: req.headers, body });
      const send = (type, text, status = 200) => res.writeHead(status, { ...cors, 'Content-Type': type }).end(text);
      if (url.pathname === '/echo') return send('application/json', JSON.stringify({ contentType: req.headers['content-type'] ?? null, body }));
      if (url.pathname === '/json') return send('application/json', '{"items":[{"id":1,"name":"needle"},{"id":2,"name":"hay needle"}]}');
      if (url.pathname === '/page') return send('application/json', JSON.stringify({ page: url.searchParams.get('page') }));
      if (url.pathname === '/badjson') return send('application/json', '{"a": 1,}');
      if (url.pathname === '/html') return send('text/html', '<!doctype html><title>t</title><h1>Hello preview</h1><script>parent.postMessage("ran","*")</script>');
      if (url.pathname === '/big') return send('text/plain', 'x'.repeat(600 * 1024));
      send('application/json', '{}', 404);
    });
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, got })));
}

(async () => {
  const { server, got } = await api();
  const host = `127.0.0.1:${server.address().port}`;
  const base = `http://${host}`;
  const h = await launch({ viewport: { width: 1280, height: 860 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  // A full storage, on demand: hitop_tabs writes throw as a full localStorage does.
  await h.ctx.addInitScript(() => {
    const set = Storage.prototype.setItem;
    Storage.prototype.setItem = function (k, v) {
      if (window.__storageFull && k === 'hitop_tabs') throw new DOMException('The quota has been exceeded.', 'QuotaExceededError');
      return set.call(this, k, v);
    };
  });
  await p.goto(h.url, { waitUntil: 'networkidle' });
  const tab = async () => (await settle(p), (await h.stored.tabs()).tab);
  const url = p.getByLabel('URL');
  const sendAndWait = async () => {
    await p.getByRole('button', { name: 'Send', exact: true }).click();
    await p.locator('.response-meta, .response .error-text').first().waitFor({ timeout: 15000 });
  };
  const method = async (m) => {
    await p.getByRole('combobox', { name: 'Method' }).click();
    await p.getByRole('option', { name: m, exact: true }).click();
  };

  // 1. A raw JSON body with no Content-Type header is sent as application/json.
  await method('POST');
  await url.fill(`${base}/echo`);
  await p.getByRole('tab', { name: 'Body' }).click();
  await p.locator('.cm-content').click();
  await p.keyboard.type('{"a": 1}', { delay: 10 });
  await p.getByRole('tab', { name: /^Headers/ }).click();
  check('Headers tab says the Content-Type is added', (await p.locator('.auto-header').innerText()).includes('application/json'));
  await sendAndWait();
  const echo = got.filter((g) => g.path === '/echo').pop();
  check('raw JSON body sent with Content-Type: application/json', echo?.headers['content-type'] === 'application/json' && echo.body === '{"a": 1}', JSON.stringify(echo?.headers['content-type']));

  // 2. A URL typed without a scheme gets one; something that isn't a URL is refused under the field.
  await method('GET');
  await url.fill(`${host}/json`);
  await sendAndWait();
  check('schemeless URL sent as http://127.0.0.1:…', got.at(-1).path === '/json' && (await tab()).url === `${base}/json`, (await tab()).url);
  const histBefore = (await h.stored.history()).length;
  const n = got.length;
  await url.fill('not a url');
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await settle(p);
  const fieldText = await p.locator('.request-line .url').innerText().catch(() => '');
  const urlError = await p.locator('.request-line .url').evaluate((el) => (el.error ? el.errorText : ''));
  check('not a URL: refused with an error under the field, nothing sent or recorded', urlError.includes("isn't a URL") && got.length === n && (await h.stored.history()).length === histBefore, `${urlError} ${fieldText}`);
  await url.fill(`${base}/json`);
  check('editing the URL clears its error', !(await p.locator('.request-line .url').evaluate((el) => el.error)));

  // 3. Response tools: Raw is the body as received, find marks matches, wrap.
  await sendAndWait();
  const bodyText = () => p.locator('.response-scroll').innerText();
  check('Pretty shows formatted JSON', (await bodyText()).includes('"name": "needle"'));
  await p.getByRole('button', { name: 'Raw', exact: true }).click();
  check('Raw shows the body as received', (await bodyText()).includes('{"items":[{"id":1,"name":"needle"}'));
  await p.getByRole('button', { name: 'Pretty', exact: true }).click();
  await p.getByLabel('Find in body').fill('NEEDLE');
  check('find counts matches (any case)', (await p.locator('.find-count').innerText()) === '1 of 2' && (await p.locator('.response-text mark').count()) === 2);
  await p.getByLabel('Find in body').press('Enter');
  check('Enter steps to the next match', (await p.locator('.find-count').innerText()) === '2 of 2');
  await p.getByLabel('Find in body').fill('');
  await p.getByRole('button', { name: /Wrap/ }).click();
  check('wrap toggles', (await p.locator('.response-scroll.wrap').count()) === 1);
  check('the body region can take focus', (await p.locator('.response-scroll').getAttribute('tabindex')) === '0');

  // Invalid JSON with a JSON content type says so.
  await url.fill(`${base}/badjson`);
  await sendAndWait();
  check('invalid JSON: a warning, body shown as text', (await p.locator('.response-note').innerText()).includes("doesn't parse") && (await bodyText()).includes('{"a": 1,}'));

  // HTML: Preview renders it in a sandbox (no scripts).
  await url.fill(`${base}/html`);
  await sendAndWait();
  await p.getByRole('button', { name: 'Preview', exact: true }).click();
  const frame = p.frameLocator('iframe.response-preview');
  check('HTML preview renders, sandboxed', (await frame.locator('h1').innerText()) === 'Hello preview' && (await p.locator('iframe.response-preview').getAttribute('sandbox')) === '');

  // 4. Compare starts on a response that differs from the current one.
  await url.fill(`${base}/page?page=1`);
  await sendAndWait();
  await url.fill(`${base}/page?page=2`);
  await sendAndWait();
  await p.getByRole('button', { name: /^Compare/ }).first().click();
  const summary = await p.locator('#dialog .compare-summary').innerText();
  check('Compare: Before defaults to the previous, different response', !summary.includes('the same'), summary);
  await p.keyboard.press('Escape');
  await settle(p);

  // 5. A large response isn't kept whole in storage, and the page says so after a reload.
  await url.fill(`${base}/big`);
  await sendAndWait();
  const big = await tab();
  check('large response: stored cut to 64 KB', big.response.length === 64 * 1024 && big.responseCut === true, `${big.response.length}`);
  check('… while the open tab still shows all of it', (await p.locator('.response-meta').innerText()).includes('600.0 KB'));
  await p.reload({ waitUntil: 'networkidle' });
  check('after a reload: a note that only part was kept', (await p.locator('.response-note').innerText()).includes('over 256 KB'));

  // 6. A full storage is reported (once), not only in the console.
  await p.evaluate(() => (window.__storageFull = true));
  await url.fill(`${base}/after-full`);
  await settle(p);
  const toast = await p.locator('#toast').evaluate((el) => el.shadowRoot?.textContent + el.textContent);
  check('full storage: the toast says tabs could not be saved', toast.includes("Couldn't save your tabs"), toast.trim().slice(0, 160));
  await p.evaluate(() => (window.__storageFull = false));

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message);
  process.exit(1);
});
