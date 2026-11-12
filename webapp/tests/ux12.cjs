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
    await p.locator('.response-meta, .response .error-card').first().waitFor({ timeout: 15000 });
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
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
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
  await p.locator('.response .error-card').waitFor({ timeout: 4000 });
  const timeoutText = await p.locator('.response .error-card').innerText();
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
  check('Settings has its sections', ['Requests', 'Storage', 'Keyboard shortcuts'].every((n) => sections.includes(n)) && (await page.locator('.shortcut-list').innerText()).includes('Send the request'), sections.join());
  // Dark only (owner decision 2026-11-09): no theme choice anywhere, and a light system stays dark.
  check('Dark only: no Appearance section, no theme select', !sections.includes('Appearance') && (await p.locator('.settings-theme').count()) === 0, sections.join());
  await p.emulateMedia({ colorScheme: 'light' });
  check('Dark only: dark even when the system asks for light', (await p.evaluate(() => [document.documentElement.getAttribute('color-scheme'), getComputedStyle(document.documentElement).colorScheme].join())) === 'dark,dark');
  await p.emulateMedia({ colorScheme: null });
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

  // --- 2. Import adds by default (a name in use gets " (2)"); Replace all asks (ux.cjs) ------
  const colsBefore = await h.stored.collections();
  const exportJson = JSON.stringify([
    { id: colsBefore[0].id, name: colsBefore[0].name, requests: [{ id: 'r-imp', name: 'Imported one', url: `${base}/json`, method: 'GET', headers: [], body: '' }] },
    { id: 'imp-2', name: 'Shop', requests: [{ id: 'r-shop', name: 'List orders', url: `${base}/json`, method: 'GET', headers: [], body: '', file: 'orders/list' }, { id: 'r-me', name: 'Me', url: `${base}/json`, method: 'GET', headers: [], body: '', file: 'account/me' }, { id: 'r-top', name: 'Health', url: `${base}/json`, method: 'GET', headers: [], body: '' }] },
  ]);
  await p.locator('aside').getByRole('button', { name: 'Import', exact: true }).click();
  await p.locator('#dialog #json').waitFor();
  check('Import: "Add to my collections" is the default', await p.locator('#dialog').getByRole('radio', { name: /Add to my collections/ }).isChecked());
  await p.locator('#dialog #json').evaluate((el, text) => (el.value = text), exportJson);
  await p.locator('#dialog').getByRole('button', { name: 'Import', exact: true }).click();
  await settle(p);
  const colsAfter = await h.stored.collections();
  check('Import adds, no confirm; a clashing name gets (2)', (await p.locator('lit-material-confirm-dialog[open]').count()) === 0 && colsAfter.length === colsBefore.length + 2 && colsAfter.some((c) => c.name === `${colsBefore[0].name} (2)`) && colsAfter.some((c) => c.name === 'Shop'), colsAfter.map((c) => c.name).join());
  check('… existing collections and tabs stay; the first imported request opens', colsAfter[0].requests.length === colsBefore[0].requests.length && (await p.locator('.tab-strip lit-material-tab[selected]').innerText()).includes('Imported one'));

  // --- 3. Split panes: side by side at 1280, a keyboard and pointer handle, remembered; fold --
  await sendAndWait();
  const sep = p.getByRole('separator', { name: 'Resize the request and response panes' });
  const ratio = () => p.evaluate(() => document.querySelector('.pane-editor').getBoundingClientRect().width / document.querySelector('.panes').getBoundingClientRect().width);
  check('wide: editor and response side by side, split 50%', (await p.locator('.panes.columns').count()) === 1 && (await sep.getAttribute('aria-valuenow')) === '50' && (await sep.getAttribute('aria-orientation')) === 'vertical');
  await sep.focus();
  await p.keyboard.press('ArrowRight');
  await p.keyboard.press('ArrowRight');
  await settle(p);
  const layout = () => p.evaluate(() => JSON.parse(localStorage.getItem('hitop_layout') || 'null'));
  check('arrow keys move the split; it is remembered', (await sep.getAttribute('aria-valuenow')) === '60' && (await layout())?.columns === 60 && Math.abs((await ratio()) - 0.6) < 0.03, String(await ratio()));
  const box = await p.locator('.panes').boundingBox();
  const hb = await sep.boundingBox();
  await p.mouse.move(hb.x + hb.width / 2, hb.y + 40);
  await p.mouse.down();
  await p.mouse.move(box.x + box.width * 0.4, hb.y + 40, { steps: 5 });
  await p.mouse.up();
  await settle(p);
  check('dragging the handle moves the split', Math.abs(Number(await sep.getAttribute('aria-valuenow')) - 40) <= 2 && (await layout()).columns === Number(await sep.getAttribute('aria-valuenow')), await sep.getAttribute('aria-valuenow'));
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  check('… and kept after a reload', Math.abs((await ratio()) - (await layout()).columns / 100) < 0.03);
  await p.getByRole('button', { name: 'Hide the response' }).click();
  await settle(p);
  const bar = await p.locator('.folded-bar').innerText();
  check('the response folds away; its bar keeps the status', (await layout()).responseCollapsed === true && (await p.locator('.response-meta').count()) === 0 && bar.includes('200'), bar.replace(/\n/g, ' '));
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  check('… folded after a reload too', (await p.locator('.folded-bar').count()) === 1);
  await p.getByRole('button', { name: 'Show response' }).click();
  await settle(p);
  check('Show response brings it back', (await p.locator('.response-meta').count()) === 1 && (await layout()).responseCollapsed === false);
  // Narrower: stacked, the handle sets the editor's height (with the side pane shown: at 1200 px or
  // less it folds by itself unless chosen, UX2).
  await p.evaluate(() => (document.querySelector('hitop-app').sidePref = true));
  await p.setViewportSize({ width: 1024, height: 768 });
  await settle(p);
  const editorHeight = () => p.evaluate(() => Math.round(document.querySelector('.pane-editor').getBoundingClientRect().height));
  const h0 = await editorHeight();
  check('narrower: stacked with a horizontal handle', (await p.locator('.panes.rows').count()) === 1 && (await sep.getAttribute('aria-orientation')) === 'horizontal' && h0 === 280, String(h0));
  await sep.focus();
  await p.keyboard.press('ArrowDown');
  await settle(p);
  check('ArrowDown makes the editor taller', (await editorHeight()) === 320 && (await layout()).rows === 320, String(await editorHeight()));
  await p.keyboard.press('Enter');
  await settle(p);
  // Phone: plain stacked, no handle, no sideways scroll.
  await p.setViewportSize({ width: 390, height: 844 });
  await settle(p);
  check('phone: stacked, no handle, no horizontal scroll', (await p.locator('.panes.stack').count()) === 1 && (await sep.count()) === 0 && !(await p.evaluate(() => document.documentElement.scrollWidth > innerWidth)));
  await p.evaluate(() => (document.querySelector('hitop-app').sidePref = undefined));
  await p.setViewportSize({ width: 1280, height: 860 });
  await settle(p);

  // --- 4. Mod+P quick open (never the browser's Print inside the app); Mod+L goes to the URL (Mod+K is the palette) ---
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control';
  await p.evaluate(() => window.addEventListener('keydown', (e) => e.code === 'KeyP' && (window.__lastP = e), true));
  await p.locator('.response-meta .status-chip').click();
  await p.keyboard.press(`${mod}+p`);
  const quick = p.getByRole('combobox', { name: 'Find a request' });
  await quick.waitFor({ timeout: 5000 });
  check('Mod+P opens quick open, not Print', (await p.evaluate(() => window.__lastP?.defaultPrevented)) === true && (await p.getByRole('option').count()) > 3);
  await p.keyboard.type('lstord', { delay: 20 });
  await settle(p);
  const firstOption = await p.getByRole('option').first().innerText();
  check('fuzzy: "lstord" finds List orders, with its collection and folder', firstOption.includes('List orders') && firstOption.includes('Shop › orders'), firstOption.replace(/\n/g, ' '));
  await p.keyboard.press('Enter');
  await settle(p);
  check('Enter opens it in a tab', (await p.locator('.tab-strip lit-material-tab[selected]').innerText()).includes('List orders') && (await p.locator('#dialog #quick-input').count()) === 0);
  await p.keyboard.press(`${mod}+p`);
  await quick.waitFor({ timeout: 5000 });
  await p.keyboard.type('health', { delay: 20 });
  await p.keyboard.press('ArrowDown');
  await p.keyboard.press('ArrowUp');
  const activeId = await quick.getAttribute('aria-activedescendant');
  check('arrow keys move the active option (aria-activedescendant)', activeId === 'quick-opt-0');
  await p.keyboard.press('Escape');
  await settle(p);
  await p.locator('.pane-title').first().click(); // not typing: ? is a key then
  await p.keyboard.press('Shift+Slash');
  await p.locator('#dialog .shortcut-list').waitFor({ timeout: 5000 });
  const list = await p.locator('#dialog .shortcut-list').innerText();
  check('the shortcuts list has Mod+P, Mod+K and Mod+L', list.includes('Quick open') && list.includes('Command palette') && list.includes('Go to the URL field'));
  await p.evaluate(() => (window.__lastP = null));
  await p.keyboard.press(`${mod}+p`);
  check('Mod+P with another dialog open still never prints', (await p.evaluate(() => window.__lastP?.defaultPrevented)) === true && (await p.locator('#dialog .shortcut-list').count()) === 1);
  await p.keyboard.press('Escape');
  await settle(p);
  await p.locator('.pane-title').first().click();
  await p.keyboard.press(`${mod}+l`);
  await settle(p);
  check('Mod+L goes to the URL field', await p.evaluate(() => document.activeElement?.closest?.('.url') != null || document.activeElement?.classList.contains('url')));

  // --- 7. Folder groups, the request context menu, tab close buttons, the History page --------
  const aside = p.locator('aside');
  const shopId = (await h.stored.collections()).find((c) => c.name === 'Shop').id;
  const folders = await aside.locator('.tree-row.folder').allInnerTexts();
  check('sidebar: Postman/OpenAPI folders are groups', folders.map((f) => f.trim()).join() === 'orders,account', folders.join());
  const ordersFolder = aside.locator('.tree-row.folder', { hasText: 'orders' });
  await ordersFolder.click();
  await settle(p);
  check('a folder folds (remembered)', (await ordersFolder.getAttribute('aria-expanded')) === 'false' && (await aside.getByRole('treeitem', { name: /List orders/ }).count()) === 0 && (await p.evaluate(() => localStorage.getItem('hitop_collapsed_collections'))).includes(`${shopId}/orders`));
  await ordersFolder.click();
  await settle(p);
  const health = () => aside.locator('.tree-row.leaf', { hasText: /^\s*GET\s*Health\s*$/ });
  await health().click({ button: 'right' });
  const menu = p.locator('#request-menu');
  await menu.locator('lit-material-list-item', { hasText: /^\s*Rename/ }).click();
  await p.locator('#dialog #name').waitFor();
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'Health check'));
  await p.locator('#dialog').getByRole('button', { name: 'Rename' }).click();
  await settle(p);
  let shop = (await h.stored.collections()).find((c) => c.name === 'Shop');
  check('context menu: Rename', shop.requests.some((r) => r.name === 'Health check') && !shop.requests.some((r) => r.name === 'Health'));
  await aside.locator('.tree-row.leaf', { hasText: 'Health check' }).click({ button: 'right' });
  await menu.getByText('Duplicate', { exact: true }).click();
  await settle(p);
  shop = (await h.stored.collections()).find((c) => c.name === 'Shop');
  const at = shop.requests.findIndex((r) => r.name === 'Health check');
  check('context menu: Duplicate, right after it, opened', shop.requests[at + 1]?.name === 'Health check (Copy)' && (await p.locator('.tab-strip lit-material-tab[selected]').innerText()).includes('Health check (Copy)'));
  const meRow = aside.locator('.tree-row.leaf', { hasText: /^\s*GET\s*Me\s*$/ });
  await meRow.click({ button: 'right' });
  await menu.getByText('Duplicate', { exact: true }).click();
  await settle(p);
  shop = (await h.stored.collections()).find((c) => c.name === 'Shop');
  check('… a request in a folder stays in its folder', shop.requests.find((r) => r.name === 'Me (Copy)')?.file === 'account/me-copy');
  await aside.locator('.tree-row.leaf', { hasText: 'Health check (Copy)' }).click({ button: 'right' });
  await menu.locator('lit-material-list-item', { hasText: /^\s*Delete…/ }).click();
  await p.locator('lit-material-confirm-dialog[open]').getByRole('button', { name: 'Delete', exact: true }).click();
  await settle(p);
  shop = (await h.stored.collections()).find((c) => c.name === 'Shop');
  check('context menu: Delete (asks first)', !shop.requests.some((r) => r.name === 'Health check (Copy)'));

  // Close "×" on a tab.
  const tabCount = async () => (await h.stored.tabs()).tabs.length;
  const nTabs = await tabCount();
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  const lastTab = p.locator('.tab-strip lit-material-tab').last();
  await lastTab.hover();
  await lastTab.locator('.tab-close').click();
  await settle(p);
  check('the × on a tab closes it', (await tabCount()) === nTabs);
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await url.fill(`${base}/json`);
  await settle(p);
  await p.locator('.tab-strip lit-material-tab').last().locator('.tab-close').click();
  await p.locator('lit-material-confirm-dialog[open]').getByRole('button', { name: 'Cancel' }).click();
  await settle(p);
  check('… and asks first when the tab has unsaved content', (await tabCount()) === nTabs + 1);
  check('the × is hidden from screen readers (keyboard: Alt+Shift+W)', (await p.locator('.tab-close').first().getAttribute('aria-hidden')) === 'true');

  // History as a page.
  await sendAndWait();
  await aside.getByRole('button', { name: 'History', exact: true }).click();
  await settle(p);
  const histPage = p.locator('main');
  check('History is a page, marked current in the sidebar', (await histPage.locator('h1').innerText()) === 'History' && (await aside.getByRole('button', { name: 'History', exact: true }).getAttribute('aria-current')) === 'page' && (await histPage.locator('.history-day h2').first().innerText()) === 'Today');
  const rowsAll = await histPage.locator('.history-row').count();
  await histPage.getByLabel('Search history').fill('mid');
  const rowsMid = await histPage.locator('.history-row').count();
  check('History search filters', rowsMid >= 1 && rowsMid < rowsAll, `${rowsMid}/${rowsAll}`);
  await histPage.getByLabel('Search history').fill('');
  const hBefore = (await h.stored.history()).length;
  await histPage.locator('.history-row').first().getByRole('button', { name: /^Delete / }).click();
  await settle(p);
  check('History: delete one entry', (await h.stored.history()).length === hBefore - 1);
  await histPage.locator('.history-item').first().click();
  await settle(p);
  check('History: an entry opens in the request view', (await p.locator('hitop-request').count()) === 1);

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message);
  process.exit(1);
});
