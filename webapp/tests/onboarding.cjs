// First run and search: a fresh app offers a sample collection and Postman import next to the
// empty response; the sample runs (web: through the proxy to JSONPlaceholder) with its tests and
// a value passed from one request to the next; quick open (Mod+P) finds collections,
// environments by a variable name and history.
//   BASE=http://localhost:7879 node tests/onboarding.cjs   (HITOP_MODE=tauri for the desktop build)
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

(async () => {
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await settle(p);

  const sample = p.getByRole('button', { name: 'Try a sample collection' });
  check('a fresh app offers a sample and Postman import', (await sample.isVisible()) && (await p.getByRole('button', { name: 'Import from Postman' }).isVisible()));
  await sample.click();
  await settle(p);
  check('the sample opens as a collection', (await p.locator('h1').innerText()).includes('Sample: JSONPlaceholder'));
  if (!TAURI) {
    await p.getByRole('button', { name: /Run collection/ }).click();
    await p.locator('.run h2', { hasText: 'Last run' }).waitFor({ timeout: 30000 });
    const run = await p.locator('.run-list').innerText();
    check('the sample runs with its tests passing, a value passed on', (await p.locator('.run-list .status-chip.ok').count()) === 3 && !run.includes('Failed'), run.replace(/\n/g, ' | ').slice(0, 400));
  }
  // Its docs page: Share, File, Download docs page.
  if (!TAURI) {
    await p.locator('hitop-collection').getByRole('button', { name: 'Share', exact: true }).click();
    await p.locator('#dialog').getByRole('tab', { name: /File/ }).click();
    const [download] = await Promise.all([p.waitForEvent('download'), p.locator('#dialog').getByRole('button', { name: /Download docs page/ }).click()]);
    const page = require('fs').readFileSync(await download.path(), 'utf8');
    check('the docs page lists the requests and the variables, no script', download.suggestedFilename() === 'sample-jsonplaceholder-docs.html' && page.includes('Get the first post') && page.includes('${baseUrl}') && !page.includes('<script'), download.suggestedFilename());
    await p.locator('#dialog .dialog-actions').getByRole('button', { name: 'Close' }).click();
    await settle(p);
  }
  check('...and the offer is gone once there is something of your own', (await p.evaluate(() => document.querySelector('hitop-app').store.isFresh)) === false);

  // Quick open: a collection, an environment by its variable name, a URL from history.
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.environments = [...s.environments, { id: 'env-qa', name: 'qa', variables: { tenantSlug: 'acme' } }];
    s.history = [...s.history, { id: 'h1', method: 'GET', url: 'https://history.e2e.test/only-here', statusCode: 204, timestamp: Date.now() }];
    s.emit();
  });
  const quick = async (text) => {
    await p.keyboard.press(process.platform === 'darwin' ? 'Meta+p' : 'Control+p');
    await p.locator('#quick-input').waitFor();
    await p.locator('#quick-input').fill(text);
    await settle(p, 200);
    const first = (await p.locator('#quick-list [role="option"]').first().innerText().catch(() => '')).replace(/\n/g, ' ');
    await p.keyboard.press('Escape');
    await settle(p, 200);
    return first;
  };
  check('quick open finds a collection', (await quick('sample json')).includes('Sample: JSONPlaceholder'), await quick('sample json'));
  check('...an environment by a variable name', (await quick('tenantslug')).includes('qa'), await quick('tenantslug'));
  check('...and a URL from history', (await quick('only-here')).includes('history.e2e.test'), await quick('only-here'));

  // AWS Signature v4: the request goes out signed (web: as the proxy receives it).
  if (!TAURI) {
    await p.evaluate(() => {
      const s = document.querySelector('hitop-app').store;
      s.show('request');
      s.updateTab({ method: 'GET', url: 'https://api.e2e-aws.test/orders?b=2&a=1', auth: { type: 'awsv4', accessKey: 'AKIDEXAMPLE', secretKey: 'secret-e2e', region: 'eu-west-1', service: 'execute-api' } });
    });
    await settle(p);
    const before = h.sent.length;
    await p.getByRole('button', { name: 'Send' }).click();
    await settle(p, 1500);
    const sent = h.sent.slice(before).find((x) => x.url.includes('e2e-aws.test'));
    const auth = sent?.headers.find(([k]) => k.toLowerCase() === 'authorization')?.[1] ?? '';
    check('AWS Signature v4 signs the request', /^AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE\/\d{8}\/eu-west-1\/execute-api\/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=[0-9a-f]{64}$/.test(auth) && !JSON.stringify(sent).includes('secret-e2e'), auth || JSON.stringify(sent));
  }

  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
