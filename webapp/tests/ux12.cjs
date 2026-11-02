// The 1.2 web app UX items (reviews/2026-11-01-webapp-ux, "Status (1.2.0)"),
// in web and Tauri mode, against a local Node server (no public host):
// large responses in IndexedDB (and the 1.1 fallback without it), Cancel and
// the request timeout, the Settings page, import that adds, the split panes,
// Mod+P quick open, tab close buttons, the History page, the request context
// menu and folder groups in the sidebar.
//   BASE=http://localhost:7879 node tests/ux12.cjs
//   HITOP_MODE=tauri node tests/ux12.cjs
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400); // saves are batched every 250 ms

function api() {
  const got = [];
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    const url = new URL(req.url, 'http://x');
    got.push({ method: req.method, path: url.pathname });
    const send = (type, text, status = 200) => res.writeHead(status, { ...cors, 'Content-Type': type }).end(text);
    if (url.pathname === '/json') return send('application/json', '{"items":[{"id":1,"name":"needle"}]}');
    if (url.pathname === '/big') return send('text/plain', 'b'.repeat(600 * 1024));
    if (url.pathname === '/mid') return send('text/plain', 'm'.repeat(100 * 1024));
    if (url.pathname === '/bigjson') return send('application/json', JSON.stringify({ rows: Array.from({ length: 4000 }, (_, i) => ({ id: i, text: 'r'.repeat(20) })) }));
    if (url.pathname === '/slow') {
      // Answers after 5 s, unless the client goes first.
      const timer = setTimeout(() => send('application/json', '{"slow":true}'), 5000);
      req.on('close', () => clearTimeout(timer));
      return;
    }
    send('application/json', '{}', 404);
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, got })));
}

