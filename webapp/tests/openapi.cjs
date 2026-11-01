// OpenAPI import and sync, against a running hitop-server (web) or the
// mocked desktop (HITOP_MODE=tauri) and a local Node server that serves the
// specs and the API: import from a URL and from pasted YAML (with the
// report), edits to the imported requests, then sync with a newer spec:
// preview of the diff, apply, edits kept, changes applied, a run.
//   BASE=http://localhost:7879 node tests/openapi.cjs
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

const spec = (base, v2) => `openapi: 3.0.3
info: {title: Shop, version: "${v2 ? 2 : 1}"}
servers: [{url: "${base}/v1"}]
tags: [{name: pets}]
components:
  securitySchemes:
    key: {type: apiKey, in: header, name: X-Key}
security: [{key: []}]
paths:
  /pets:
    get:
      tags: [pets]
      operationId: listPets
      summary: List pets
      parameters:
      - {name: limit, in: query, required: true, example: ${v2 ? 20 : 10}}
      - {name: X-Api-Version, in: header, required: true, example: "${v2 ? 2 : 1}"}
      - {name: X-Client, in: header, required: true, example: web}
      responses:
        "200":
          description: ok
          content: {application/json: {example: [{id: 1, name: Rex}]}}
    post:
      tags: [pets]
      operationId: addPet
      summary: Add a pet
      requestBody: {content: {application/json: {example: {name: Rex${v2 ? ', kind: dog' : ''}}}}}
      responses: {"201": {description: ok}}
  /pets/{id}:
${v2
  ? `    get:
      tags: [pets]
      operationId: getPet
      summary: Get a pet
      parameters: [{name: id, in: path, required: true, schema: {type: integer, example: 1}}]
      responses: {"200": {description: ok}}
`
  : `    delete:
      tags: [pets]
      operationId: deletePet
      summary: Delete a pet
      parameters: [{name: id, in: path, required: true, schema: {type: integer}}]
      responses: {"204": {description: ok}}
`}`;

function api() {
  const hits = [];
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    const base = `http://127.0.0.1:${server.address().port}`;
    if (req.url === '/openapi.yaml') return res.writeHead(200, { ...cors, 'Content-Type': 'application/yaml' }).end(spec(base, false));
    if (req.url === '/v2.yaml') return res.writeHead(200, { ...cors, 'Content-Type': 'application/yaml' }).end(spec(base, true));
    hits.push({ method: req.method, url: req.url, key: req.headers['x-key'] || '' });
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end('[]');
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, hits })));
}

