// A collection run can be cancelled: the request in flight is dropped, what ran stays, nothing is
// remembered as a measure, and the next run works (web and, with HITOP_MODE=tauri, the desktop).
const http = require('http');
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();

(async () => {
  let held = null;
  const server = http.createServer((req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    if (req.url === '/hold') {
      held = res;
      return;
    }
    res.end('{"ok":true}');
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://localhost:${server.address().port}`;
  const h = await launch({ results });
  const errors = [];
  h.page.on('pageerror', (e) => errors.push(e.message));
  await h.page.goto(h.url, { waitUntil: 'networkidle' });
  const store = (body, arg) => h.page.evaluate(([b, a]) => new Function('s', 'a', b)(document.querySelector('hitop-app').store, a), [body, arg]);
  await store(`s.addCollection("Run me"); s.collections = s.collections.map((c) => c.name === "Run me" ? { ...c, requests: [1, 2, 3].map((n) => ({ id: 900 + n, name: "r" + n, method: "GET", url: a + (n === 2 ? "/hold" : "/ok"), headers: [], body: "" })) } : c);
    s.view = { kind: "collection", collectionId: s.collections.find((c) => c.name === "Run me").id }; s.emit();`, base);
  const run = () => store('return { ...s.run, n: s.run.results.length };');
  await h.page.getByRole('button', { name: 'Run collection' }).click();
  for (let i = 0; i < 200 && !held; i++) await h.page.waitForTimeout(50);
  check('the second request is in flight', Boolean(held) && (await run()).running);
  await h.page.locator('.cancel-run').click();
  await h.page.waitForFunction(() => !document.querySelector('hitop-app').store.run.running, null, { timeout: 15000 });
  let r = await run();
  check('cancelled: the first result stays, no report', r.cancelled && r.n === 1 && !r.report, JSON.stringify({ c: r.cancelled, n: r.n }));
  check('the Cancel button is gone', (await h.page.locator('.cancel-run').count()) === 0);
  held.end('{}');
  await store(`s.collections = s.collections.map((c) => c.name === "Run me" ? { ...c, requests: c.requests.map((q) => ({ ...q, url: a + "/ok" })) } : c); s.emit();`, base);
  await h.page.getByRole('button', { name: 'Run collection' }).click();
  await h.page.waitForFunction(() => document.querySelector('hitop-app').store.run.report, null, { timeout: 15000 });
  r = await run();
  check('the next run goes through', !r.cancelled && !r.running && r.n === 3, JSON.stringify({ n: r.n }));
  // An import that answers after its dialog was closed doesn't open or change a dialog.
  await h.page.evaluate(() => {
    const app = document.querySelector('hitop-app');
    app.store.importJson = () => new Promise((_, reject) => (window.failImport = () => reject(new Error('late answer'))));
    app.openDialog('import');
  });
  await h.page.locator('#dialog #json').waitFor();
  await h.page.evaluate(() => {
    document.querySelector('#dialog #json').value = '[]';
    document.querySelector('#dialog-form').requestSubmit();
  });
  await h.page.waitForFunction(() => window.failImport, null, { timeout: 10000 });
  await h.page.evaluate(() => document.querySelector('hitop-app').closeDialog());
  await h.page.waitForFunction(() => !document.querySelector('hitop-app').dialog, null, { timeout: 10000 });
  await h.page.evaluate(() => window.failImport());
  await h.page.waitForTimeout(300);
  check('a late answer leaves a closed dialog closed', await h.page.evaluate(() => !document.querySelector('hitop-app').dialog));
  check('no page errors', errors.length === 0, errors.join(' | '));
  server.closeAllConnections?.();
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
