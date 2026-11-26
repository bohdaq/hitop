// Plugins in the web app and the desktop (docs/PLUGINS_THREAT_MODEL.md, part 2): installed from
// Settings, `${plugin.name(args)}` answered before a send (web: their own worker; desktop: hitop-core's
// sandboxed child, run for real by the mock's plugin-json), masked when computed from a secret, stopped
// after 5 s, never in localStorage or exports.
//   BASE=http://localhost:7879 node tests/plugins.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();

const SECRET = 'user:s3cr3t-Plugin';
const SECRET_B64 = 'dXNlcjpzM2NyM3QtUGx1Z2lu';
const manifest = (name, desc = '') => `/* hitop-plugin {"name": "${name}", "version": "1.0.0", "hooks": ["value"]${desc}} */\n`;
const B64 = `${manifest('b64', ', "description": "base64"')}function value(text) {
  const t = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"; let o = "", i = 0;
  for (; i + 2 < text.length; i += 3) { const n = (text.charCodeAt(i) << 16) | (text.charCodeAt(i + 1) << 8) | text.charCodeAt(i + 2); o += t[n >> 18 & 63] + t[n >> 12 & 63] + t[n >> 6 & 63] + t[n & 63]; }
  const r = text.length - i;
  if (r) { const n = (text.charCodeAt(i) << 16) | (r > 1 ? text.charCodeAt(i + 1) << 8 : 0); o += t[n >> 18 & 63] + t[n >> 12 & 63] + (r > 1 ? t[n >> 6 & 63] : "=") + "="; }
  return o;
}`;
const NONCE = `${manifest('nonce')}function value() { return String(Math.random()).slice(2); }`;
const SPIN = `${manifest('spin')}function value() { for (;;) {} }`;
const DEEP = `${manifest('deep')}function value() { return value(); }`;
const HUGE = `${manifest('huge')}function value() { return 'a'.repeat(1e9); }`;
const BIG = `${manifest('big')}function value() { return 'a'.repeat(2 * 1024 * 1024); }`;
const NUM = `${manifest('num')}function value() { return 42; }`;
// Not a loop the engine's loop limit counts: only the 5 s stop ends it.
const REGEX = `${manifest('regex')}function value() { return String(/^(a+)+$/.test('a'.repeat(40) + 'b')); }`;
const PROBE = `${manifest('probe')}function value() { return [typeof process, typeof require, typeof fetch, typeof console, typeof localStorage, typeof indexedDB, typeof document, typeof self, typeof postMessage, typeof importScripts, typeof XMLHttpRequest, typeof WebSocket, typeof BroadcastChannel].filter((t) => t !== "undefined").join(",") || "nothing"; }`;

