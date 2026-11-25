// Postman import and pm.* scripts, against a running hitop-server and a local
// Node server (no public host): the import report, then a collection run
// whose scripts use pm.test, pm.expect and pm.environment.set.
//   BASE=http://localhost:7879 node tests/postman.cjs
const fs = require('fs');
const http = require('http');
const path = require('path');
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

  await p.locator('aside').getByRole('button', { name: 'Import', exact: true }).click();
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

  await p.locator('aside').getByRole('treeitem', { name: 'Postman shop', exact: true }).first().click();
  await p.getByRole('button', { name: /Run collection/ }).click();
  await p.locator('.run h2', { hasText: 'Last run' }).waitFor({ timeout: 30000 });
  const run = await p.locator('.run').innerText();
  const passed = (run.match(/1 of 1 tests passed/g) || []).length;
  // In the desktop the script commands (Rust) pass script-set variables back too.
  check(`${TAURI ? '[tauri] ' : ''}pm.environment.set carries the token to the next request`, passed === 2 && !/\b401\b/.test(run), run.replace(/\n/g, ' | ').slice(0, 300));

  // Insomnia, HAR and a Bruno folder's zip: the same importer (hitop-core convert.rs).
  const fixture = (f) => fs.readFileSync(path.join(__dirname, '../../cli/fixtures/imports', f), 'utf8');
  const importText = async (text) => {
    await p.evaluate(() => document.querySelector('hitop-app').openDialog('import'));
    await p.locator('#dialog #json').waitFor();
    await p.locator('#dialog #json').evaluate((el, t) => (el.value = t), text);
    await p.locator('#dialog').getByRole('button', { name: 'Import', exact: true }).click();
  };
  const closeReport = async () => {
    await p.locator('#dialog .import-report').waitFor({ timeout: 10000 });
    const text = await p.locator('#dialog').innerText();
    await p.locator('#dialog').getByRole('button', { name: 'Done' }).click();
    await settle(p);
    return text;
  };
  await importText(fixture('insomnia/environments.yaml'));
  await p.waitForFunction(() => document.querySelector('hitop-app').store.collections.some((c) => c.name === 'environments'), null, { timeout: 10000 });
  await settle(p);
  const ins = await p.evaluate(() => { const s = document.querySelector('hitop-app').store; return { body: s.collections.find((c) => c.name === 'environments').requests[0].body, env: s.environments.find((e) => e.name === 'ExampleA')?.variables.exampleString }; });
  check('Insomnia YAML: the request, its variables and the sub environment', ins.body.includes('${exampleObject.anotherString}') && ins.env?.startsWith('sube'), JSON.stringify(ins));
  await importText(fixture('har/recording.har'));
  const harReport = await closeReport();
  const harText = await p.evaluate(() => JSON.stringify(document.querySelector('hitop-app').store.collections));
  check('HAR: requests without the recorded credentials, and the report says so', harText.includes('https://shop.test/orders?page=1') && !harText.includes('recorded-s3cret') && harReport.includes('left out'), harReport.slice(0, 200));
  const { zipSync, strToU8 } = await import('fflate');
  const bru = { 'api/bruno.json': strToU8(fixture('bruno/testbench/bruno.json')), 'api/echo/echo json.bru': strToU8(fixture('bruno/testbench/echo/echo json.bru')), 'api/environments/Prod.bru': strToU8(fixture('bruno/testbench/environments/Prod.bru')) };
  await p.evaluate(() => document.querySelector('hitop-app').store.show('collections'));
  await settle(p);
  await p.locator('#zip-input').setInputFiles({ name: 'api.zip', mimeType: 'application/zip', buffer: Buffer.from(zipSync(bru)) });
  const bruReport = await closeReport();
  const bruReq = await p.evaluate(() => document.querySelector('hitop-app').store.collections.find((c) => c.name === 'bruno-testbench')?.requests[0]);
  check('Bruno zip: the request, its folder, assertions as tests; scripts reported', bruReq?.url === '${host}/api/echo/json' && bruReq.file === 'echo/echo-json' && bruReq.postRequestScript.includes('status eq 200') && bruReport.includes("Bruno's script API"), JSON.stringify(bruReq).slice(0, 300));

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
