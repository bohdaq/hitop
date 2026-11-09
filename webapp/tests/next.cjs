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
  await p.locator('aside .tree-row.leaf', { hasText: 'New request' }).first().click();
  await settle(p);
  check('title bar names the collection on screen', (await p.locator('header.topbar .crumb-name').innerText()).trim().length > 0);
  check('status bar says where the request is', (await p.locator('footer.statusbar').innerText()).includes('›') || (await p.locator('footer.statusbar').innerText()).includes('Unsaved'));

  // Tab reorder: two tabs, move the second before the first with the keyboard.
  await p.locator('hitop-request .request-line .url input, hitop-request .request-line .url').first().click();
  await p.keyboard.type(`${base}/one`);
  await p.locator('.tab-strip').getByRole('button', { name: 'New tab' }).click();
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
  check('the dirty tab shows unsaved in the status bar', (await p.locator('footer.statusbar').innerText()).includes('Unsaved'));

  // --- A4 + B3: variables in the URL, the response line, the timing waterfall, Diff vs last ---
  await p.locator('.tab-strip').getByRole('button', { name: 'New tab' }).click();
  await settle(p);
  const url = p.locator('hitop-request .request-line .url');
  await url.click();
  await p.keyboard.type(`${base}/one?id=\${orderId}`);
  await settle(p);
  const chip = p.locator('.url-vars .var-chip');
  check('the URL\'s variables show as chips; an undefined one is marked', (await chip.count()) === 1 && (await chip.innerText()).trim() === '${orderId}' && (await chip.getAttribute('class')).includes('missing'));
  await chip.click();
  await settle(p);
  check('a click on a variable says where it comes from', (await p.locator('lit-material-popover.var-peek').evaluate((el) => el.open)) && (await p.locator('.var-peek').innerText()).includes('Nothing defines it'));
  await p.keyboard.press('Escape');
  await url.click();
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
  await p.keyboard.press('Control+Enter');
  await settle(p);
  await p.locator('.response-meta .status-chip').waitFor();
  const respTabs = p.locator('.response lit-material-tabs[aria-label="Response details"] lit-material-tab');
  check('a second response offers Diff vs last', (await respTabs.allInnerTexts()).some((t) => t.trim() === 'Diff vs last'));
  await respTabs.filter({ hasText: 'Diff vs last' }).click();
  await settle(p);
  check('Diff vs last compares in the response pane', (await p.locator('.response .compare-summary').count()) === 1 && (await p.locator('.response #diff-left').count()) === 1, await p.locator('.response .compare-summary').innerText().catch(() => ''));

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