(async () => {
  const hits = [];
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    hits.push({ url: req.url, nonce: req.headers['x-nonce'], probe: req.headers['x-probe'] });
    res.writeHead(200, { ...cors, 'Content-Type': 'text/plain' }).end(`echo ${new URL(req.url, 'http://x').searchParams.get('sig')}`);
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://localhost:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.waitForFunction(() => document.querySelector('hitop-app')?.store?.tab, null, { timeout: 30000 });
  // An environment with a secret `cred`.
  await p.evaluate(([base, secret]) => {
    const s = document.querySelector('hitop-app').store;
    s.addEnvironment('Prod');
    const env = s.activeEnvironment;
    env.variables = { base, cred: secret };
    env.secrets = ['cred'];
    s.emit();
  }, [base, SECRET]);
  // Sends the current tab with these fields; the toast text when it wasn't sent.
  const send = (fields) =>
    p.evaluate(async (fields) => {
      const s = document.querySelector('hitop-app').store;
      Object.assign(s.tab, { method: 'GET', headers: [], ...fields });
      await s.send();
      return { status: s.tab.statusCode ?? null };
    }, fields);
  // What the app told the person (its notify events), so a refusal is waited for, not slept on.
  await p.evaluate(() => {
    window.__notes = [];
    document.querySelector('hitop-app').store.addEventListener('notify', (e) => window.__notes.push(typeof e.detail === 'string' ? e.detail : JSON.stringify(e.detail)));
  });
  const sendRefused = async (fields) => {
    const before = hits.length;
    const seen = await p.evaluate(() => window.__notes.length);
    await send(fields);
    await p.waitForFunction((n) => window.__notes.slice(n).some((t) => /wasn't sent/.test(t)), seen, { timeout: 30000 });
    const said = await p.evaluate((n) => window.__notes.slice(n).find((t) => /wasn't sent/.test(t)), seen);
    return { said, sent: hits.length - before };
  };
  const signed = { url: '${base}/p?sig=${plugin.b64(cred)}&n=${plugin.nonce()}', headers: [{ name: 'X-Nonce', value: '${plugin.nonce()}', enabled: true }] };

  {
    // Install from Settings: a file chooser.
    await p.evaluate(() => document.querySelector('hitop-app').go('settings'));
    await p.waitForSelector('.plugins-section', { timeout: 10000 });
    const installFile = async (name, text) => {
      await p.locator('.plugin-file').setInputFiles({ name: `${name}.js`, mimeType: 'text/javascript', buffer: Buffer.from(text) });
      await p.waitForSelector(`.plugin-row[data-plugin="${name}"]`, { timeout: 10000 });
    };
    await installFile('b64', B64);
    await installFile('nonce', NONCE);
    const row = await p.locator('.plugin-row[data-plugin="b64"]').textContent();
    check('an installed plugin is listed with its version, hooks and hash', /b64/.test(row) && /1\.0\.0/.test(row) && /hooks: value/.test(row) && /sha256 [0-9a-f]{12}/.test(row) && /base64/.test(row), row);
    await p.locator('.plugin-file').setInputFiles({ name: 'bad.js', mimeType: 'text/javascript', buffer: Buffer.from('function value() {}') });
    await p.waitForSelector('.plugins-section .error-text', { timeout: 10000 });
    check('a file without a manifest is refused with the reason', /starts with its manifest/.test(await p.locator('.plugins-section .error-text').textContent()));

    // A send: the answer goes out, the same call is one answer in one request, the next one calls again.
    await send(signed);
    await send(signed);
    const [a, b] = hits.slice(-2).map((x) => ({ sig: new URL(x.url, base).searchParams.get('sig'), n: new URL(x.url, base).searchParams.get('n'), nonce: x.nonce }));
    check('the plugin answer is sent', a?.sig === SECRET_B64 && b?.sig === SECRET_B64, JSON.stringify(hits));
    check('the same call in one request is one answer', a?.n && a.n === a.nonce && b.n === b.nonce, JSON.stringify([a, b]));
    check('the next request calls it again', a?.n !== b?.n, JSON.stringify([a, b]));
    // T11: an answer computed from a secret is masked like one: the response snapshots in History
    // (the server echoed it). History keeps the request as written, with the call.
    await p.evaluate(() => document.querySelector('hitop-app').store.save());
    const history = JSON.stringify(await h.stored.history());
    check('History has no answer computed from the secret, masked in the response', !history.includes(SECRET_B64) && history.includes('echo ••••') && history.includes('${plugin.b64(cred)}'), (history.match(/.{0,60}(echo|plugin\.b64).{0,40}/g) ?? ['none']).join(' | '));
    const stored = await p.evaluate(() => Object.keys(localStorage).map((k) => localStorage.getItem(k)).join('\n'));
    // Plugins are never in localStorage or exports.
    const exported = await p.evaluate(() => {
      const s = document.querySelector('hitop-app').store;
      return ['hitop', 'postman'].map((f) => String(s.exportJson(f) ?? '')).join('\n');
    });
    check('no plugin is in localStorage or an export', exported.length > 200 && /"requests"|"item"/.test(exported) && !/hitop-plugin|charCodeAt/.test(stored + exported), `export ${exported.length} chars`);

    // Postman-script mode (fillVariables): the answer is filled in there too.
    await send({ ...signed, preRequestScript: "pm.environment.set('seen', 'yes');" });
    check('a request with a Postman script sends the answer', new URL(hits.at(-1).url, base).searchParams.get('sig') === SECRET_B64, JSON.stringify(hits.at(-1)));

    // A collection run: each request calls again; within one, one answer; results masked.
    const run = await p.evaluate(async () => {
      const s = document.querySelector('hitop-app').store;
      const c = s.addCollection('Signed');
      const req = (name) => ({ id: Math.floor(Math.random() * 1e12), name, method: 'GET', url: '${base}/p?sig=${plugin.b64(cred)}&n=${plugin.nonce()}', headers: [{ name: 'X-Nonce', value: '${plugin.nonce()}', enabled: true }] });
      s.collections = s.collections.map((x) => (x.id === c.id ? { ...x, requests: [req('one'), req('two')] } : x));
      s.emit();
      const results = await s.runCollection(c.id);
      return JSON.stringify(results);
    });
    const ran = hits.slice(-2).map((x) => ({ sig: new URL(x.url, base).searchParams.get('sig'), n: new URL(x.url, base).searchParams.get('n'), nonce: x.nonce }));
    check('a run sends the answers, one per request', ran.length === 2 && ran.every((x) => x.sig === SECRET_B64 && x.n === x.nonce) && ran[0].n !== ran[1].n, JSON.stringify(ran));
    check('run results have no answer computed from the secret', !run.includes(SECRET_B64) && run.includes('echo ••••'), run.slice(0, 400));

    // The plugin sees nothing of the worker (boa, not browser JavaScript).
    await installFile('probe', PROBE);
    await send({ url: '${base}/probe', headers: [{ name: 'X-Probe', value: '${plugin.probe()}', enabled: true }] });
    check('a plugin reaches nothing (fetch, storage, the worker, the page)', hits.at(-1)?.probe === 'nothing', JSON.stringify(hits.at(-1)));

    // Deep recursion, a huge or too big answer, an answer that isn't text: each refused with the reason,
    // and the next call works (the worker is replaced after a trap).
    for (const [name, text, says] of [
      ['deep', DEEP, /plugin 'deep' failed/],
      ['huge', HUGE, /plugin 'huge' (stopped|was stopped|failed)/],
      ['big', BIG, /plugin 'big' .*answered more than 1048576 bytes/],
      ['num', NUM, /plugin 'num' answered something other than text/],
    ]) {
      await installFile(name, text);
      const r = await sendRefused({ url: `\${base}/${name}?x=\${plugin.${name}()}` });
      check(`${name}: refused with the reason, nothing sent`, r.sent === 0 && says.test(r.said), JSON.stringify(r).slice(0, 400));
      await send(signed);
      check(`${name}: the next call works`, new URL(hits.at(-1).url, base).searchParams.get('sig') === SECRET_B64, JSON.stringify(hits.at(-1)));
    }
    // A loop is stopped by the engine's loop limit; a catastrophic regex after 5 s. The page stays
    // usable, the next call works.
    await installFile('spin', SPIN);
    let r = await sendRefused({ url: '${base}/spin?x=${plugin.spin()}' });
    check('an endless loop is stopped and nothing is sent', r.sent === 0 && /plugin 'spin' failed: .*maximum number of iteration loops/s.test(r.said), JSON.stringify(r));
    await installFile('regex', REGEX);
    const started = Date.now();
    const stopped = sendRefused({ url: '${base}/regex?x=${plugin.regex()}' });
    const usable = await Promise.race([p.evaluate(() => 1 + 1), new Promise((ok) => setTimeout(() => ok('frozen'), 3000))]);
    check('the page stays usable while a plugin runs', usable === 2, String(usable));
    r = await stopped;
    const took = Date.now() - started;
    check('a plugin that runs on is stopped after 5 s and nothing is sent', r.sent === 0 && /plugin 'regex' was stopped: it ran longer than 5 s/.test(r.said) && took >= 4500 && took < 15000, `${JSON.stringify(r)} after ${took} ms`);
    await send(signed);
    check('the next call works', new URL(hits.at(-1).url, base).searchParams.get('sig') === SECRET_B64, JSON.stringify(hits.at(-1)));

    // A changed plugin (its stored text) is refused until installed again.
    if (TAURI) {
      const file = require('path').join(h.mock.dir, 'xdg', 'hitop', 'plugins', 'b64.js');
      const fs = require('fs');
      fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('return o;', 'return o + "x";'));
    } else await p.evaluate(() => new Promise((ok, no) => {
      const req = indexedDB.open('hitop-plugins');
      req.onsuccess = () => {
        const t = req.result.transaction('plugins', 'readwrite');
        const st = t.objectStore('plugins');
        st.get('b64').onsuccess = (e) => st.put({ ...e.target.result, source: e.target.result.source.replace('return o;', 'return o + "x";') }, 'b64');
        t.oncomplete = () => (req.result.close(), ok());
        t.onerror = () => no(t.error);
      };
    }));
    let refused = await sendRefused(signed);
    check('a changed plugin is refused', refused.sent === 0 && /plugin 'b64' changed since it was installed/.test(refused.said), JSON.stringify(refused));

    // Removed: refused, naming it.
    p.once('dialog', () => {});
    await p.evaluate(() => document.querySelector('hitop-app').go('settings'));
    await p.locator('.plugin-row[data-plugin="b64"] lit-material-button').click();
    await p.getByRole('button', { name: 'Remove', exact: true }).last().click();
    await p.waitForFunction(() => !document.querySelector('.plugin-row[data-plugin="b64"]'), null, { timeout: 10000 });
    refused = await sendRefused(signed);
    check('a request calling a plugin that is not installed is not sent, and names it', refused.sent === 0 && /plugin 'b64', which isn't installed/.test(refused.said), JSON.stringify(refused));
    // Scripts still run in their own worker after all this.
    const again = await p.evaluate(() => document.querySelector('hitop-app').store.runPostScriptFull("pm.test('t', () => {})", null, '', {}, 200, 1, {}).then((r) => r.tests.length));
    check('scripts still run', again === 1, String(again));
  }
  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
