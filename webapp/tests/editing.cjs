// Editing checks for the web app against a running hitop-server: typing in
// the header, body, script and variable editors keeps focus and saves every
// character; Enter submits dialogs; a HEAD response still shows its status.
//   BASE=http://localhost:7879 node tests/editing.cjs
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400); // saves are batched every 250 ms

(async () => {
  const h = await launch({ viewport: { width: 1280, height: 900 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  const tab = async () => (await h.stored.tabs()).tab;
  const focusedTag = () => p.evaluate(() => { let a = document.activeElement; while (a?.shadowRoot?.activeElement) a = a.shadowRoot.activeElement; return a?.localName; });

  await p.getByRole('button', { name: 'Add header' }).click();
  await p.keyboard.type('X-Trace-Id', { delay: 20 });
  await p.keyboard.press('Tab');
  await p.keyboard.type('abc-123', { delay: 20 });
  await settle(p);
  let t = await tab();
  check('header typed in full, focus kept', t.headers[0]?.name === 'X-Trace-Id' && t.headers[0]?.value === 'abc-123' && (await focusedTag()) === 'input', JSON.stringify(t.headers));

  await p.getByRole('combobox', { name: 'Method' }).click();
  await p.getByRole('option', { name: 'POST' }).click();
  await p.getByRole('tab', { name: 'Body' }).click();
  await p.locator('.cm-content').click();
  await p.keyboard.type('{"a": 1}', { delay: 15 });
  await settle(p);
  t = await tab();
  check('body typed in full', t.method === 'POST' && t.requestBody === '{"a": 1}', JSON.stringify(t.requestBody));

  await p.getByRole('tab', { name: /Script before/ }).click();
  await p.locator('.cm-content').click();
  await p.keyboard.type("setHeader('X-A', 'b');", { delay: 15 });
  await settle(p);
  t = await tab();
  check('script typed in full', t.preRequestScript === "setHeader('X-A', 'b');");

  await p.getByRole('combobox', { name: 'Method' }).click();
  await p.getByRole('option', { name: 'HEAD' }).click();
  await p.getByLabel('URL').fill('https://example.com/');
  await p.getByRole('button', { name: 'Send' }).click();
  await p.locator('.response-meta').waitFor({ timeout: 20000 });
  check('HEAD shows status and headers', (await p.locator('.response').innerText()).includes('200'));

  await p.locator('aside').getByRole('button', { name: 'Collections' }).click();
  await p.getByRole('button', { name: 'New collection' }).first().click();
  await p.waitForTimeout(300);
  await p.keyboard.type('Enter test', { delay: 15 });
  await p.keyboard.press('Enter');
  await settle(p);
  check('Enter submits a dialog once', ((await p.locator('aside').innerText()).match(/Enter test/g) || []).length === 1);

  await p.locator('aside').getByRole('button', { name: 'Variables' }).first().click();
  await p.getByRole('button', { name: 'Add variable' }).click();
  await p.keyboard.type('baseUrl', { delay: 20 });
  await p.keyboard.press('Tab');
  await p.keyboard.type('https://api.example.com', { delay: 10 });
  await settle(p);
  const vars = (await h.stored.collections())[0].variables;
  check('variable typed in full, focus kept', vars.baseUrl === 'https://api.example.com' && (await focusedTag()) === 'input', JSON.stringify(vars));

  check('no page errors', errors.length === 0, errors.join(' || '));
  await h.finish();
})().catch((e) => { console.log(results.join('\n')); console.error('CRASH', e.message); process.exit(1); });
