// DD1 (HITOP_NEXT_UI.md): drag and drop in the sidebar tree, in web and Tauri mode: a request
// into an empty folder and between rows, Undo, folders kept over a reload, folders reordered and
// nested, collections reordered, Alt copies, a click still opens, Alt+Down, the tab strip, Esc.
//   BASE=http://localhost:7879 node tests/tree-dnd.cjs
//   HITOP_MODE=tauri node tests/tree-dnd.cjs
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
  const my = `const s = document.querySelector('hitop-app').store; const my = s.collections.find(c => /My requests/.test(c.name));`;
  const state = () => ev(`${my} return { req: my.requests.map(r => r.name + ':' + (r.file || '')).join(' '), folders: my.folders || [], cols: s.collections.map(c => c.name), tabs: s.tabs.map(t => t.title) };`);
  // Seed: My requests gets A, B (top), folder "asd" (empty, via the app), folder "api" with C.
  await ev(`${my} const app = document.querySelector('hitop-app');
    const mk = (name, file) => ({ id: Math.floor(Math.random()*1e12), name, url: 'https://example.com/' + name, method: 'GET', headers: [], body: '', ...(file ? { file } : {}) });
    s.collections = s.collections.map(c => c.id === my.id ? { ...c, requests: [mk('A'), mk('B'), mk('C', 'api/c')] } : c); s.emit();
    app.addFolder(my.id, '', 'asd'); await new Promise(r => setTimeout(r, 300));`);
  await page.waitForTimeout(500);
  const row = (name) => page.locator('nav.tree [data-drag="request"]', { hasText: new RegExp(`^\\s*GET\\s*${name}\\s*$`) }).first();
  const folderRow = (name) => page.locator('nav.tree [data-drag="folder"]', { hasText: name }).first();
  const colRow = (name) => page.locator('nav.tree [data-drag="collection"]', { hasText: name }).first();
  const drag = async (src, dst, frac = 0.5, opts = {}) => {
    const a = await src.boundingBox(), b = await dst.boundingBox();
    await page.mouse.move(a.x + 30, a.y + a.height / 2); await page.mouse.down();
    await page.mouse.move(a.x + 30, a.y + a.height / 2 + 7, { steps: 3 });
    await page.mouse.move(b.x + 40, b.y + b.height * frac, { steps: 10 });
    if (opts.alt) await page.keyboard.down('Alt');
    await page.mouse.up();
    if (opts.alt) await page.keyboard.up('Alt');
    await page.waitForTimeout(400);
  };
  // 1) into an EMPTY folder (drop on its row)
  await drag(row('A'), folderRow('asd'), 0.5);
  let st = await state();
  check('request dropped on an empty folder goes inside', st.req.includes('A:asd/a'), st.req);
  check('Undo toast offered', (await page.locator('text=Undo').count()) > 0);
  // 2) between rows: B before C (inside api)
  await drag(row('B'), row('C'), 0.2);
  st = await state();
  check('dropped on the top half of a row = before it, in its folder', /B:api\/b C:api\/c/.test(st.req), st.req);
  // 3) Undo puts it back
  await page.getByRole('button', { name: 'Undo' }).last().click().catch(() => {});
  await page.waitForTimeout(300);
  st = await state();
  check('Undo restores', st.req.startsWith('B:') || /^A:asd\/a B: /.test(st.req) || st.req.includes('B: '), st.req);
  // 4) folders persist over a reload
  const pre = await state();
  await page.reload(); await page.waitForSelector('hitop-app'); await page.waitForTimeout(1500);
  st = await state();
  if (process.env.DEBUG) console.log('reload', JSON.stringify(pre), '=>', JSON.stringify(st));
  check('the folder is stored in the collection', st.folders.includes('asd'), JSON.stringify(st.folders));
  check('after a reload the folder row is there', (await folderRow('asd').count()) === 1);
  // 5) folder before folder: api before asd
  await drag(folderRow('api'), folderRow('asd'), 0.1);
  st = await state();
  const order = await page.locator('nav.tree [data-drag="folder"]').allInnerTexts();
  check('a folder dragged above another lands before it', order.findIndex((t) => /api/.test(t)) < order.findIndex((t) => /asd/.test(t)), order.join('|'));
  // 6) folder into folder
  await drag(folderRow('api'), folderRow('asd'), 0.5);
  st = await state();
  check('a folder dropped on a folder nests', /C:asd\/api\/c/.test(st.req), st.req);
  // 7) collection reorder: My requests above the sample
  const cols0 = (await state()).cols;
  await drag(colRow('My requests'), colRow(cols0[0]), 0.2);
  st = await state();
  check('collections reorder in the sidebar', st.cols[0] === 'My requests', st.cols.join('|'));
  // 8) copy with Alt
  await drag(row('A'), colRow('My requests'), 0.5, { alt: true });
  st = await state();
  check('Alt at the drop copies', (st.req.match(/A:/g) || []).length === 2, st.req);
  // 9) a click still opens a request
  await row('C').click(); await page.waitForTimeout(300);
  check('a click opens the request', (await ev(`const s = document.querySelector('hitop-app').store; return s.tab?.title;`)) === 'C');
  // 10) keyboard Alt+Up
  const before = (await state()).req;
  await row('B').focus(); await page.keyboard.press('Alt+ArrowDown'); await page.waitForTimeout(300);
  st = await state();
  check('Alt+Down moves a request', st.req !== before, before + ' => ' + st.req);
  // 11) drop on the tab strip opens it
  await drag(row('B'), page.locator('.tab-strip'), 0.5);
  const title = await ev(`return document.querySelector('hitop-app').store.tab?.title;`);
  check('a request dropped on the tab strip opens', title === 'B', title);
  // 12) Esc cancels
  const a = await row('C').boundingBox(), b = await folderRow('asd').boundingBox();
  const st0 = (await state()).req;
  await page.mouse.move(a.x + 30, a.y + 10); await page.mouse.down(); await page.mouse.move(a.x + 30, a.y + 20, { steps: 3 }); await page.mouse.move(b.x + 40, b.y + 10, { steps: 6 });
  await page.keyboard.press('Escape'); await page.mouse.up(); await page.waitForTimeout(300);
  check('Esc cancels a drag', (await state()).req === st0);
  check('no ghost or line left', (await page.locator('.tree-drag-ghost, .tree-drop-line').count()) === 0);
  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
  if (results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
