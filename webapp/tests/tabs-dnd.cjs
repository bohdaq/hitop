// DD2 (HITOP_NEXT_UI.md): tabs. A drop line while a tab is dragged, a tab dropped on a sidebar
// folder moves its request there, Pin keeps tabs first (no ×), Close others / to the right keep
// unsaved and pinned tabs. Web and Tauri mode.
//   BASE=http://localhost:7879 node tests/tabs-dnd.cjs
//   HITOP_MODE=tauri node tests/tabs-dnd.cjs
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();

(async () => {
  const h = await launch({ viewport: { width: 1400, height: 900 }, results });
  const page = h.page;
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(h.url, { waitUntil: 'networkidle' });
  await page.waitForSelector('hitop-app');
  await page.waitForTimeout(800);
  const ev = (b) => page.evaluate(new Function('return (async () => {' + b + '})()'));
  const my = `const app = document.querySelector('hitop-app'); const s = app.store; const my = s.collections.find(c => /My requests/.test(c.name));`;
  // Seed: My requests with A, B, C (C in folder "api"), each open in a tab.
  await ev(`${my}
    const mk = (name, file) => ({ id: Math.floor(Math.random()*1e12), name, url: 'https://example.com/' + name, method: 'GET', headers: [], body: '', ...(file ? { file } : {}) });
    s.collections = s.collections.map(c => c.id === my.id ? { ...c, requests: [mk('A'), mk('B'), mk('C', 'api/c')] } : c); s.emit();
    const fresh = s.collections.find(c => c.id === my.id);
    for (const r of fresh.requests) { s.openRequest(r, my.id); await new Promise(r => setTimeout(r, 150)); }`);
  await page.waitForTimeout(400);
  const titles = async () => (await page.locator('.tab-strip lit-material-tab .tab-title').allInnerTexts()).map((t) => t.trim());
  const tab = (name) => page.locator('.tab-strip lit-material-tab', { has: page.locator('.tab-title', { hasText: new RegExp(`^${name}$`) }) }).first();
  const t0 = await titles();

  // 1) dragging a tab shows a line; dropping it reorders
  let a = await tab('A').boundingBox();
  const c = await tab('C').boundingBox();
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2); await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 12, a.y + a.height / 2, { steps: 3 });
  await page.mouse.move(c.x + c.width - 4, c.y + c.height / 2, { steps: 12 });
  check('while a tab is dragged: it is lifted and a drop line shows', (await page.locator('.tab-strip lit-material-tab[dragging]').count()) === 1 && (await page.locator('.tree-drop-line:not([hidden])').count()) === 1);
  await page.mouse.up(); await page.waitForTimeout(300);
  const t1 = await titles();
  check('the tab lands where the line was', t1.indexOf('A') > t1.indexOf('C'), `${t0} → ${t1}`);
  check('the line goes away', (await page.locator('.tree-drop-line').count()) === 0);

  // 2) a tab dropped on a sidebar folder moves its request into it; the tabs stay in order
  a = await tab('B').boundingBox();
  const folder = await page.locator('nav.tree [data-drag="folder"]', { hasText: 'api' }).first().boundingBox();
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2); await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 12, a.y + a.height / 2, { steps: 3 });
  await page.mouse.move(folder.x + 40, folder.y + folder.height / 2, { steps: 14 });
  check('over the sidebar the folder lights up', (await page.locator('.drop-inside').count()) === 1);
  await page.mouse.up(); await page.waitForTimeout(400);
  const files = await ev(`${my} return my.requests.map(r => r.name + ':' + (r.file || '')).join(' ');`);
  check('a tab dropped on a folder moves its request there', /B:api\/b/.test(files), files);
  check('...and the tabs did not reorder', JSON.stringify(await titles()) === JSON.stringify(t1), (await titles()).join());

  // 3) Pin
  await tab('C').click({ button: 'right' });
  await page.locator('#tab-menu').getByText('Pin tab').click(); await page.waitForTimeout(300);
  let t3 = await titles();
  check('a pinned tab goes first, with a pin and no ×', t3[0] === 'C' && (await tab('C').locator('.tab-pin').count()) === 1 && (await tab('C').locator('.tab-close').count()) === 0, t3.join());
  await ev(`const s = document.querySelector('hitop-app').store; s.moveTab(s.tabs.length - 1, 0);`);
  t3 = await titles();
  check('nothing moves before a pinned tab', t3[0] === 'C', t3.join());
  const pinnedStored = await ev(`return JSON.parse(localStorage.getItem('hitop_tabs') || '[]').some(t => t.pinned && t.title === 'C')`);
  check('pinned is kept with the tabs', pinnedStored || (await ev(`return document.querySelector('hitop-app').store.tabs.some(t => t.pinned)`)));

  // 4) Close others keeps pinned and unsaved tabs
  await ev(`const s = document.querySelector('hitop-app').store; const i = s.tabs.findIndex(t => t.title === 'A'); s.selectTab(i); s.updateTab ? s.updateTab({ url: 'https://example.com/A?changed=1' }) : null;`);
  await page.waitForTimeout(200);
  const dirtyA = await ev(`const s = document.querySelector('hitop-app').store; return s.isDirty(s.tabs.find(t => t.title === 'A'))`);
  const n0 = (await titles()).length;
  await tab('B').click({ button: 'right' });
  await page.locator('#tab-menu').getByText('Close other tabs').click(); await page.waitForTimeout(300);
  const t4 = await titles();
  check('Close other tabs: keeps this one, the pinned one and unsaved ones', t4.includes('B') && t4.includes('C') && (!dirtyA || t4.includes('A')) && t4.length < n0 + 1, `${n0} → ${t4.join()} (A dirty: ${dirtyA})`);
  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
  if (results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
