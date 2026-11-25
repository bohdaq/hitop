// A request's scripts may come from someone else (a shared or imported collection): they can't
// reach the page (session, device key, DOM, network), and one that never ends is stopped.
//   BASE=http://localhost:7879 node tests/script-isolation.cjs   (HITOP_MODE=tauri: the desktop)
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();

const PROBE = `
const reach = [];
for (const name of ["window", "self", "document", "localStorage", "sessionStorage", "indexedDB", "fetch", "XMLHttpRequest", "WebSocket", "Worker", "importScripts", "postMessage", "navigator", "location", "process", "require"]) {
  try { if ((0, eval)("typeof " + name) !== "undefined") reach.push(name); } catch (e) {}
}
try { const g = Function("return this")(); for (const n of ["localStorage", "fetch", "document", "postMessage"]) if (g && g[n]) reach.push("global." + n); } catch (e) {}
try { const g = setContext.constructor("return this")(); if (g.localStorage || g.fetch) reach.push("constructor"); } catch (e) {}
`;

(async () => {
  const hits = [];
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    hits.push({ url: req.url, reach: req.headers['x-reach'] });
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end('{"ok":true}');
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://localhost:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.waitForFunction(() => document.querySelector('hitop-app')?.store?.tab, null, { timeout: 30000 });
  await p.evaluate(() => localStorage.setItem('hitop_canary', 'secret'));
  const run = (pre, post) =>
    p.evaluate(async ([url, pre, post]) => {
      const s = document.querySelector('hitop-app').store;
      Object.assign(s.tab, { url, method: 'GET', preRequestScript: pre, postRequestScript: post });
      await s.send();
      const t = s.tab;
      return { status: t.statusCode, tests: t.testResults ?? t.tests ?? null, response: typeof t.response === 'string' ? t.response.slice(0, 300) : '', keys: Object.keys(t).filter((k) => /test|log/i.test(k)) };
    }, [url(base), pre, post]);
  const url = (b) => `${b}/x`;

  const r = await run(`${PROBE} setHeader("X-Reach", reach.join(",") || "nothing");`, `${PROBE} setContext("after", reach.join(",") || "nothing");`);
  check('the script before reaches nothing of the page', hits[0]?.reach === 'nothing', JSON.stringify({ hits, r }));
  const after = await p.evaluate(() => JSON.stringify(document.querySelector('hitop-app').store.contexts));
  check('the script after reaches nothing of the page', /"after":"nothing"/.test(after), after);

  const pm = await p.evaluate(async () => {
    const s = document.querySelector('hitop-app').store;
    const out = await s.runPostScriptFull("console.log('hello'); pm.test('ok', () => pm.expect(pm.response.json().ok).to.eql(true)); throw new Error('stop here');", null, '{"ok":true}', {}, 200, 5, { url: 'http://x', method: 'GET', headers: [], body: '' });
    return out;
  });
  check('results, console lines and the error of a script that throws are kept', pm.tests[0]?.pass === true && pm.logs[0]?.text === 'hello' && /stop here/.test(pm.error), JSON.stringify(pm));

  if (!TAURI) {
    // Stopped after LIMIT_MS (30 s); the page stays usable meanwhile, and the next script runs.
    const started = Date.now();
    const endless = p.evaluate(() => document.querySelector('hitop-app').store.runPostScriptFull('while (true) {}', null, '', {}, 200, 1, {}).then(() => 'ended', (e) => e.message));
    const usable = await Promise.race([p.evaluate(() => 1 + 1), new Promise((ok) => setTimeout(() => ok('frozen'), 5000))]);
    check('the page stays usable while a script never ends', usable === 2, String(usable));
    const said = await endless;
    check('a script that never ends is stopped', /more than 30 s/.test(said), `${said} after ${Date.now() - started} ms`);
    const again = await p.evaluate(() => document.querySelector('hitop-app').store.runPostScriptFull("pm.test('t', () => {})", null, '', {}, 200, 1, {}).then((r) => r.tests.length));
    check('the next script runs', again === 1, String(again));
  }
  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
