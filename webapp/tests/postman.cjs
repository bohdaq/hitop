// Postman import and pm.* scripts, against a running hitop-server and a local
// Node server (no public host): the import report, then a collection run
// whose scripts use pm.test, pm.expect and pm.environment.set.
//   BASE=http://localhost:7879 node tests/postman.cjs
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

// Login answers a token; /me needs it as a bearer token. CORS for the page.
function api() {
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    const json = (status, body) => res.writeHead(status, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify(body));
    if (req.url === '/login') return json(200, { token: 't-1' });
    if (req.url === '/me') return req.headers.authorization === 'Bearer t-1' ? json(200, { name: 'ada' }) : json(401, { error: 'no token' });
    json(404, {});
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

(async () => {
  const server = await api();
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ viewport: { width: 1280, height: 900 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });

  const collection = {
    info: { name: 'Postman shop', schema: 'https://schema.getpostman.com/json/collection/v2.1.0/collection.json' },
    variable: [{ key: 'base', value: base }],
    item: [
      {
        name: 'Login',
        request: { method: 'POST', url: '{{base}}/login' },
        event: [{ listen: 'test', script: { exec: ["pm.test('logged in', () => pm.response.to.have.status(200));", "pm.environment.set('token', pm.response.json().token);"] } }],
      },
      {
        name: 'Account',
        // Never runs (pm.info isn't there), but the report lists both calls.
        event: [{ listen: 'prerequest', script: { exec: ['if (pm.info) {', "  pm.sendRequest('{{base}}/ping', () => {});", '}'] } }],
        item: [
          {
            name: 'Me',
            request: { method: 'GET', url: '{{base}}/me', auth: { type: 'bearer', bearer: [{ key: 'token', value: '{{token}}' }] } },
            event: [{ listen: 'test', script: { exec: ["pm.test('is ada', function () {", "  pm.expect(pm.response.json()).to.have.property('name').that.equals('ada');", '});'] } }],
          },
        ],
      },
    ],
  };

  await p.locator('aside').getByRole('button', { name: 'Import' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog #json').evaluate((el, text) => (el.value = text), JSON.stringify(collection));
  await p.locator('#dialog').getByRole('button', { name: /import/i }).click();
  await p.locator('#dialog .import-report').waitFor({ timeout: 10000 });
  const report = await p.locator('#dialog').innerText();
  check('import report lists the unsupported calls by line', report.includes('folder Account (script before), line 1: pm.info') && report.includes('line 2: pm.sendRequest') && report.includes("pm.sendRequest('{{base}}/ping', () => {});"), report.replace(/\n/g, ' | ').slice(0, 300));
  check('import report copy has no hyphens', !/pre-request|post-request/.test(report));
  await p.locator('#dialog').getByRole('button', { name: 'Done' }).click();
  await settle(p);

  const cols = await h.stored.collections();
  const shop = cols.find((c) => c.name === 'Postman shop');
  const me = shop?.requests.find((r) => r.name === 'Me');
  check('folders, {{variables}} and auth come over', me?.file === 'account/me' && me.url === '${base}/me' && me.auth?.token === '${token}', JSON.stringify(me));

  await p.locator('aside').getByRole('button', { name: 'Postman shop', exact: true }).first().click();
  await p.getByRole('button', { name: /Run collection/ }).click();
  await p.locator('.run h2', { hasText: 'Last run' }).waitFor({ timeout: 30000 });
  const run = await p.locator('.run').innerText();
  const passed = (run.match(/1 of 1 tests passed/g) || []).length;
  if (TAURI) {
    // The desktop's script commands don't pass script-set variables back yet
    // (desktop/src-tauri/src/commands.rs), so Me has no token to send.
    check('[tauri] pm.test runs in Rust', passed === 1 && run.includes("'token' not found"), run.replace(/\n/g, ' | ').slice(0, 300));
  } else {
    check('pm.environment.set carries the token to the next request', passed === 2 && !/\b401\b/.test(run), run.replace(/\n/g, ' | ').slice(0, 300));
  }

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
