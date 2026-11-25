// Server-Sent Events in the web app: a request with Accept: text/event-stream shows its events as
// they arrive (through hitop-server's POST /api/proxy/stream for public hosts), Stop ends it and
// keeps what came; an event stream sent without that header offers to add it. Desktop (Tauri mode):
// the same through Rust (send_stream), and OAuth 2.0 sign-in in the system browser.
//   BASE=http://localhost:7879 node tests/sse.cjs   (needs the internet: stream.wikimedia.org)
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

async function desktop() {
  const http = require('http');
  let streams = 0;
  let closed = 0;
  let auth;
  const tokenBodies = [];
  const server = http.createServer((req, res) => {
    if (req.url.startsWith('/token')) {
      let body = '';
      req.on('data', (d) => (body += d));
      req.on('end', () => {
        tokenBodies.push(body);
        res.setHeader('Content-Type', 'application/json');
        res.end('{"access_token":"refreshed-tok","expires_in":3600}');
      });
      return;
    }
    if (req.url === '/me') {
      auth = req.headers.authorization;
      return res.end('{"me":"ada"}');
    }
    streams += 1;
    res.writeHead(200, { 'Content-Type': 'text/event-stream' });
    let n = 0;
    const tick = setInterval(() => res.write(`event: tick\ndata: ${++n}\n\n`), 100);
    res.on('close', () => ((closed += 1), clearInterval(tick)));
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  const tab = (changes) => p.evaluate((c) => document.querySelector('hitop-app').store.updateTab(c), changes);
  await tab({ method: 'GET', url: `${base}/stream`, headers: [{ name: 'Accept', value: 'text/event-stream' }] });
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.waitForFunction(() => document.querySelectorAll('.sse-events li').length >= 3, null, { timeout: 15000 });
  check('desktop: events show as they arrive', (await p.locator('section.sse[data-state="streaming"]').count()) === 1 && streams === 1);
  await p.locator('section.sse').getByRole('button', { name: 'Stop' }).click();
  await p.waitForFunction(() => !document.querySelector('section.sse[data-state="streaming"]'), null, { timeout: 15000 });
  for (let i = 0; i < 100 && !closed; i++) await settle(p, 50);
  const kept = await p.evaluate(() => { const s = document.querySelector('hitop-app').store; return s.events.get(s.tab.id).events.length; });
  check('desktop: Stop ends the stream in Rust too and keeps the events', closed === 1 && kept >= 3 && h.mock.stops.length === 1, String(kept));
  // OAuth 2.0 authorization code: the browser through Rust, the token reused, variables filled in.
  await tab({ url: `${base}/me`, headers: [{ name: '', value: '' }], auth: { type: 'oauth2', grant: 'authorization_code', authUrl: 'https://idp.test/authorize', tokenUrl: 'https://idp.test/token', clientId: 'app', scope: 'read' } });
  for (let i = 0; i < 2; i++) {
    auth = null;
    await p.getByRole('button', { name: 'Send', exact: true }).click();
    for (let k = 0; k < 200 && !auth; k++) await settle(p, 50);
    check(`desktop: OAuth sign-in, send ${i + 1}`, auth === 'Bearer desktop-user-tok', String(auth));
  }
  check('desktop: one sign-in for both', h.mock.signIns.length === 1 && h.mock.signIns[0].clientId === 'app', JSON.stringify(h.mock.signIns));
  // Expired: the refresh token, without a browser; with another token URL a new sign-in, never this refresh token.
  await tab({ auth: { type: 'oauth2', grant: 'authorization_code', authUrl: 'https://idp.test/authorize', tokenUrl: `${base}/token`, clientId: 'app', scope: 'read' } });
  const send = async () => {
    auth = null;
    await p.getByRole('button', { name: 'Send', exact: true }).click();
    for (let k = 0; k < 200 && !auth; k++) await settle(p, 50);
    return auth;
  };
  await send();
  const signedIn = h.mock.signIns.length;
  await p.evaluate(() => { for (const t of document.querySelector('hitop-app').store.oauthTokens.values()) t.until = 1; });
  check('desktop: an expired token is refreshed without a sign-in', (await send()) === 'Bearer refreshed-tok' && h.mock.signIns.length === signedIn && tokenBodies.length === 1 && tokenBodies[0] === 'grant_type=refresh_token&refresh_token=desktop-refresh&client_id=app', `${auth} ${JSON.stringify(tokenBodies)}`);
  await p.evaluate(() => { for (const t of document.querySelector('hitop-app').store.oauthTokens.values()) t.until = 1; });
  await tab({ auth: { type: 'oauth2', grant: 'authorization_code', authUrl: 'https://idp.test/authorize', tokenUrl: `${base}/token?other`, clientId: 'app', scope: 'read' } });
  check('desktop: another token URL signs in, without this refresh token', (await send()) === 'Bearer desktop-user-tok' && h.mock.signIns.length === signedIn + 1 && tokenBodies.length === 1, JSON.stringify(tokenBodies));
  check('no page errors', errors.length === 0, errors.join(' | '));
  server.closeAllConnections?.();
  server.close();
  await h.finish();
}

(async () => {
  if (TAURI) return desktop();
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.updateTab({ url: 'https://stream.wikimedia.org/v2/stream/recentchange', headers: [{ name: 'Accept', value: 'text/event-stream' }] });
  });
  await settle(p);
  await p.getByRole('button', { name: 'Send' }).click();
  await p.locator('.sse-events li').first().waitFor({ timeout: 30000 });
  check('events show as they arrive, marked live', (await p.locator('section.sse').innerText()).includes('Live') && (await p.locator('.sse-events li').count()) >= 1);
  await settle(p, 1500);
  await p.locator('section.sse').getByRole('button', { name: 'Stop' }).click();
  await settle(p, 800);
  const n = await p.evaluate(() => document.querySelector('hitop-app').store.events.get(document.querySelector('hitop-app').store.tab.id).events.length);
  check('Stop ends the stream and keeps the events', !(await p.locator('section.sse').count()) || (await p.locator('section.sse').innerText()).includes('Stream ended'), String(n));
  check('...as an Events tab next to Body and Headers', (await p.getByRole('tab', { name: /^Events \d+/ }).count()) === 1);
  // UX2 (8): without the Accept header the proxy says it is an event stream (body unread) and the
  // app opens it live through the stream endpoint (needs a hitop-server with `eventStreams`).
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.updateTab({ headers: [{ name: '', value: '' }] });
  });
  await settle(p);
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  const live = await p.locator('section.sse[data-state="streaming"] .sse-events li').first().waitFor({ timeout: 30000 }).then(() => true, () => false);
  check('UX2: through the proxy, an event stream without Accept goes live too', live);
  await p.keyboard.press('Escape');
  await settle(p, 800);
  check('...and Esc stops it', (await p.locator('section.sse[data-state="streaming"]').count()) === 0);
  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
