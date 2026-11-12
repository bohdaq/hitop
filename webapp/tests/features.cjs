// Checks for query parameters, auth presets, environments and assertions,
// against a running hitop-server. Requests go through the proxy, so the test
// reads what was sent from the /api/proxy request bodies.
//   BASE=http://localhost:7879 node tests/features.cjs
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

(async () => {
  const h = await launch({ viewport: { width: 1280, height: 900 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const sent = h.sent;
  const last = () => sent[sent.length - 1];
  const header = (req, name) => req?.headers.find((h) => h[0].toLowerCase() === name.toLowerCase())?.[1];
  const send = async () => {
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => document.querySelector('.response-meta') && !document.querySelector('[aria-busy="true"]'), null, { timeout: 20000 });
    await settle(p);
  };
  const tab = async () => (await h.stored.tabs()).tab;
  const section = (name) => p.getByRole('tab', { name: new RegExp(`^${name}`) }).click();

  await p.goto(h.url, { waitUntil: 'networkidle' });
  // A new request to work on (FV1: the first visit opens the sample's List posts).
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);

  // Query parameters follow the URL both ways
  await p.getByLabel('URL').fill('https://jsonplaceholder.typicode.com/posts?userId=1');
  await section('Params');
  await settle(p);
  const firstKey = await p.locator('.param-row').evaluateAll((rows) => rows.map((r) => [...r.querySelectorAll('lit-material-text-field')].map((f) => f.value).join('=')).join('&'));
  check('params parsed from the URL', firstKey === 'userId=1', firstKey);
  await p.getByRole('button', { name: 'Add parameter' }).click();
  await p.keyboard.type('_limit', { delay: 15 });
  await p.keyboard.press('Tab');
  await p.keyboard.type('2', { delay: 15 });
  await settle(p);
  check('params written to the URL', (await tab()).url === 'https://jsonplaceholder.typicode.com/posts?userId=1&_limit=2', (await tab()).url);

  // Bearer auth with a variable from the active environment
  await section('Auth');
  await p.getByRole('combobox', { name: 'Type' }).click();
  await p.getByRole('option', { name: 'Bearer token' }).click();
  await p.getByLabel('Token').fill('${token}');

  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await p.getByRole('button', { name: 'New environment' }).first().click();
  await p.waitForTimeout(300);
  await p.keyboard.type('Staging');
  await p.keyboard.press('Enter');
  await settle(p);
  await p.getByRole('button', { name: 'Add variable' }).click();
  await p.keyboard.type('token', { delay: 15 });
  await p.keyboard.press('Tab');
  await p.keyboard.type('stg-123', { delay: 15 });
  await settle(p);
  const envs = await h.stored.environments();
  check('environment created and active', envs.list[0]?.name === 'Staging' && envs.list[0].variables.token === 'stg-123' && envs.active === envs.list[0].id, JSON.stringify(envs));

  // A secret: masked in the editor, kept in localStorage, left out of the export
  check('secrets note next to the switch', (await p.locator('.env-secrets').innerText()).includes('never written to exports'));
  await p.getByRole('button', { name: 'Add secret' }).click();
  await p.keyboard.type('apiSecret', { delay: 15 });
  await p.keyboard.press('Tab');
  await p.keyboard.type('sec-456', { delay: 15 });
  await settle(p);
  const secretInput = p.locator('.env-secrets input[autocomplete="off"]').last();
  check('secret value masked', (await secretInput.getAttribute('type')) === 'password', await secretInput.getAttribute('type'));
  await p.getByRole('switch', { name: 'Show secret values' }).click();
  await settle(p);
  check('secret value shown on request', (await secretInput.getAttribute('type')) === 'text', await secretInput.getAttribute('type'));
  const stored = (await h.stored.environments()).list[0];
  if (h.tauri) {
    const acc = `global/Staging/apiSecret`;
    check('[tauri] secret kept in the keychain, not in environments.json', h.mock.keychain.get(acc) === 'sec-456' && !('apiSecret' in stored.variables) && JSON.stringify(stored.secrets) === '["apiSecret"]' && stored.variables.token === 'stg-123', JSON.stringify(stored));
  } else {
    check('secret kept in this browser', stored.variables.apiSecret === 'sec-456' && JSON.stringify(stored.secrets) === '["apiSecret"]' && stored.variables.token === 'stg-123', JSON.stringify(stored));
  }
  const exported = (await h.download(() => p.locator('.page-header').getByRole('button', { name: 'Export' }).click())).toString('utf8');
  check('export keeps the secret name, not its value', !exported.includes('sec-456') && exported.includes('"apiSecret"') && exported.includes('stg-123'), exported.replace(/\s+/g, ' '));

  // Back to the request (Mod+L: the URL field of the open tab)
  await p.keyboard.press('Control+KeyL');
  await settle(p);
  await p.getByRole('tab', { name: /Script after/ }).click();
  await p.locator('.cm-content').click();
  await p.keyboard.type("assert.status(200); assert.bodyContains('userId'); assert.header('content-type', 'json'); assert.responseTime(10000); assert.equal(response.length, 99, 'two posts');", { delay: 2 });
  await settle(p);
  await send();
  const req = last();
  check('bearer token sent with env value', header(req, 'Authorization') === 'Bearer stg-123', header(req, 'Authorization'));
  check('query sent', req.url.endsWith('?userId=1&_limit=2'), req.url);
  const meta = await p.locator('.response-meta').innerText();
  check('tests summary shown', meta.includes('1 of 5 tests failed'), meta.replace(/\n/g, ' '));
  await p.getByRole('tab', { name: /^Tests/ }).click();
  const testsText = await p.locator('.tests').innerText();
  check('failed assertion explained', testsText.includes('two posts') && testsText.includes('got 2'), testsText.replace(/\n/g, ' | '));

  // Basic auth and API key in the query
  await section('Auth');
  await p.getByRole('combobox', { name: 'Type' }).click();
  await p.getByRole('option', { name: 'Basic auth' }).click();
  await p.getByLabel('Username').fill('ada');
  await p.getByLabel('Password').fill('s3cret');
  await send();
  check('basic auth header', header(last(), 'Authorization') === 'Basic ' + Buffer.from('ada:s3cret').toString('base64'), header(last(), 'Authorization'));
  await p.getByRole('combobox', { name: 'Type' }).click();
  await p.getByRole('option', { name: 'API key' }).click();
  await p.getByLabel('Value', { exact: true }).fill('k-1');
  await p.getByRole('combobox', { name: 'Send as' }).click();
  await p.getByRole('option', { name: 'Query parameter' }).click();
  await send();
  check('API key as query parameter', last().url.includes('X-API-Key=k-1') && !header(last(), 'Authorization'), last().url);

  // Switching the environment off removes the override
  await p.getByRole('combobox', { name: 'Environment' }).click();
  await p.getByRole('option', { name: 'No environment' }).click();
  await settle(p);
  check('environment can be turned off', (await h.stored.environments()).active === null);

  // Auth is saved with the request, and survives a Postman export and import
  await p.getByRole('button', { name: 'Save request' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'Posts with key'));
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await settle(p);
  const saved = (await h.stored.collections()).flatMap((c) => c.requests).find((r) => r.name === 'Posts with key');
  check('auth saved with only its own fields', saved?.auth?.type === 'apikey' && saved.auth.value === 'k-1' && !('password' in saved.auth), JSON.stringify(saved?.auth));
  await p.locator('aside').getByRole('button', { name: 'Export' }).click();
  await p.waitForTimeout(300);
  await p.getByRole('combobox', { name: 'Format' }).click();
  await p.getByRole('option', { name: 'Postman v2.1' }).click();
  await p.waitForTimeout(300);
  const postman = await p.locator('#dialog lit-material-code-block').innerText();
  const pm = JSON.parse(postman);
  const item = pm.flatMap((c) => c.item).find((i) => i.name === 'Posts with key');
  check('Postman export keeps auth and header names', item?.request.auth?.type === 'apikey' && pm.every((c) => c.item.every((i) => i.request.header.every((h) => h.key))), JSON.stringify(item?.request.auth));
  await p.keyboard.press('Escape');
  await p.waitForTimeout(300);
  await p.locator('aside').getByRole('button', { name: 'Import', exact: true }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog #json').evaluate((el, text) => (el.value = text), postman);
  await p.locator('#dialog').getByRole('radio', { name: /Replace all/ }).check();
  await p.locator('#dialog').getByRole('button', { name: /import/i }).click();
  // Replacing collections that have requests asks first.
  await p.locator('lit-material-confirm-dialog[open]').getByRole('button', { name: 'Replace', exact: true }).click();
  await settle(p);
  const reimported = (await h.stored.collections()).flatMap((c) => c.requests).find((r) => r.name === 'Posts with key');
  check('Postman import restores auth', reimported?.auth?.type === 'apikey' && reimported.auth.in === 'query', JSON.stringify(reimported?.auth));

  // A failed assertion fails the collection run
  const col = (await h.stored.collections()).find((c) => c.requests.some((r) => r.name === 'Posts with key')).name;
  await p.locator('aside').getByRole('treeitem', { name: col, exact: true }).first().click();
  await p.getByRole('button', { name: /Run collection/ }).click();
  await p.locator('.run h2', { hasText: 'Last run' }).waitFor({ timeout: 30000 });
  const run = await p.locator('.run').innerText();
  check('collection run reports the failed assertion', run.includes('4 of 5 tests passed') && run.includes('two posts'), run.replace(/\n/g, ' | ').slice(0, 200));

  // With a secret in the active environment, the run's history masks its value
  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await p.getByRole('button', { name: 'Use this environment' }).click();
  await p.getByRole('button', { name: 'Add secret' }).click();
  await p.keyboard.type('key', { delay: 15 });
  await p.keyboard.press('Tab');
  await p.keyboard.type('k-1', { delay: 15 });
  await settle(p);
  await p.locator('aside').getByRole('treeitem', { name: col, exact: true }).first().click();
  const before = (await h.stored.history()).length;
  await p.getByRole('button', { name: /Run collection/ }).click();
  for (let i = 0; i < 120 && (await h.stored.history()).length <= before; i++) await p.waitForTimeout(250);
  await settle(p);
  const runItem = (await h.stored.history()).filter((x) => x.isCollectionRun).pop();
  check('run history masks secret values', runItem.url.includes('X-API-Key=••••') && !JSON.stringify(runItem).includes('k-1'), runItem.url);

  check('no page errors', errors.length === 0, errors.join(' || '));
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message);
  process.exit(1);
});
