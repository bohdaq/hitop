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

  // 5. A large response isn't kept whole in localStorage: since 1.2 it goes to
  // IndexedDB and comes back whole after a reload (ux12.cjs has the fallbacks).
  await url.fill(`${base}/big`);
  await sendAndWait();
  const big = await tab();
  check('large response: localStorage keeps its first 16 KB, marked as stored', big.response.length === 16 * 1024 && big.responseCut === true && big.responseStored === true, `${big.response.length}`);
  check('… while the open tab still shows all of it', (await p.locator('.response-meta').innerText()).includes('600.0 KB'));
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  check('after a reload: the whole body is back, no note', (await p.locator('.response-meta').innerText()).includes('600.0 KB') && (await p.locator('.response-note').count()) === 0);

  // 6. A full storage is reported (once), not only in the console.
  await p.evaluate(() => (window.__storageFull = true));
  await url.fill(`${base}/after-full`);
  await settle(p);
  const toast = await p.locator('#toast').evaluate((el) => el.shadowRoot?.textContent + el.textContent);
  check('full storage: the toast says tabs could not be saved', toast.includes("Couldn't save your tabs"), toast.trim().slice(0, 160));
  await p.evaluate(() => (window.__storageFull = false));

  // 7. Drafts are titled from their URL; the selected tab is scrolled into view.
  // (The first tab is the first-run "New request" saved in Default; a new tab is a draft.)
  await p.getByRole('button', { name: 'New tab' }).click();
  await url.fill(`${host}/users/7`);
  await settle(p);
  const draftTitle = await p.locator('.tab-strip lit-material-tab[selected]').first().innerText();
  check('a draft is titled METHOD host/path as its URL is typed', draftTitle.trim() === `GET ${host}/users/7`, draftTitle);
  for (let i = 0; i < 12; i++) await p.getByRole('button', { name: 'New tab' }).click();
  await settle(p);
  const inView = await p.evaluate(() => {
    const tabs = [...document.querySelectorAll('.tab-strip lit-material-tab')];
    const last = tabs.at(-1).getBoundingClientRect();
    const strip = document.querySelector('.tab-strip lit-material-tabs').getBoundingClientRect();
    return last.left >= strip.left - 1 && last.right <= strip.right + 1 ? true : JSON.stringify([last.left, last.right, strip.left, strip.right, tabs.length]);
  });
  check('the newest (selected) tab is scrolled into view', inView === true, String(inView));

  // 8. Keyboard: Mod+Enter sends from the body editor, Mod+S saves, Alt+Shift+N/W, ? lists them.
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control';
  await method('POST');
  await url.fill(`${base}/echo`);
  await p.getByRole('tab', { name: 'Body' }).click();
  await p.locator('.cm-content').click();
  await p.keyboard.type('{"k": 2}', { delay: 10 });
  const before = got.length;
  await p.keyboard.press(`${mod}+Enter`);
  await p.locator('.response-meta').waitFor({ timeout: 15000 });
  await settle(p);
  check('Mod+Enter in the body editor sends', got.length === before + 1 && got.at(-1).body === '{"k": 2}' && !(await tab()).requestBody.includes('\n'), JSON.stringify(got.at(-1)?.body));
  await p.keyboard.press(`${mod}+s`);
  await p.locator('#dialog #name').waitFor({ timeout: 5000 });
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'Echo'));
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await settle(p);
  check('Mod+S on a draft asks where, then saves', (await h.stored.collections()).some((c) => c.requests.some((r) => r.name === 'Echo')));

  // 9. Unsaved edits: a dot on the tab and in the sidebar; closing asks; Mod+S saves in place.
  await url.fill(`${base}/echo?v=2`);
  await settle(p);
  const echoRow = p.locator('aside .request-item', { hasText: 'Echo' });
  const dots = [await p.locator('.tab-strip lit-material-tab[selected] .dirty-dot').count(), await echoRow.locator('.dirty-dot').count()];
  check('edited saved request: dot on its tab and sidebar row', dots[0] === 1 && dots[1] === 1, JSON.stringify(dots));
  const tabsBefore = (await h.stored.tabs()).tabs.length;
  await p.keyboard.press('Alt+Shift+KeyW');
  // The confirm dialog's host is display: contents; wait for its buttons.
  const ask = p.locator('lit-material-confirm-dialog[open]');
  await ask.getByRole('button', { name: 'Cancel' }).waitFor({ timeout: 5000 });
  check('closing it asks to discard', await p.getByText('Discard changes to Echo?').isVisible());
  await ask.getByRole('button', { name: 'Cancel' }).click();
  await settle(p);
  check('Cancel keeps the tab and the edit', (await h.stored.tabs()).tabs.length === tabsBefore && (await tab()).url.endsWith('?v=2'));
  await p.locator('.response-meta').click();
  await p.keyboard.press(`${mod}+s`);
  await settle(p);
  const echoSaved = (await h.stored.collections()).flatMap((c) => c.requests).find((r) => r.name === 'Echo');
  check('Mod+S on a saved request saves in place (no dialog), the dot goes', echoSaved?.url.endsWith('?v=2') && (await p.locator('#dialog #name').count()) === 0 && (await echoRow.locator('.dirty-dot').count()) === 0, echoSaved?.url);
  await p.keyboard.press('Alt+Shift+KeyN');
  await settle(p);
  check('Alt+Shift+N opens a tab', (await h.stored.tabs()).tabs.length === tabsBefore + 1);
  await p.keyboard.press('Alt+Shift+KeyW');
  await settle(p);
  check('Alt+Shift+W closes an empty tab without asking', (await h.stored.tabs()).tabs.length === tabsBefore && (await p.locator('lit-material-confirm-dialog[open]').count()) === 0);
  await p.locator('.response-meta').click();
  await p.keyboard.press('Shift+Slash');
  await p.locator('#dialog .shortcut-list').waitFor({ timeout: 5000 });
  check('? lists the shortcuts', (await p.locator('#dialog .shortcut-list').innerText()).includes('Send the request'));
  await p.keyboard.press('Escape');
  await settle(p);

  // 10. WebSocket opens on Messages.
  await p.getByRole('button', { name: 'New tab' }).click();
  await method('WebSocket');
  await settle(p);
  check('switching to WebSocket shows Messages', (await p.locator('hitop-request .editor lit-material-tab[selected], hitop-request .editor lit-material-tab[aria-selected="true"]').first().innerText()).startsWith('Messages'));

  // 11. Import: a file fills the text area; replacing collections with requests asks first.
  const exportJson = JSON.stringify([{ id: 'imp-1', name: 'Imported', requests: [{ id: 'r-1', name: 'From file', url: `${base}/json`, method: 'GET', headers: [], body: '' }] }]);
  await p.locator('aside').getByRole('button', { name: 'Import', exact: true }).click();
  await p.locator('#dialog input[type=file]').setInputFiles({ name: 'export.json', mimeType: 'application/json', buffer: Buffer.from(exportJson) });
  await settle(p);
  check('the chosen file fills the text area', (await p.locator('#dialog #json').evaluate((el) => el.value)) === exportJson && (await p.locator('#dialog .file-name').innerText()) === 'export.json');
  // Adding is the default since 1.2 (ux12.cjs); Replace all keeps the confirm.
  await p.locator('#dialog').getByRole('radio', { name: /Replace all/ }).check();
  await p.locator('#dialog').getByRole('button', { name: /import/i }).click();
  await ask.getByRole('button', { name: 'Cancel' }).waitFor({ timeout: 5000 });
  check('replacing asks first, naming what goes', await p.getByText(/Importing deletes .*Default/).isVisible());
  await ask.getByRole('button', { name: 'Cancel' }).click();
  await settle(p);
  check('Cancel keeps the collections and the dialog', (await h.stored.collections()).some((c) => c.requests.some((r) => r.name === 'Echo')) && (await p.locator('#dialog #json').count()) === 1);
  await p.locator('#dialog').getByRole('button', { name: /import/i }).click();
  await ask.getByRole('button', { name: 'Replace', exact: true }).click();
  await settle(p);
  const after = await h.stored.collections();
  check('Replace imports', after.length === 1 && after[0].name === 'Imported', after.map((c) => c.name).join());

  // 12. Top bar: logo at the left (a site link on the web), environment picker at the right; sidebar starts with New request.
  const brand = p.locator('header.topbar .brand');
  check('top bar has the logo and HITOP', (await brand.innerText()).trim() === 'HITOP' && (await p.locator('aside .brand').count()) === 0);
  check(TAURI ? '[tauri] logo is not a link (the webview would leave the app)' : 'logo links to the site, named "HITOP website"', TAURI ? (await brand.evaluate((el) => el.localName)) === 'span' : (await brand.getAttribute('aria-label')) === 'HITOP website' && (await brand.getAttribute('href')) === '/');
  check('environment picker and theme toggle in the top bar', (await p.locator('header.topbar').getByRole('combobox', { name: 'Environment' }).isVisible()) && (await p.locator('header.topbar lit-material-color-scheme-toggle').isVisible()));
  const first = await p.locator('aside').evaluate((el) => [...el.querySelectorAll('lit-material-button, button, input')].find((b) => b.offsetParent !== null)?.textContent.trim());
  check('sidebar starts with New request', first === 'New request', first);
  const footer = await p.locator('.sidebar-footer').boundingBox();
  check('sidebar footer stays on screen', footer && footer.y + footer.height <= 860 + 1, JSON.stringify(footer));
  await p.setViewportSize({ width: 390, height: 844 });
  await settle(p);
  check('phone: top bar is the menu button and the logo', (await p.getByRole('button', { name: 'Open collections' }).isVisible()) && (await brand.isVisible()) && !(await p.locator('header.topbar .app-tools').isVisible()));
  await p.getByRole('button', { name: 'Open collections' }).click();
  await settle(p);
  check('phone: the drawer has the environment picker and the theme', (await p.locator('aside').getByRole('combobox', { name: 'Environment' }).isVisible()) && (await p.locator('aside lit-material-color-scheme-toggle').isVisible()));
  check('phone: no horizontal scroll', !(await p.evaluate(() => document.documentElement.scrollWidth > innerWidth)));

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message);
  process.exit(1);
});
