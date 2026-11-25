// The Chrome extension built from the web app (HITOP Next A8), loaded unpacked in Chromium: the
// page opens without a CSP violation, a request goes to a server that sends no CORS headers (host
// permissions, no proxy), and the scripts before and after it run in the script worker (boa in wasm), where they reach
// nothing of the page or the network.
//   npm run build:extension && node scripts/package-extension.mjs chrome <dir> && EXTENSION_DIR=<dir> node tests/extension.cjs
const http = require('http');
const os = require('os');
const path = require('path');
const fs = require('fs');
const { chromium } = require('playwright');

const results = [];
const check = (name, ok, detail = '') => {
  results.push(ok);
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${ok || !detail ? '' : ` — ${String(detail).slice(0, 800)}`}`);
};

(async () => {
  const dir = process.env.EXTENSION_DIR;
  if (!dir || !fs.existsSync(path.join(dir, 'manifest.json'))) {
    console.log('[extension] EXTENSION_DIR with a packaged Chrome extension not set: skipped');
    process.exit(0);
  }
  let seen = null;
  const server = http.createServer((req, res) => {
    seen = { method: req.method, url: req.url, headers: req.headers };
    // No Access-Control-* headers: only an extension with host permissions can read this.
    res.writeHead(200, { 'Content-Type': 'application/json' }).end('{"id":7,"status":"paid"}');
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}`;
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-ext-'));
  const ctx = await chromium.launchPersistentContext(profile, {
    channel: 'chromium',
    headless: true,
    // A full Chromium (extensions don't load in the headless shell); CHROMIUM_PATH picks one.
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
    args: [`--disable-extensions-except=${path.resolve(dir)}`, `--load-extension=${path.resolve(dir)}`],
  });
  try {
    let [worker] = ctx.serviceWorkers();
    worker ??= await ctx.waitForEvent('serviceworker', { timeout: 15000 });
    const id = new URL(worker.url()).host;
    const p = await ctx.newPage();
    const errors = [];
    const csp = [];
    p.on('pageerror', (e) => errors.push(e.message));
    p.on('console', (m) => /Content Security Policy|unsafe-eval/i.test(m.text()) && csp.push(m.text()));
    await p.goto(`chrome-extension://${id}/app/index.html`);
    await p.locator('hitop-app').waitFor({ timeout: 15000 });
    await p.waitForFunction(() => document.querySelector('hitop-app')?.store?.tab, null, { timeout: 15000 });
    check('the app opens as an extension page', (await p.title()).includes('HITOP'));
    check('dark from boot.js (no inline script)', (await p.evaluate(() => document.documentElement.getAttribute('color-scheme'))) === 'dark');
    await p.evaluate((u) => {
      const s = document.querySelector('hitop-app').store;
      s.show('request');
      s.updateTab({
        url: `${u}/orders/7`,
        preRequestScript: 'setHeader("X-From-Script", "pre-" + (1 + 1));',
        postRequestScript: 'pm.test("status is 200", () => pm.response.to.have.status(200));\npm.test("paid", () => pm.expect(pm.response.json().status).to.eql("paid"));',
      });
    }, base);
    await p.evaluate(() => document.querySelector('hitop-app').store.send());
    await p.waitForFunction(() => document.querySelector('hitop-app').store.tab.tests?.length === 2, null, { timeout: 15000 }).catch(() => {});
    const tab = await p.evaluate(() => { const t = document.querySelector('hitop-app').store.tab; return { status: t.statusCode, tests: t.tests, response: String(t.response).slice(0, 200) }; });
    check('a host without CORS answers the extension (host permissions, no proxy)', tab.status === 200 && tab.response.includes('paid'), JSON.stringify(tab));
    check('the script before ran', seen?.headers['x-from-script'] === 'pre-2', JSON.stringify(seen?.headers));
    check('the script after ran: 2 of 2 tests pass', tab.tests?.length === 2 && tab.tests.every((t) => t.pass), JSON.stringify(tab.tests));
    // Scripts may be someone else's: they find neither the page nor a network.
    const reach = await p.evaluate(async () => {
      const r = await document.querySelector('hitop-app').store.runPostScriptFull('const seen = []; for (const n of ["fetch", "XMLHttpRequest", "WebSocket", "importScripts", "localStorage", "document", "chrome", "postMessage"]) if ((0, eval)("typeof " + n) !== "undefined") seen.push(n); assert.equal(seen.join(","), "", "reach");', null, '', {}, 200, 1, {});
      return r.tests[0];
    });
    check('a script reaches neither the page nor the network', reach?.pass === true, JSON.stringify(reach));
    // Owner decision 2026-11-10: public hosts through hitop.rws8.tech's proxy, limits said in the app.
    const bundles = fs.readdirSync(path.join(dir, 'app/assets')).filter((f) => f.endsWith('.js')).map((f) => fs.readFileSync(path.join(dir, 'app/assets', f), 'utf8')).join('\n');
    check('public hosts go through the HITOP proxy (built in)', bundles.includes('https://hitop.rws8.tech/api/proxy'));
    await p.evaluate(() => document.querySelector('hitop-app').store.show('settings'));
    await p.locator('section[aria-label="This extension"]').waitFor({ timeout: 10000 }).catch(() => {});
    const limits = await p.locator('section[aria-label="This extension"]').innerText().catch(() => '');
    check('Settings states the extension limits', /secret-manager/.test(limits) && /Git panel/.test(limits) && /store reviews/.test(limits), limits);
    check('no page errors', errors.length === 0, errors.join(' | '));
    check('no CSP violation', csp.length === 0, csp.join(' | '));
  } finally {
    await ctx.close();
    server.close();
    fs.rmSync(profile, { recursive: true, force: true });
  }
  process.exit(results.every(Boolean) ? 0 : 1);
})().catch((e) => {
  console.error('CRASH', e.message);
  process.exit(1);
});
