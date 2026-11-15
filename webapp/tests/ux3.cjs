// The 2026-11-15 UX review fixes (reviews/2026-11-15-ux-review.md): redirects and their cookies,
// the request's Settings tab, inherited auth and headers, path variables, Format, Console, the JSON
// filter, the "Not sent" card, the URL mirror, the phone layout and the History preview tab.
//   BASE=http://localhost:7879 node tests/ux3.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker, inApp, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

// Echoes what it got: method, path, headers. CORS open, so the web build calls it directly.
function server() {
  const s = http.createServer((req, res) => {
    const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': '*', 'access-control-allow-methods': '*', 'access-control-expose-headers': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    const u = new URL(req.url, 'http://x');
    res.writeHead(200, { ...cors, 'content-type': 'application/json' }).end(JSON.stringify({ method: req.method, path: u.pathname, headers: req.headers, items: [{ id: 1 }, { id: 2 }] }));
  });
  return new Promise((resolve) => s.listen(0, '127.0.0.1', () => resolve(s)));
}

(async () => {
  const srv = await server();
  const base = `http://127.0.0.1:${srv.address().port}`;
  const h = await launch({ results, viewport: { width: 1440, height: 900 } });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await settle(p, 800);
  const store = (body, arg) => inApp(p, body, arg);
  const tabUrl = (url, extra = {}) => store('app.store.updateTab({ url: a.url, ...a.extra }); app.store.emit();', { url, extra });
  const send = async () => {
    await p.locator('hitop-request .send').click();
    await p.locator('hitop-request .response[data-state="done"], hitop-request .response[data-state="error"], hitop-request .response[data-state="not-sent"]').first().waitFor({ timeout: 15000 });
    await settle(p);
  };
  const echo = async () => JSON.parse((await store('return app.store.rawBodies.get(app.store.tab.id) ?? app.store.tab.response;')).value || '{}');

  // A new request in My requests to work in.
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);

  // --- Inherited auth and headers: the collection's go out with a request set to Inherit ---
  const colId = (await store('return app.store.tab.loadedCollectionId;')).value;
  await store('app.store.setDefaults(a.id, "", { auth: { type: "bearer", token: "col-token" }, headers: [{ name: "X-App", value: "hitop" }] });', { id: colId });
  await tabUrl(`${base}/users/:id`, { pathVariables: [{ key: 'id', value: 'a b' }] });
  await send();
  let got = await echo();
  check('inherited header is sent', got.headers?.['x-app'] === 'hitop', JSON.stringify(got.headers));
  check('inherited auth is sent (Inherit is the default)', got.headers?.authorization === 'Bearer col-token', got.headers?.authorization);
  check('path variables are filled in and encoded', got.path === '/users/a%20b', got.path);
  await p.locator('hitop-request .sub-tabs').first().locator('lit-material-tab', { hasText: 'Auth' }).click();
  await settle(p);
  const note = await p.locator('hitop-request .inherit-note').first().innerText().catch(() => '');
  check('the Auth tab says where the inherited auth comes from', note.includes('Bearer token') && note.includes('the collection'), note);
  await store('app.store.updateTab({ auth: { type: "none" } });');
  await send();
  got = await echo();
  check('"No auth" wins over the collection', !got.headers?.authorization, got.headers?.authorization);
  await store('app.store.setDefaults(a.id, "", { auth: { type: "none" }, headers: [] }); app.store.updateTab({ auth: { type: "inherit" } });', { id: colId });

  // --- Console and a script that throws keeps its results and names the line ---
  await tabUrl(`${base}/json`, { pathVariables: [], postRequestScript: 'pm.test("ok", () => pm.response.to.have.status(200));\nconsole.log("seen", pm.response.json().path);\nfoo.bar();' });
  await send();
  const tests = (await store('return app.store.tab.tests;')).value;
  check('a throwing script keeps the tests before it', tests?.[0]?.pass === true && tests.length === 2, JSON.stringify(tests));
  // The web app's scripts give the line; the desktop's engine (boa) names the error only.
  check('the error says what failed', TAURI ? /ReferenceError: foo is not defined/.test(tests?.[1]?.detail ?? '') : /line 3, column 1/.test(tests?.[1]?.detail ?? ''), tests?.[1]?.detail);
  const consoleTab = p.locator('hitop-request .response lit-material-tab', { hasText: 'Console' });
  check('a Console tab shows what console.log printed', (await consoleTab.count()) === 1);
  await consoleTab.click();
  await settle(p);
  const line = await p.locator('hitop-request .console-line').first().innerText().catch(() => '');
  check('the console line is there', line.includes('seen /json'), line);
  await store('app.store.updateTab({ postRequestScript: "" });');

  // --- JSON filter ---
  await send();
  await p.locator('hitop-request .response lit-material-tab', { hasText: 'Body' }).click();
  await settle(p);
  await p.locator('hitop-request .filter-input').fill('$.items[*].id');
  await settle(p);
  const filterCount = await p.locator('hitop-request .json-filter .find-count').innerText().catch(() => '');
  const filtered = await p.locator('hitop-request .response-body').innerText().catch(() => '');
  check('the JSON filter narrows the body', filterCount === '2 matches' && filtered.replace(/\s/g, '').includes('[1,2]'), `${filterCount} ${filtered.slice(0, 80)}`);
  await p.locator('hitop-request .filter-input').fill('');

  // --- Format a JSON body with a variable in it ---
  await store('app.store.updateTab({ method: "POST", requestBody: \'{"a":1,"b":${n}}\' });');
  await p.locator('hitop-request .sub-tabs').first().locator('lit-material-tab', { hasText: 'Body' }).click();
  await settle(p);
  await p.locator('hitop-request .format-body').click();
  await settle(p);
  const body = (await store('return app.store.tab.requestBody;')).value;
  check('Format re-indents JSON with a variable in it', body === '{\n  "a": 1,\n  "b": ${n}\n}', JSON.stringify(body));
  await store('app.store.updateTab({ method: "GET", requestBody: "" });');

  // --- The "Not sent" card names the variable and offers to define it ---
  await tabUrl(`${base}/\${nowhere}`);
  await send();
  const card = await p.locator('hitop-request .response[data-state="not-sent"]').innerText().catch(() => '');
  check('an undefined variable gives a card that stays', card.includes('${nowhere}') && card.includes('Define'), card.replace(/\n/g, ' '));
  await p.locator('hitop-request .not-sent-actions lit-material-button').first().click();
  await settle(p);
  const vars = (await store('return app.store.collection(a).variables;', colId)).value;
  check('Define adds it to the collection and opens Variables', vars && 'nowhere' in vars && (await store('return app.store.view.kind;')).value === 'variables', JSON.stringify(vars));

  // --- Pages keep the way back; Esc in a field first leaves the field ---
  check('a page shows Back to the request', (await p.locator('.page-back-bar .back-to-request').count()) === 1);
  await p.locator('hitop-variables lit-material-key-value-editor input').first().focus().catch(() => {});
  await p.keyboard.press('Escape');
  await settle(p);
  const still = (await store('return app.store.view.kind;')).value;
  await p.keyboard.press('Escape');
  await settle(p);
  const back = (await store('return app.store.view.kind;')).value;
  check('Esc in a field leaves the field, the next Esc goes back', still === 'variables' && back === 'request', `${still} → ${back}`);

  // --- Follow redirects off goes out with the request (web: the proxy's envelope) ---
  if (!TAURI) {
    await p.route('**/api/proxy', async (route) => {
      const req = JSON.parse(route.request().postData());
      const envelope = req.followRedirects === false
        ? { status: 302, statusText: 'Found', headers: [['location', 'https://login.example.com/home'], ['set-cookie', 'sid=1; Path=/']], body: '', bodyEncoding: 'utf8', truncated: false, timeMs: 5, url: req.url }
        : { status: 200, statusText: 'OK', headers: [['content-type', 'application/json']], body: '{"home":true}', bodyEncoding: 'utf8', truncated: false, timeMs: 9, url: 'https://login.example.com/home', redirects: [{ status: 302, url: req.url, location: 'https://login.example.com/home', setCookies: ['sid=abc; Path=/'] }] };
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(envelope) });
    });
    await store('app.store.setCookieJarEnabled(true); app.store.setCookies([]);');
    await tabUrl('https://login.example.com/login');
    await send();
    const chip = await p.locator('hitop-request .redirect-chip').innerText().catch(() => '');
    check('a redirect shows as a chip', chip.includes('1 redirect'), chip);
    await p.locator('hitop-request .redirect-chip').click();
    await settle(p);
    const chain = await p.locator('hitop-request .redirect-chain').innerText().catch(() => '');
    check('the Redirects tab lists the hop and its cookie', chain.includes('302') && chain.includes('login.example.com/home') && chain.includes('sid'), chain.replace(/\n/g, ' '));
    const jar = (await store('return app.store.cookies;')).value;
    check("a hop's cookie goes in the jar", jar?.some((c) => c.name === 'sid' && c.value === 'abc' && c.domain === 'login.example.com'), JSON.stringify(jar));
    await p.locator('hitop-request .sub-tabs').first().locator('lit-material-tab', { hasText: 'Settings' }).click();
    await settle(p);
    await p.locator('hitop-request .request-settings lit-material-switch').first().click();
    await settle(p);
    await send();
    const last = h.sent.at(-1);
    check('Follow redirects off is sent to the proxy', last?.followRedirects === false, JSON.stringify(last));
    check('the redirect itself is the response', (await store('return app.store.tab.statusCode;')).value === 302);
    const cookieTab = await p.locator('hitop-request .response lit-material-tab', { hasText: 'Cookies' }).count();
    check('a response that sets cookies has a Cookies tab', cookieTab === 1);
    await p.unroute('**/api/proxy');
    await store('app.store.updateTab({ settings: undefined });');

    // A URL without a scheme but with a space in the path gets https://.
    await tabUrl('example.com/a b?q=two words');
    await p.route('**/api/proxy', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 200, statusText: 'OK', headers: [], body: '', bodyEncoding: 'utf8', truncated: false, timeMs: 1, url: 'https://example.com/' }) }));
    await send();
    check('a scheme-less URL with a space is sent with https://', h.sent.at(-1)?.url?.startsWith('https://example.com/a'), h.sent.at(-1)?.url);
    await p.unroute('**/api/proxy');
  }

  // --- The URL mirror lines up with the input, and Tab from the URL opens no peek ---
  await store('app.store.setVariables(a, { ...app.store.collection(a).variables, baseUrl: "https://x.dev" });', colId);
  await tabUrl('${baseUrl}/posts?_limit=3&x=${baseUrl}');
  await settle(p, 600);
  const drift = await p.evaluate(() => {
    const input = document.querySelector('hitop-request .url')?.shadowRoot?.querySelector('input');
    const mirror = document.querySelector('hitop-request .url-mirror-text');
    const ctx = document.createElement('canvas').getContext('2d');
    const cs = getComputedStyle(input);
    ctx.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
    return Math.abs(mirror.getBoundingClientRect().width - ctx.measureText(input.value).width);
  });
  check('the URL mirror is as wide as the text (no drift per variable)', drift < 1.5, `${drift.toFixed(2)} px`);
  await p.locator('hitop-request .url').click();
  await p.keyboard.press('Tab');
  await settle(p);
  const peekOpen = await p.evaluate(() => Boolean(document.querySelector('hitop-request .var-peek')?.open));
  check('Tab from the URL opens no variable peek', !peekOpen);

  // --- History opens into one preview tab ---
  const tabsBefore = (await store('return app.store.tabs.length;')).value;
  await store('const h = app.store.history; app.store.openHistoryItem(h[h.length - 1]); app.store.openHistoryItem(h[h.length - 2]);');
  await settle(p);
  const tabsAfter = (await store('return app.store.tabs.length;')).value;
  check('opening two History entries uses one preview tab', tabsAfter === tabsBefore + 1, `${tabsBefore} → ${tabsAfter}`);

  // --- Phone: the last editor tab doesn't shift the workspace sideways ---
  await p.setViewportSize({ width: 390, height: 844 });
  await tabUrl(`${base}/json`);
  await settle(p, 600);
  await p.locator('hitop-request .sub-tabs').first().locator('lit-material-tab').last().click();
  await settle(p, 600);
  const shift = await p.evaluate(() => {
    const m = document.querySelector('main.main');
    return { left: m.scrollLeft, over: m.scrollWidth - m.clientWidth, page: document.documentElement.scrollWidth - innerWidth };
  });
  check('phone: tapping the last editor tab moves nothing sideways', shift.left === 0 && shift.page <= 0, JSON.stringify(shift));
  check('phone: the environment is in the top bar', await p.locator('.env-chip-phone').isVisible());

  check('no page errors', errors.length === 0, errors.join(' | '));
  srv.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
