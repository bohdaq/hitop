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

  // Digest: the 401 challenge answered once (httpbin, through the proxy on the web).
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.show('request');
    s.updateTab({ method: 'GET', url: 'https://httpbin.org/digest-auth/auth/ada/sesame', auth: { type: 'digest', username: 'ada', password: 'sesame' } });
  });
  await settle(p);
  await p.getByRole('button', { name: 'Send' }).click();
  await p.locator('.status-chip').first().waitFor({ timeout: 30000 });
  const digest = await p.evaluate(() => { const t = document.querySelector('hitop-app').store.tab; return { status: t.statusCode, body: String(t.response) }; });
  check('Digest auth answers the challenge', digest.status === 200 && digest.body.includes('"authenticated": true'), JSON.stringify(digest).slice(0, 200));

  // OAuth 2.0 authorization code with PKCE (web): the sign-in pop-up, the callback page, the token
  // trade and the API call through the proxy (all mocked here); a second send needs no sign-in.
  if (!TAURI) {
    let signIns = 0;
    let trade = null;
    await h.ctx.route('https://idp.e2e-oauth.test/authorize**', (route) => {
      signIns += 1;
      const q = new URL(route.request().url()).searchParams;
      const ok = q.get('code_challenge_method') === 'S256' && q.get('code_challenge') && q.get('client_id') === 'web-app';
      route.fulfill({ status: 302, headers: { Location: `${q.get('redirect_uri')}?code=${ok ? 'w3b-code' : 'bad'}&state=${q.get('state')}` } });
    });
    await p.route('**/api/proxy', async (route) => {
      const req = JSON.parse(route.request().postData() || '{}');
      if (req.url === 'https://idp.e2e-oauth.test/token') {
        trade = req.body;
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 200, statusText: 'OK', headers: [['content-type', 'application/json']], body: JSON.stringify({ access_token: 'web-user-tok', expires_in: 3600 }), bodyEncoding: 'utf8', truncated: false, timeMs: 1, url: req.url }) });
      }
      if (req.url === 'https://api.e2e-oauth.test/me') {
        const auth = (req.headers || []).find(([k]) => k.toLowerCase() === 'authorization')?.[1];
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: auth === 'Bearer web-user-tok' ? 200 : 401, statusText: 'OK', headers: [['content-type', 'application/json']], body: '{"me":"ada"}', bodyEncoding: 'utf8', truncated: false, timeMs: 1, url: req.url }) });
      }
      return route.continue();
    });
    await p.evaluate(() => {
      const s = document.querySelector('hitop-app').store;
      s.show('request');
      s.updateTab({ method: 'GET', url: 'https://api.e2e-oauth.test/me', auth: { type: 'oauth2', grant: 'authorization_code', authUrl: 'https://idp.e2e-oauth.test/authorize', tokenUrl: 'https://idp.e2e-oauth.test/token', clientId: 'web-app', scope: 'profile' } });
    });
    await settle(p);
    const status = () => p.evaluate(() => document.querySelector('hitop-app').store.tab.statusCode);
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => document.querySelector('hitop-app').store.tab.statusCode != null, null, { timeout: 20000 });
    check('authorization code: signed in in a pop-up, the code traded with the PKCE verifier, the API answered', (await status()) === 200 && signIns === 1 && /grant_type=authorization_code&code=w3b-code&redirect_uri=.*oauth-callback\.html&code_verifier=[A-Za-z0-9_-]{43}&client_id=web-app/.test(trade || ''), `${await status()} ${signIns} ${trade}`);
    await p.getByRole('button', { name: 'Send' }).click();
    await settle(p, 1000);
    check('...and the token is reused: no second sign-in', signIns === 1 && (await status()) === 200);
    await p.unroute('**/api/proxy');
  }

  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
