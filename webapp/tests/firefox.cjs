// The Firefox extension's script runner (HITOP Next): Firefox has no sandboxed extension pages, so
// pre/post scripts run in boa compiled to WebAssembly in a worker (src/platform/wasm-scripts.js).
// Playwright's Firefox can't load an extension, so this serves the extension build
// (dist-extension/) over http with the extension pages' CSP (no eval, 'wasm-unsafe-eval') and opens
// it with ?wasm-scripts, which picks the same runner the moz-extension page picks.
//   npm run build:extension && node tests/firefox.cjs
const http = require('http');
const path = require('path');
const fs = require('fs');
const { firefox } = require('playwright');

const CSP = "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'";
const results = [];
const check = (name, ok, detail = '') => {
  results.push(ok);
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${ok || !detail ? '' : ` — ${String(detail).slice(0, 800)}`}`);
};
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.png': 'image/png', '.json': 'application/json', '.woff2': 'font/woff2' };

(async () => {
  const dist = path.join(__dirname, '../dist-extension');
  if (!fs.existsSync(path.join(dist, 'index.html'))) {
    console.log('[firefox] dist-extension missing (npm run build:extension): skipped');
    process.exit(0);
  }
  const app = http.createServer((req, res) => {
    const file = path.join(dist, decodeURIComponent(new URL(req.url, 'http://x').pathname));
    if (!file.startsWith(dist) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) return res.writeHead(404).end();
    res.writeHead(200, { 'Content-Type': TYPES[path.extname(file)] ?? 'application/octet-stream', 'Content-Security-Policy': CSP });
    fs.createReadStream(file).pipe(res);
  });
  let seen = null;
  const api = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    seen = { url: req.url, headers: req.headers };
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end('{"id":7,"status":"paid"}');
  });
  await new Promise((r) => app.listen(0, '127.0.0.1', r));
  await new Promise((r) => api.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${api.address().port}`;
  const browser = await firefox.launch({ headless: true });
  try {
    const p = await browser.newPage();
    const errors = [];
    const csp = [];
    p.on('pageerror', (e) => errors.push(e.message));
    p.on('console', (m) => /Content.Security.Policy|unsafe-eval|CSP/i.test(m.text()) && csp.push(m.text()));
    await p.goto(`http://127.0.0.1:${app.address().port}/index.html?wasm-scripts`);
    await p.waitForFunction(() => document.querySelector('hitop-app')?.store?.tab, null, { timeout: 30000 });
    check('the extension build opens in Firefox under the extension CSP', (await p.title()).includes('HITOP'));
    const send = async (pre, post) => {
      seen = null;
      await p.evaluate(([u, a, b]) => {
        const s = document.querySelector('hitop-app').store;
        s.show('request');
        s.updateTab({ url: `${u}/orders/7`, preRequestScript: a, postRequestScript: b, tests: [] });
        return s.send();
      }, [base, pre, post]);
      await p.waitForFunction(() => { const t = document.querySelector('hitop-app').store.tab; return t.statusCode || t.responseType === 'error'; }, null, { timeout: 40000 }).catch(() => {});
      return p.evaluate(() => { const t = document.querySelector('hitop-app').store.tab; return { status: t.statusCode, type: t.responseType, tests: t.tests, response: String(t.response).slice(0, 300) }; });
    };
    const tab = await send(
      'setHeader("X-From-Script", "pre-" + (1 + 1));\npm.request.headers.add({ key: "X-Pm", value: "shim" });\nsetHeader("X-Sees", [typeof document, typeof localStorage, typeof fetch, typeof browser].join("/"));',
      'pm.test("status is 200", () => pm.response.to.have.status(200));\npm.test("paid", () => pm.expect(pm.response.json().status).to.eql("paid"));\npm.test("fails on purpose", () => pm.expect(1).to.eql(2));',
    );
    check('a request with scripts is answered', tab.status === 200, JSON.stringify(tab));
    check('the script before ran (setHeader)', seen?.headers['x-from-script'] === 'pre-2', JSON.stringify(seen?.headers));
    check('the pm.* shim ran before the request', seen?.headers['x-pm'] === 'shim', JSON.stringify(seen?.headers));
    check('a script sees no page, storage, fetch or extension API', seen?.headers['x-sees'] === 'undefined/undefined/undefined/undefined', seen?.headers['x-sees']);
    check('the script after ran its pm.test checks: 2 pass, 1 fails', tab.tests?.length === 3 && tab.tests[0].pass && tab.tests[1].pass && !tab.tests[2].pass, JSON.stringify(tab.tests));
    await p.evaluate(() => {
      const s = document.querySelector('hitop-app').store;
      const notify = s.notify.bind(s);
      s.notify = (m, ...rest) => ((window.__note = m), notify(m, ...rest));
    });
    await send('throw new Error("broken on purpose")', '').catch(() => {});
    const note = await p.evaluate(() => window.__note ?? '');
    check('a script that throws stops the request and says why', seen === null && /broken on purpose/.test(note), note);
    check('no page errors', errors.length === 0, errors.join(' | '));
    check('no CSP violation', csp.length === 0, csp.join(' | '));
  } finally {
    await browser.close();
    app.close();
    api.close();
  }
  process.exit(results.every(Boolean) ? 0 : 1);
})().catch((e) => {
  console.error('CRASH', e.message);
  process.exit(1);
});