(async () => {
  const { server, hits } = await api();
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ viewport: { width: 1280, height: 900 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  const dialog = p.locator('#dialog');
  const shop = async () => (await h.stored.collections()).find((c) => c.name === 'Shop');

  // 1. From a URL, through the Import dialog's link.
  await p.locator('aside').getByRole('button', { name: 'Import', exact: true }).click();
  await settle(p);
  await dialog.getByRole('button', { name: /OpenAPI spec from a URL/ }).click();
  await settle(p);
  await p.locator('#dialog #spec-url').evaluate((el, url) => (el.value = url), `${base}/openapi.yaml`);
  await dialog.getByRole('button', { name: 'Import', exact: true }).click();
  await p.locator('#dialog .dialog-note', { hasText: 'imported from OpenAPI' }).waitFor({ timeout: 15000 });
  const report = await dialog.innerText();
  check('report says what was mapped', /3 operations became 3 requests\s+in 1 folder/.test(report) && report.includes('Variables: baseUrl, id'), report.replace(/\n/g, ' | ').slice(0, 300));
  check('report lists what to check', report.includes('To check') && report.includes('set key in an environment'), report.replace(/\n/g, ' | ').slice(0, 400));
  await dialog.getByRole('button', { name: 'Done' }).click();
  await settle(p);

  let col = await shop();
  const list = col?.requests.find((r) => r.name === 'List pets');
  check('requests come over with the record and the inlined auth', list?.url === '${baseUrl}/pets?limit=10' && list?.openapi?.operationId === 'listPets' && list?.auth?.key === 'X-Key' && list?.file === 'pets/list-pets', JSON.stringify(list));
  check('the server is the baseUrl variable; the source is recorded', col?.variables?.baseUrl === `${base}/v1` && col?.openapi?.source === `${base}/openapi.yaml`, JSON.stringify(col?.variables) + JSON.stringify(col?.openapi));
  check('response examples are saved examples', list?.examples?.[0]?.name === '200 ok' && list.examples[0].source === 'openapi', JSON.stringify(list?.examples));

  // 2. Pasted into the Import dialog: added, nothing replaced.
  await p.locator('aside').getByRole('button', { name: 'Import', exact: true }).click();
  await settle(p);
  await p.locator('#dialog #json').evaluate((el, text) => (el.value = text), spec(base, false).replace('title: Shop', 'title: Pasted shop'));
  await dialog.getByRole('button', { name: /import/i }).last().click();
  await p.locator('#dialog .dialog-note', { hasText: 'imported from OpenAPI' }).waitFor({ timeout: 15000 });
  await dialog.getByRole('button', { name: 'Done' }).click();
  await settle(p);
  const names = (await h.stored.collections()).map((c) => c.name);
  check('pasted YAML is added next to the others', names.includes('Shop') && names.includes('Pasted shop'), names.join(', '));

  // 3. Your edits: a script and an assertion, an auth override, a rename, a
  // body, and a value for the API key variable.
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.collections = s.collections.map((c) =>
      c.name !== 'Shop'
        ? c
        : {
            ...c,
            variables: { ...c.variables, key: 'k-1', token: 't-1' },
            requests: c.requests.map((r) =>
              r.name === 'List pets'
                ? { ...r, name: 'All my pets', preRequestScript: "setHeader('X-Run', '1');", postRequestScript: 'assert.status(200);', headers: [...r.headers, { name: 'X-Mine', value: 'mine', enabled: true }] }
                : r.name === 'Add a pet'
                  ? { ...r, body: '{"name": "Fido"}', auth: { type: 'bearer', token: '${token}' } }
                  : r,
            ),
          },
    );
    s.emit();
  });
  await settle(p);

  // 4. Sync with the newer spec: preview, then apply.
  await p.locator('aside').getByRole('button', { name: 'Shop', exact: true }).first().click();
  await settle(p);
  await p.getByRole('button', { name: /Sync with OpenAPI/ }).click();
  await settle(p);
  const prefilled = await p.locator('#dialog #spec-url').evaluate((el) => el.value);
  check('sync starts from the recorded URL', prefilled === `${base}/openapi.yaml`, prefilled);
  await p.locator('#dialog #spec-url').evaluate((el, url) => (el.value = url), `${base}/v2.yaml`);
  await dialog.getByRole('button', { name: 'Preview' }).click();
  await p.locator('#dialog .openapi-summary').waitFor({ timeout: 15000 });
  const diff = await dialog.innerText();
  check('preview: counts', diff.includes('1 added, 2 changed, 1 removed, 0 unchanged'), diff.replace(/\n/g, ' | ').slice(0, 500));
  check('preview: added, changed with kept edits, removed', diff.includes('GET /pets/{id} (getPet)') && /All my pets: updated url/.test(diff) && diff.includes('Add a pet: kept your body') && diff.includes('Delete a pet: gone from the spec, kept'), diff.replace(/\n/g, ' | ').slice(0, 600));
  check(
    'preview: header rows one by one (the spec changed one, you added one)',
    diff.includes('Header X-Api-Version') && diff.includes('header X-Api-Version changed by the spec: updated') && diff.includes('header X-Mine added by you: kept') && /All my pets: updated url, headers/.test(diff),
    diff.replace(/\n/g, ' | ').slice(0, 800),
  );
  check('nothing changes before Apply', (await shop()).requests.length === 3);
  await dialog.getByRole('button', { name: 'Apply' }).click();
  await settle(p);

  col = await shop();
  const mine = col.requests.find((r) => r.openapi?.operationId === 'listPets');
  const add = col.requests.find((r) => r.openapi?.operationId === 'addPet');
  check('edits survive: name, scripts', mine?.name === 'All my pets' && mine.preRequestScript === "setHeader('X-Run', '1');" && mine.postRequestScript === 'assert.status(200);', JSON.stringify(mine));
  check("spec changes apply where you didn't edit", mine?.url === '${baseUrl}/pets?limit=20', mine?.url);
  const rows = (mine?.headers ?? []).filter((x) => x.name).map((x) => `${x.name}=${x.value}`).join(',');
  check("both header outcomes: the spec's new value and your own header", rows === 'X-Api-Version=2,X-Client=web,X-Mine=mine', rows);
  check('your body and auth override stay', add?.body === '{"name": "Fido"}' && add?.auth?.type === 'bearer', JSON.stringify(add));
  check('the new operation is added, the removed one kept', col.requests.some((r) => r.openapi?.operationId === 'getPet' && r.url === '${baseUrl}/pets/${id}') && col.requests.some((r) => r.name === 'Delete a pet'), col.requests.map((r) => r.name).join(', '));
  check('the record moves to the new version', col.openapi?.version === '2' && col.openapi?.source === `${base}/v2.yaml`, JSON.stringify(col.openapi));

  // 5. Sync again with prune: the removed operation's request goes.
  await p.getByRole('button', { name: /Sync with OpenAPI/ }).click();
  await settle(p);
  await p.locator('#dialog #spec-prune').click();
  await dialog.getByRole('button', { name: 'Preview' }).click();
  await p.locator('#dialog .openapi-summary').waitFor({ timeout: 15000 });
  await dialog.getByRole('button', { name: 'Apply and delete' }).click();
  await settle(p);
  col = await shop();
  check('prune deletes it', !col.requests.some((r) => r.name === 'Delete a pet') && col.requests.length === 3, col.requests.map((r) => r.name).join(', '));

  // 6. The synced collection runs against the API.
  await p.getByRole('button', { name: /Run collection/ }).click();
  await p.locator('.run h2', { hasText: 'Last run' }).waitFor({ timeout: 30000 });
  const listed = hits.find((x) => x.method === 'GET' && x.url === '/v1/pets?limit=20' && x.key === 'k-1');
  const run = await p.locator('.run').innerText();
  check(`${TAURI ? '[tauri] ' : ''}run sends the synced URL`, Boolean(listed), JSON.stringify(hits) + ' ' + run.replace(/\n/g, ' | ').slice(0, 400));

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