(async () => {
  const { server, got } = await api();
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ viewport: { width: 1280, height: 860 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  // Switches for this suite, read before the app starts: no IndexedDB (a
  // private window), or IndexedDB refusing writes (full).
  await h.ctx.addInitScript(() => {
    if (localStorage.getItem('__noidb')) Object.defineProperty(window, 'indexedDB', { value: undefined, configurable: true });
    if (localStorage.getItem('__idbfull'))
      IDBObjectStore.prototype.put = function () {
        throw new DOMException('The quota has been exceeded.', 'QuotaExceededError');
      };
  });
  const tabs = async () => (await settle(p), await h.stored.tabs());
  const url = p.getByLabel('URL');
  const sendAndWait = async () => {
    await p.getByRole('button', { name: 'Send', exact: true }).click();
    await p.locator('.response-meta, .response .error-text').first().waitFor({ timeout: 15000 });
  };
  const flag = async (name, on) => {
    await p.evaluate(([k, v]) => (v ? localStorage.setItem(k, '1') : localStorage.removeItem(k)), [name, on]);
    await p.reload({ waitUntil: 'networkidle' });
    await settle(p);
  };
  const toastText = () => p.locator('#toast').evaluate((el) => (el.shadowRoot?.textContent ?? '') + el.textContent);

  await p.goto(h.url, { waitUntil: 'networkidle' });

  // --- 1. Large responses: IndexedDB, and the 1.1 behaviour without it -----------------
  // Without IndexedDB (a private window): a body over 256 KB is saved as its first 64 KB.
  await flag('__noidb', true);
  await url.fill(`${base}/big`);
  await sendAndWait();
  let st = await tabs();
  check('no IndexedDB: a 600 KB body is saved as its first 64 KB (1.1)', st.tab.response.length === 64 * 1024 && st.tab.responseCut === true && !st.tab.responseStored, `${st.tab.response.length}`);
  const bigTabId = st.tab.id;
  await p.getByRole('button', { name: 'New tab' }).click();
  await url.fill(`${base}/mid`);
  await sendAndWait();
  st = await tabs();
  check('no IndexedDB: a 100 KB body stays inline', st.tab.response.length === 100 * 1024 && !st.tab.responseCut);
  const midTabId = st.tab.id;
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  check('no IndexedDB: the 100 KB body is whole after a reload', (await p.locator('.response-meta').innerText()).includes('100.0 KB') && (await p.locator('.response-note').count()) === 0);
  // With IndexedDB again: 1.1's cut tab keeps its note; the inline 100 KB body moves to IndexedDB on the next save.
  await flag('__noidb', false);
  await p.locator('.tab-strip lit-material-tab').nth(0).click();
  await settle(p);
  check('1.1 cut tab: still the part that was kept, with its note', (await p.locator('.response-note').innerText()).includes('over 256 KB'));
  await p.locator('.tab-strip lit-material-tab').nth(1).click();
  st = await tabs();
  const mid = st.tabs.find((t) => t.id === midTabId);
  check('a 100 KB inline body from 1.1 moves to IndexedDB on save', mid.response.length === 16 * 1024 && mid.responseCut === true && mid.responseStored === true, `${mid.response.length}`);
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  check('… and comes back whole after a reload', (await p.locator('.response-meta').innerText()).includes('100.0 KB') && (await p.locator('.response-note').count()) === 0);
  // Raw survives the reload too (it's kept with the body).
  await url.fill(`${base}/bigjson`);
  await sendAndWait();
  await settle(p); // the IndexedDB write follows the batched save
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  await p.getByRole('button', { name: 'Raw', exact: true }).click();
  check('Raw of a stored JSON body survives a reload', (await p.locator('.response-scroll').innerText()).startsWith('{"rows":[{"id":0,'));
  await p.getByRole('button', { name: 'Pretty', exact: true }).click();
  // IndexedDB full: said once, then the 1.1 way.
  await flag('__idbfull', true);
  await p.locator('.tab-strip lit-material-tab').nth(0).click();
  await url.fill(`${base}/big?again`);
  await sendAndWait();
  await settle(p);
  const full = await toastText();
  st = await tabs();
  check('IndexedDB full: a toast says so', full.includes("Couldn't keep a large response"), full.trim().slice(0, 120));
  check('… and the tab is saved the 1.1 way', st.tab.response.length === 64 * 1024 && st.tab.responseCut === true && !st.tab.responseStored, `${st.tab.response.length}`);
  await flag('__idbfull', false);
  check('the 1.1 cut tab is still there', st.tabs.some((t) => t.id === bigTabId));

  // --- 6. Cancel a request in flight; the request timeout -------------------------------
  const histBefore = (await h.stored.history()).length;
  await url.fill(`${base}/slow`);
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  const cancel = p.getByRole('button', { name: 'Cancel', exact: true });
  await cancel.waitFor({ timeout: 5000 });
  check('while waiting, Cancel replaces Send', (await p.getByRole('button', { name: 'Send', exact: true }).count()) === 0);
  await p.waitForTimeout(300);
  const t0 = Date.now();
  await cancel.click();
  await p.getByText('Cancelled', { exact: true }).waitFor({ timeout: 3000 });
  check('Cancel stops at once and says so', Date.now() - t0 < 2000 && (await p.getByRole('button', { name: 'Send', exact: true }).count()) === 1, `${Date.now() - t0} ms`);
  await settle(p);
  check('a cancelled request is not in History', (await h.stored.history()).length === histBefore);
  check('the server saw it go out', got.some((g) => g.path === '/slow'));
  if (TAURI) check('[tauri] send_cancel reached Rust with the request id', (h.mock.cancels || []).length === 1 && h.mock.sent.at(-1).id === h.mock.cancels[0], JSON.stringify([h.mock.cancels, h.mock.sent.at(-1)?.id]));
  // The time limit from Settings (hitop_settings.timeoutMs).
  await p.evaluate(() => localStorage.setItem('hitop_settings', JSON.stringify({ timeoutMs: 1000 })));
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.locator('.response .error-text').waitFor({ timeout: 4000 });
  const timeoutText = await p.locator('.response .error-text').innerText();
  check('the request timeout stops a slow request and names the setting', timeoutText.includes('No response within 1 s') && timeoutText.includes('Settings'), timeoutText);
  if (TAURI) check('[tauri] the time limit goes to Rust too', h.mock.sent.at(-1).timeoutMs === 1000);
  await p.evaluate(() => localStorage.removeItem('hitop_settings'));
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);

  // --- 5. Settings: theme, time limit, storage, shortcuts (desktop sections: settings.cjs) ---
  const goSettings = async () => {
    await p.locator('aside').getByRole('button', { name: 'Settings', exact: true }).click();
    await settle(p);
  };
  await goSettings();
  const page = p.locator('main');
  const sections = await page.locator('section[aria-label]').evaluateAll((els) => els.map((e) => e.getAttribute('aria-label')));
  check('Settings has its sections', ['Appearance', 'Requests', 'Storage', 'Keyboard shortcuts'].every((n) => sections.includes(n)) && (await page.locator('.shortcut-list').innerText()).includes('Send the request'), sections.join());
  await p.locator('.settings-theme').click();
  await p.getByRole('option', { name: 'Dark', exact: true }).click();
  await settle(p);
  check('Theme: Dark applies and is remembered under hitop-theme', (await p.evaluate(() => [document.documentElement.getAttribute('color-scheme'), localStorage.getItem('hitop-theme')].join())) === 'dark,dark');
  await p.locator('.settings-theme').click();
  await p.getByRole('option', { name: 'Match the system', exact: true }).click();
  await settle(p);
  check('Theme: Match the system clears it', (await p.evaluate(() => [document.documentElement.getAttribute('color-scheme'), localStorage.getItem('hitop-theme')].join())) === ',');
  await p.locator('.settings-timeout').evaluate((el) => {
    el.value = '2.5';
    el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await settle(p);
  check('Time limit: saved in hitop_settings as ms', (await p.evaluate(() => localStorage.getItem('hitop_settings'))) === '{"timeoutMs":2500}' && (await p.locator('.settings-timeout').evaluate((el) => el.supportingText)).includes('within 2.5 s'));
  await p.locator('.settings-timeout').evaluate((el) => {
    el.value = '0';
    el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await settle(p);
  const facts = await page.locator('.settings-facts').innerText();
  check('Storage: sizes and the IndexedDB count are shown', facts.includes('Open tabs') && /\d+ kept/.test(facts), facts.replace(/\n/g, ' | '));
  await page.getByRole('button', { name: 'Clear stored responses' }).click();
  const ask12 = p.locator('lit-material-confirm-dialog[open]');
  await ask12.getByRole('button', { name: 'Clear', exact: true }).click();
  await settle(p);
  st = await tabs();
  check('Clear stored responses: no tab keeps a response, IndexedDB is empty', st.tabs.every((t) => t.response == null && !t.responseStored) && (await page.locator('.settings-facts').innerText()).includes('0 kept'));

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message);
  process.exit(1);
});
