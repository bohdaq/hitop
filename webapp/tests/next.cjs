// HITOP Next (HITOP_NEXT_UI.md): the app shell (rail, title bar, status bar, tab reorder), in web
// and Tauri mode, against a local Node server.
//   BASE=http://localhost:7879 node tests/next.cjs
//   HITOP_MODE=tauri node tests/next.cjs
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

function api() {
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    if (req.url.startsWith('/fail')) return res.writeHead(409, { ...cors, 'Content-Type': 'application/json' }).end('{"error":"already_paid"}');
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end('{"ok":true}');
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, base: `http://127.0.0.1:${server.address().port}` })));
}

(async () => {
  const { server, base } = await api();
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await settle(p);

  // --- A3 shell ---
  const rail = p.locator('nav[aria-label="Sections"]');
  const items = await rail.locator('lit-material-navigation-rail-item').allInnerTexts();
  check('rail lists the sections', ['Collections', 'History', 'Environments', 'Cookies', 'Settings'].every((n) => items.some((t) => t.trim() === n)), items.join());
  check(TAURI ? '[tauri] the rail has Git' : '[web] no Git in the rail', items.some((t) => t.trim() === 'Git') === TAURI, items.join());
  await rail.getByRole('button', { name: 'History', exact: true }).click();
  await settle(p);
  check('the rail opens History and marks it current', (await p.locator('main h1').first().innerText()) === 'History' && (await rail.getByRole('button', { name: 'History', exact: true }).getAttribute('aria-current')) === 'page');
  await rail.getByRole('button', { name: 'Collections', exact: true }).click();
  await settle(p);
  check('Collections opens the collections page', (await p.locator('hitop-collections').count()) === 1);

  check('title bar has a search field', (await p.locator('header.topbar .command-field').isVisible()));
  await p.locator('aside .tree-row.leaf', { hasText: 'List posts' }).first().click();
  await settle(p);
  check('title bar names the collection on screen', (await p.locator('header.topbar .crumb-name').innerText()).trim() === 'Sample: JSONPlaceholder');
  const statusText = await p.locator('.statusbar').innerText();
  check('status bar has the save state, not the collection and request again (FV1)', statusText.includes('Saved') && !statusText.includes('›') && !statusText.includes('Sample: JSONPlaceholder'), statusText.replace(/\n/g, ' | '));

  // Tab reorder: two tabs, move the second before the first with the keyboard.
  await p.locator('hitop-request .request-line .url input, hitop-request .request-line .url').first().click();
  await p.keyboard.type(`${base}/one`);
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  const titles = () => p.locator('.tab-strip lit-material-tab').allInnerTexts();
  const before = (await titles()).map((t) => t.trim());
  await p.locator('.tab-strip lit-material-tab').nth(1).focus();
  await p.keyboard.press('Alt+Shift+ArrowLeft');
  await settle(p);
  const after = (await titles()).map((t) => t.trim());
  check('Alt+Shift+Arrow reorders tabs, the open one stays open', after.length === 2 && after[0] === before[1] && after[1] === before[0] && (await h.stored.tabs()).current === 0, `${before} → ${after}`);
  await p.keyboard.press('Alt+Shift+KeyW');
  await settle(p);
  check('the dirty tab shows unsaved in the status bar', (await p.locator('.statusbar').innerText()).includes('Unsaved'));

  // --- A4 + B3: variables in the URL, the response line, the timing waterfall, Diff vs last ---
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  const url = p.locator('hitop-request .request-line .url');
  await url.click();
  await p.keyboard.type(`${base}/one?id=\${orderId}`);
  await settle(p);
  const chip = p.locator('.url-vars .var-chip');
  check('the URL\'s variables show as chips; an undefined one is marked', (await chip.count()) === 1 && (await chip.innerText()).trim() === '${orderId}' && (await chip.getAttribute('class')).includes('missing'));
  // The pointer away from where the chip appeared under it (hovering already peeks), then a click.
  await p.mouse.move(5, 500);
  await settle(p);
  // (A click can land while the hover's peek is still opening, which light dismiss then closes:
  // the click is the person's second try, as by hand.)
  for (let i = 0; i < 2 && !(await p.locator('lit-material-popover.var-peek').evaluate((el) => el.open)); i++) {
    await chip.click();
    await p.waitForFunction(() => document.querySelector('lit-material-popover.var-peek')?.open, null, { timeout: 2000 }).catch(() => {});
  }
  await settle(p);
  check('a click on a variable says where it comes from', (await p.locator('lit-material-popover.var-peek').evaluate((el) => el.open)) && (await p.locator('.var-peek').innerText()).includes('Nothing defines it'));
  await p.keyboard.press('Escape');
  await p.keyboard.press('Control+KeyL'); // the URL field (a click on it would land on the chip)
  await p.keyboard.press('End');
  for (let i = 0; i < '?id=${orderId}'.length; i++) await p.keyboard.press('Backspace');
  await p.keyboard.press('Control+Enter');
  await p.locator('.response-meta .status-chip').waitFor();
  const waterfall = p.locator('.response lit-material-timing-waterfall');
  check('one response: no Diff vs last yet', !(await p.locator('.response lit-material-tabs[aria-label="Response details"] lit-material-tab').allInnerTexts()).some((t) => t.trim() === 'Diff vs last'));
  if (TAURI) {
    check('[tauri] the response shows where the time went', (await waterfall.count()) === 1 && (await waterfall.evaluate((el) => el.timings?.totalMs >= 0)));
  } else {
    check('[web] localhost goes straight from the browser: no phases, no waterfall', (await waterfall.count()) === 0);
  }
  // The HTTP version of the response: after the URL and in the status bar, only when the engine said.
  const urlHint = (await p.locator('hitop-request .url-bar .url-hint').innerText().catch(() => '')).trim();
  const bar = await p.locator('.statusbar').innerText();
  if (TAURI) {
    check('[tauri] the URL bar shows the response\'s HTTP version', urlHint === 'HTTP/1.1', urlHint);
    check('[tauri] the status bar says scheme · version', bar.includes('HTTP · HTTP/1.1') && !/TLS/.test(bar), bar.replace(/\n/g, ' | '));
    await settle(p); // saves are batched (250 ms)
    const kept = (await h.stored.tabs()).tab;
    check('[tauri] the tab keeps the version and scheme', kept?.httpVersion === 'HTTP/1.1' && kept?.responseScheme === 'http', JSON.stringify({ v: kept?.httpVersion, s: kept?.responseScheme }));
  } else {
    check('[web] a direct fetch can\'t see the version: the hint stays the scheme', urlHint === 'HTTP', urlHint);
    check('[web] no version in the status bar', !/HTTP\/1\.1|\bh2\b|\bh3\b|TLS/.test(bar), bar.replace(/\n/g, ' | '));
  }
  await p.keyboard.press('Control+Enter');
  await settle(p);
  await p.locator('.response-meta .status-chip').waitFor();
  const respTabs = p.locator('.response lit-material-tabs[aria-label="Response details"] lit-material-tab');
  check('a second response offers Diff vs last', (await respTabs.allInnerTexts()).some((t) => t.trim() === 'Diff vs last'));
  await respTabs.filter({ hasText: 'Diff vs last' }).click();
  await settle(p);
  check('Diff vs last compares in the response pane', (await p.locator('.response .compare-summary').count()) === 1 && (await p.locator('.response #diff-left').count()) === 1, await p.locator('.response .compare-summary').innerText().catch(() => ''));

  // --- The Body tab's facts line (Main board): kind, JSON valid or where it breaks ---
  const setTab = (changes) => p.evaluate((c) => document.querySelector('hitop-request').set(c), changes);
  await setTab({ method: 'POST', requestBody: '{\n  "sku": "A-1",\n  "qty": ${qty}\n}' });
  await settle(p);
  await p.locator('hitop-request section.editor').getByRole('tab', { name: 'Body', exact: true }).click();
  await settle(p);
  const facts = async () => (await p.locator('hitop-request .body-facts').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();
  check('Raw JSON body: kind and Valid JSON ✓ (${variables} count as values)', (await facts()) === 'JSON Valid JSON ✓', await facts());
  check('the body type control keeps its name and options', (await p.locator('hitop-request').getByRole('combobox', { name: 'Body type' }).count()) === 1);
  const edBefore = await p.locator('hitop-request .cm-editor').boundingBox();
  await setTab({ requestBody: '{\n  "sku": "A-1"\n  "qty": 2\n}' });
  await settle(p);
  const edAfter = await p.locator('hitop-request .cm-editor').boundingBox();
  check('a JSON error says line and column', (await facts()) === 'JSON JSON error · line 3, col 3', await facts());
  check('the line doesn\'t move the editor when validity changes', edBefore && edAfter && edBefore.y === edAfter.y, `${edBefore?.y} → ${edAfter?.y}`);
  await setTab({ requestBody: '<order id="1"/>' });
  await settle(p);
  check('XML is named, no JSON verdict', (await facts()) === 'XML', await facts());
  await setTab({ method: 'GET', requestBody: '' });
  await settle(p);

  // --- A5: the command palette (Mod+K) ---
  const mod = process.platform === 'darwin' ? 'Meta' : 'Control';
  const palette = p.locator('lit-material-command-palette');
  const box = palette.getByRole('combobox');
  await p.locator('.pane-title').first().click();
  await p.keyboard.press(`${mod}+k`);
  await box.waitFor({ timeout: 5000 });
  check('Mod+K opens the command palette with focus in its field', (await palette.evaluate((el) => el.open)) && (await box.evaluate((el) => el.matches(':focus'))));
  const sections = await palette.evaluate((el) => [...new Set(el.items.map((i) => i.section))]);
  check('the palette has requests, actions and environments', ['Requests', 'Actions', 'Environments'].every((x) => sections.includes(x)), sections.join());
  await p.keyboard.type('>histo', { delay: 20 });
  await settle(p);
  const opts = await palette.getByRole('option').allInnerTexts();
  check('">" limits it to actions; fuzzy finds Open history', opts.length >= 1 && opts[0].includes('Open history') && !opts.some((o) => o.includes('New request') && !o.includes('Actions')), opts.join(' | '));
  await p.keyboard.press('Enter');
  await settle(p);
  check('Enter runs the action and closes the palette', (await p.locator('main h1').first().innerText()) === 'History' && !(await palette.evaluate((el) => el.open)));
  await p.keyboard.press(`${mod}+k`);
  await box.waitFor({ timeout: 5000 });
  await p.keyboard.press('Escape');
  await settle(p);
  check('Escape closes it', !(await palette.evaluate((el) => el.open)));
  await p.keyboard.press(`${mod}+k`);
  await box.waitFor({ timeout: 5000 });
  await p.keyboard.type('a GET request to list orders', { delay: 5 });
  await p.keyboard.press('Tab');
  await settle(p);
  check('Tab hands the query to the agent', !(await palette.evaluate((el) => el.open)) && (await p.locator('#agent').evaluate((el) => el.open)) && (await p.evaluate(() => { const a = document.querySelector('hitop-app').agent; return a?.action === 'generate' && a.prompt; })) === 'a GET request to list orders');
  await p.keyboard.press('Escape');
  await settle(p);
  await settle(p);
  await p.locator('header.topbar .command-field').click();
  await box.waitFor({ timeout: 5000 });
  check('the title bar search field opens the palette', await palette.evaluate((el) => el.open));
  const rich = await palette.evaluate((el) => {
    const r = el.shadowRoot;
    const opt = [...r.querySelectorAll('[role="option"]')].find((o) => o.querySelector('lit-material-method-tag'));
    opt?.dispatchEvent(new MouseEvent('mousemove', { bubbles: true }));
    return { tags: r.querySelectorAll('lit-material-method-tag').length, label: opt?.querySelector('.label')?.textContent ?? '' };
  });
  await settle(p);
  const pane = await palette.evaluate((el) => el.shadowRoot.querySelector('[part="preview"]')?.textContent ?? '');
  const foot = await palette.evaluate((el) => el.shadowRoot.querySelector('[part="footer"]')?.textContent ?? '');
  check('requests show a method tag (not the method in the label)', rich.tags > 0 && !/^(GET|POST|PUT|PATCH|DELETE) /.test(rich.label), JSON.stringify(rich));
  check('the active request shows a preview with its URL', pane.length > 0 && /https?:|\$\{|\//.test(pane), pane.slice(0, 120));
  check('the footer offers Tab to ask AI', foot.includes('ask AI'), foot);
  await p.keyboard.press('Escape');
  await settle(p);
  await p.locator('.pane-title').first().click().catch(() => {});
  await p.keyboard.press(`${mod}+l`);
  await settle(p);
  check('Mod+L goes to the URL field', await p.evaluate(() => document.activeElement?.closest?.('.url') != null || document.activeElement?.classList.contains('url')));

  // --- A6: the run report ---
  await p.evaluate((b) => {
    const app = document.querySelector('hitop-app');
    const s = app.store;
    const req = (id, name, method, url, postRequestScript = '') => ({ id, name, method, url, headers: [], body: '', preRequestScript: '', postRequestScript });
    s.collections = [...s.collections, { id: 991, name: 'Run report', variables: {}, requests: [
      req(9911, 'List orders', 'GET', `${b}/one`, 'pm.test("status is 200", () => pm.response.to.have.status(200));'),
      req(9912, 'Cancel order', 'DELETE', `${b}/fail`, 'pm.test("status is 204", () => pm.response.to.have.status(204));'),
    ] }];
    s.emit();
    app.go('collection', 991);
  }, base);
  await settle(p);
  await p.getByRole('button', { name: 'Run collection' }).click();
  const report = p.locator('hitop-run-report');
  await report.locator('.run-tiles').waitFor({ timeout: 10000 });
  const tiles = await report.locator('.run-tile').allInnerTexts();
  check('run report: tiles for requests, checks, p95 and wall time', tiles.length === 4 && tiles[0].includes('1/2') && tiles[1].includes('1/2') && tiles[2].includes('first run here'), tiles.join(' | '));
  check('run report: the failure card says expected vs actual', (await report.locator('.run-failure').innerText()).includes('status is 204') && (await report.locator('.run-failure').innerText()).includes('Actual'));
  check('run report: the hitop run line for the run', (await report.locator('.run-command').innerText()).startsWith("hitop run 'Run report'") && (await report.locator('.run-command').innerText()).includes('--reporter cli,junit,json'));
  await report.getByRole('radio', { name: /Failed/ }).click();
  await settle(p);
  const failedRows = await report.locator('.run-list > li.run-row').allInnerTexts();
  check('run report: Failed shows only the failed request', failedRows.length === 1 && failedRows[0].includes('Cancel order'), failedRows.join(' | '));
  await report.getByRole('radio', { name: /All/ }).click();
  await p.getByRole('button', { name: 'Run collection' }).click();
  await settle(p);
  await report.locator('.run-tiles').waitFor({ timeout: 10000 });
  await settle(p);
  check('run report: a second run compares p95 with the last one', /vs last run|same as last run/.test(await report.locator('.run-tile').nth(2).innerText()));
  const history = await p.evaluate(() => JSON.parse(localStorage.getItem('hitop_run_history') || '{}')['991'] || []);
  check('run history keeps summaries only', history.length === 2 && !JSON.stringify(history).includes('/fail'), JSON.stringify(history));

  // Phone: the rail and pane are a drawer; no sideways scroll.
  await p.setViewportSize({ width: 390, height: 844 });
  await settle(p);
  const onScreen = async () => ((await rail.boundingBox())?.x ?? -1) >= 0;
  check('phone: the rail is in the closed drawer', !(await onScreen()));
  await p.getByRole('button', { name: 'Open collections' }).click();
  await p.waitForTimeout(700); // the drawer slides in (500 ms)
  check('phone: the open drawer has the rail', await onScreen());
  check('phone: no horizontal scroll', !(await p.evaluate(() => document.documentElement.scrollWidth > innerWidth)));
  await p.setViewportSize({ width: 1280, height: 860 });
  await p.keyboard.press('Escape');
  await settle(p);

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message);
  process.exit(1);
});
