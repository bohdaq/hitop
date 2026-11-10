// Response diff and the Code panel: two responses of a saved request (a
// saved example against the current one, then history items) compare by
// JSON structure, key order ignored; history from before responses were
// kept still loads and isn't offered; the Code panel shows hitop-core's
// snippet with secrets as {{NAME}} until "Include secret values". Seeded
// with old storage (as e2e.cjs); requests go to a local Node server only.
//   BASE=http://localhost:7879 node tests/diff-code.cjs   (HITOP_MODE=tauri: the desktop build)
const fs = require('fs');
const http = require('http');
const path = require('path');
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);
const SECRET = 'd1ff-s3cret';

// /items answers JSON that changes on each call (and its key order with it).
function api() {
  let n = 0;
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*', 'Access-Control-Expose-Headers': 'x-run' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    n++;
    res.sendDate = false; // the desktop sees Date (no CORS filter): keep x-run the only header that changes
    const url = new URL(req.url, 'http://x');
    const body = n === 1 ? { id: 1, name: 'a', tags: ['x'], n, key: url.searchParams.get('key') } : { key: url.searchParams.get('key'), tags: ['x', 'y'], n, name: 'a', id: 1 };
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json', 'x-run': String(n) }).end(JSON.stringify(body));
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

(async () => {
  const server = await api();
  const base = `http://127.0.0.1:${server.address().port}`;
  const fixture = JSON.parse(fs.readFileSync(path.join(__dirname, 'legacy-storage.json'), 'utf8'));
  const h = await launch({ seed: fixture, tauriSeed: path.join(__dirname, 'desktop-storage'), results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const type = async (text) => p.keyboard.type(text, { delay: 10 });
  const send = async () => {
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => document.querySelector('.response-meta, .error-text') && !document.querySelector('[aria-busy="true"]'), null, { timeout: 30000 });
    await settle(p);
  };
  const dialog = p.locator('#dialog');

  await p.goto(h.url, { waitUntil: 'networkidle' });
  const before = await h.stored.history();
  check('old history loads (items without a response)', before.length > 0 && before.every((i) => !i.response), `${before.length} items`);

  // A global and a secret global.
  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await settle(p);
  await p.getByRole('button', { name: 'Add global' }).click();
  await type('base');
  await p.keyboard.press('Tab');
  await type(base);
  await p.getByRole('button', { name: 'Add a secret global' }).click();
  await type('key');
  await p.keyboard.press('Tab');
  await type(SECRET);
  await settle(p);

  // A saved request.
  await p.locator('aside').getByRole('button', { name: /New request/ }).first().click();
  await p.getByLabel('URL').fill('${base}/items?key=${key}');
  await p.getByRole('button', { name: 'Save request' }).click();
  await settle(p);
  await type('Items');
  await p.keyboard.press('Enter');
  await settle(p);

  // Code panel: curl by default, the secret as a placeholder.
  await p.getByRole('button', { name: 'Code' }).click();
  await dialog.locator('.snippet').waitFor({ timeout: 10000 });
  let code = await dialog.locator('.snippet').evaluate((el) => el.textContent);
  check('code panel: curl with the secret as {{key}}', code.includes(`curl '${base}/items?key={{key}}'`) && !code.includes(SECRET), code);
  check('code panel names the placeholder', (await dialog.locator('.snippet-placeholders').innerText()).includes('{{key}}'));
  for (const [lang, want] of [['python', 'import requests'], ['go', 'http.NewRequest("GET"'], ['java', 'HttpClient.newHttpClient()'], ['rust', 'reqwest::blocking::Client::new()'], ['fetch', 'await fetch(']]) {
    await dialog.locator('#snippet-lang').evaluate((el, v) => {
      el.value = v;
      el.dispatchEvent(new Event('change', { bubbles: true }));
    }, lang);
    await p.waitForFunction((w) => document.querySelector('#dialog .snippet')?.textContent.includes(w), want, { timeout: 10000 });
    code = await dialog.locator('.snippet').evaluate((el) => el.textContent);
    check(`code panel: ${lang}`, code.includes(want) && code.includes('{{key}}') && !code.includes(SECRET), code.slice(0, 200));
  }
  await dialog.locator('#snippet-secrets').click();
  await p.waitForFunction((s) => document.querySelector('#dialog .snippet')?.textContent.includes(s), SECRET, { timeout: 10000 });
  check('"Include secret values" writes the value', (await dialog.innerText()).includes("don't share or commit it"));
  await dialog.getByRole('button', { name: 'Close' }).click();
  await settle(p);

  // First response, saved as an example; then a second one.
  await send();
  await p.getByRole('button', { name: /Save as example/ }).click();
  await settle(p);
  await p.keyboard.press('ControlOrMeta+a');
  await type('first');
  await p.keyboard.press('Enter');
  await settle(p);
  const saved = (await h.stored.collections()).flatMap((c) => c.requests).find((r) => r.name === 'Items');
  const ex = saved?.examples?.[0];
  check('example saved with the request', ex?.name === 'first' && ex.status === 200 && ex.body.includes('"tags"'), JSON.stringify(ex)?.slice(0, 200));
  check('example has the secret masked', ex && !JSON.stringify(ex).includes(SECRET));
  await send();

  // Compare: the example (before) against the current response (after).
  await p.getByRole('button', { name: /^Compare/ }).first().click();
  await dialog.locator('.compare-summary').waitFor({ timeout: 10000 });
  const summary = await dialog.locator('.compare-summary').innerText();
  check('compare: differs in a header and body paths', summary.includes('1 header') && summary.includes('2 body paths'), summary);
  const changes = await dialog.locator('.json-changes li').allInnerTexts();
  check('JSON paths: $.n changed, $.tags[1] added, key order ignored', changes.length === 2 && changes.some((c) => c.startsWith('$.n') && c.includes('changed')) && changes.some((c) => c.startsWith('$.tags[1]') && c.includes('added')), changes.join(' | '));
  const headers = await dialog.locator('.compare-headers li').allInnerTexts();
  check('headers: x-run changed', headers.length === 1 && headers[0].includes('x-run') && headers[0].includes('changed'), headers.join(' | '));
  check('side by side rows', (await dialog.locator('.side-by-side .row.changed').count()) >= 1);
  const options = await dialog.locator('#compare-left lit-material-select-option').allInnerTexts();
  const history = options.filter((o) => !o.startsWith('Current') && !o.startsWith('Example'));
  check('sources: current, the example and the two new history items (old ones left out)', options.length === 4 && options[0] === 'Current response' && options[1] === 'Example: first' && history.length === 2, options.join(' | '));
  // The same response on both sides.
  await dialog.locator('#compare-left').evaluate((el) => {
    el.value = 'current';
    el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await p.waitForFunction(() => document.querySelector('#dialog .compare-summary')?.textContent.includes('same'), null, { timeout: 10000 });
  check('a response against itself is the same', (await dialog.locator('.compare-summary').innerText()) === 'The responses are the same.');
  await dialog.getByRole('button', { name: 'Close' }).click();
  await settle(p);

  const after = await h.stored.history();
  const kept = after.filter((i) => i.response);
  check('history keeps the responses, secret masked', kept.length === 2 && kept.every((i) => i.response.status === 200 && !JSON.stringify(i).includes(SECRET)) && after.length === before.length + 2, JSON.stringify(kept[0]?.response)?.slice(0, 200));

  // The History page offers Compare.
  await p.getByRole('button', { name: 'History' }).first().click();
  await settle(p);
  await p.locator('main').getByRole('button', { name: 'Compare responses' }).click();
  await dialog.locator('.compare-summary').waitFor({ timeout: 10000 });
  check('history → Compare responses opens the compare view', (await dialog.locator('.compare-summary').innerText()).length > 0);

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
