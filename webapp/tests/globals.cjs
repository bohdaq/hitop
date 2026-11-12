// The globals scope on the Environments page: values reach requests below
// the environment, secret globals are masked and left out of the export,
// Postman globals import into the globals, and pm.globals in scripts. Local
// Node server only.
//   BASE=http://localhost:7879 node tests/globals.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

// Echoes the query and headers as JSON, with CORS for the page.
function api() {
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    const url = new URL(req.url, 'http://x');
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify({ path: url.pathname, query: Object.fromEntries(url.searchParams), auth: req.headers.authorization ?? null }));
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

(async () => {
  const server = await api();
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const send = async () => {
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => document.querySelector('.response-meta, .error-text') && !document.querySelector('[aria-busy="true"]'), null, { timeout: 30000 });
    await settle(p);
  };
  const responseJson = async () => JSON.parse(await p.locator('.response-body').evaluate((el) => el.textContent.replace(/^JSON\s*(Copy)?/, '')));
  const type = async (text) => p.keyboard.type(text, { delay: 10 });
  const storedGlobals = async () => (TAURI ? (await settle(p), JSON.parse(h.mock.read('globals.json', '{}'))) : p.evaluate(() => JSON.parse(localStorage.getItem('hitop_globals') || 'null')));

  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await settle(p);
  check('Globals section on the Environments page', (await p.locator('section.globals h2').innerText()) === 'Globals');

  // Globals: base, g, k; a secret global hidden.
  for (const [k, v] of [['base', base], ['g', 'G-val'], ['k', 'G-k']]) {
    await p.getByRole('button', { name: 'Add global' }).click();
    await type(k);
    await p.keyboard.press('Tab');
    await type(v);
    await settle(p);
  }
  await p.getByRole('button', { name: 'Add a secret global' }).click();
  await type('hidden');
  await p.keyboard.press('Tab');
  await type('g-s3cret');
  await settle(p);
  const secretInput = p.locator('lit-material-key-value-editor[aria-label="Secret globals"] input[autocomplete="off"]').last();
  check('secret global masked', (await secretInput.getAttribute('type')) === 'password', await secretInput.getAttribute('type'));
  const stored = await storedGlobals();
  if (TAURI) {
    check('[tauri] globals in globals.json, the secret value in the keychain', stored.variables?.g === 'G-val' && !('hidden' in (stored.variables || {})) && JSON.stringify(stored.secrets) === '["hidden"]' && h.mock.keychain.get('global/global/hidden') === 'g-s3cret', JSON.stringify(stored));
  } else {
    check('globals kept in this browser', stored?.variables?.g === 'G-val' && stored.variables.hidden === 'g-s3cret' && JSON.stringify(stored.secrets) === '["hidden"]', JSON.stringify(stored));
  }
  const exported = (await h.download(() => p.getByRole('button', { name: 'Export globals' }).click())).toString('utf8');
  check('globals export keeps the secret name, not its value', !exported.includes('g-s3cret') && exported.includes('"hidden"') && exported.includes('G-val'), exported.replace(/\s+/g, ' '));

  // An environment with k: it wins over the global.
  await p.getByRole('button', { name: 'New environment' }).first().click();
  await p.waitForTimeout(300);
  await type('Staging');
  await p.keyboard.press('Enter');
  await settle(p);
  await p.getByRole('button', { name: 'Add variable' }).click();
  await type('k');
  await p.keyboard.press('Tab');
  await type('E-k');
  await settle(p);

  // Postman globals through the environments import go to the globals.
  await p.locator('main .page-header').getByRole('button', { name: 'Import', exact: true }).click();
  await p.waitForTimeout(300);
  const pg = { name: 'Workspace Globals', _postman_variable_scope: 'globals', values: [{ key: 'pmg', value: 'from-postman', enabled: true }] };
  await p.locator('#dialog #json').evaluate((el, text) => (el.value = text), JSON.stringify(pg));
  await p.locator('#dialog').getByRole('button', { name: /import/i }).click();
  await settle(p);
  const page = await p.locator('main').innerText();
  check('Postman globals land in the globals, not a new environment', !page.includes('Workspace Globals') && (await storedGlobals()).variables?.pmg === 'from-postman', page.slice(0, 300));

  // A request with all of them, and a script that sets a global (the sidebar's + adds a new one).
  await p.locator('aside .new-request').click();
  await p.getByLabel('URL').fill('${base}/echo?g=${g}&k=${k}&hidden=${hidden}&pmg=${pmg}');
  await p.getByRole('tab', { name: /Script after/ }).click();
  await p.locator('.cm-content').click();
  await p.keyboard.type("pm.globals.set('fromScript', 'yes'); pm.test('globals scope', () => { pm.expect(pm.globals.get('k')).to.eql('G-k'); pm.expect(pm.environment.get('k')).to.eql('E-k'); });", { delay: 2 });
  // Close the editor's completion popup, which would take the click on Send.
  await p.keyboard.press('Escape');
  await settle(p);
  await send();
  const echo = await responseJson();
  check('globals reach the request, the environment wins', echo.query?.g === 'G-val' && echo.query.k === 'E-k' && echo.query.hidden === 'g-s3cret' && echo.query.pmg === 'from-postman', JSON.stringify(echo));
  const meta = await p.locator('.response-meta').innerText();
  check('pm.globals reads the globals scope', meta.includes('1/1 passed'), meta.replace(/\n/g, ' '));
  await p.getByLabel('URL').fill('${base}/again?s=${fromScript}');
  await p.getByRole('tab', { name: /Script after/ }).click();
  await p.locator('.cm-content').click();
  await p.keyboard.press('ControlOrMeta+a');
  await p.keyboard.press('Delete');
  await settle(p);
  await send();
  check('a global a script set reaches the next request', (await responseJson()).query?.s === 'yes');

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
