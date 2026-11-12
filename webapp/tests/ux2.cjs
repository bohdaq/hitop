// UX2 (reviews/2026-11-12-ux-audit.md, HITOP_NEXT_UI.md UX2): a check per top-10 item, in both
// modes, against a local server (CORS open, so the web build calls it straight from the browser).
//   BASE=http://localhost:7879 node tests/ux2.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker, inApp, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

// /json, /slow (answers after 4 s), /sse (an event every 200 ms, never ends), /png (a 2×1 PNG).
function server() {
  const PNG = Buffer.from('89504e470d0a1a0a0000000d4948445200000002000000010806000000f478d4fa0000000d49444154789c63f8cfc0f01f0005000201a5f5b6e10000000049454e44ae426082', 'hex');
  const s = http.createServer((req, res) => {
    const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': '*', 'access-control-allow-methods': '*', 'access-control-expose-headers': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    const u = new URL(req.url, 'http://x');
    if (u.pathname === '/slow') return setTimeout(() => res.writeHead(200, { ...cors, 'content-type': 'application/json' }).end('{"slow":true}'), 4000);
    if (u.pathname === '/sse') {
      res.writeHead(200, { ...cors, 'content-type': 'text/event-stream', 'cache-control': 'no-store' });
      let n = 0;
      const timer = setInterval(() => res.write(`id: ${++n}\ndata: {"n": ${n}}\n\n`), 200);
      req.on('close', () => clearInterval(timer));
      return;
    }
    if (u.pathname === '/png') return res.writeHead(200, { ...cors, 'content-type': 'image/png' }).end(PNG);
    res.writeHead(200, { ...cors, 'content-type': 'application/json' }).end(JSON.stringify({ ok: true, path: u.pathname }));
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
  const pickMethod = async (name) => {
    await p.locator('hitop-request').getByRole('combobox', { name: 'Method' }).click();
    await p.getByRole('option', { name, exact: true }).click();
    await settle(p);
  };

  // --- (5) First visit: + with the untouched sample open files into My requests ---
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  let cols = await h.stored.collections();
  check('(5) + with the untouched sample open adds to My requests', cols.find((c) => c.name === 'My requests')?.requests.length === 1 && cols.find((c) => c.name.startsWith('Sample'))?.requests.length === 3, cols.map((c) => `${c.name}:${c.requests.length}`).join());

  // --- (1) WebSocket and gRPC addresses are visible (the mirror draws them) ---
  for (const [name, url] of [['WebSocket', 'ws://127.0.0.1:9/feed'], ['gRPC', 'localhost:50051']]) {
    await pickMethod(name);
    await tabUrl(url);
    await settle(p);
    const mirror = await p.locator('hitop-request .url-mirror-text').evaluate((el) => ({ text: el.textContent, color: getComputedStyle(el).color }));
    check(`(1) the ${name} address is drawn (mirror text, not transparent)`, mirror.text === url && !/rgba\(.*,\s*0\)$/.test(mirror.color) && mirror.color !== 'transparent', JSON.stringify(mirror));
  }
  await pickMethod('GET');
  check('(1) back to GET: the HTTP tab has its mirror too', (await p.locator('hitop-request .url-field[data-mirror] .url-mirror').count()) === 1);

  // --- (8) The method menu: ten tags, no scrolling, Server-Sent Events = GET + Accept ---
  await p.locator('hitop-request').getByRole('combobox', { name: 'Method' }).click();
  await settle(p);
  const menu = await p.locator('hitop-request lit-material-select.method').evaluate((el) => {
    const box = el.shadowRoot.querySelector('[part="listbox"]');
    return { n: el.querySelectorAll('lit-material-select-option').length, scroll: box.scrollHeight - box.clientHeight };
  });
  check('(8) the method menu has 10 items and fits without scrolling', menu.n === 10 && menu.scroll <= 1, JSON.stringify(menu));
  check('(8) it offers Server-Sent Events', (await p.getByRole('option', { name: 'Server-Sent Events', exact: true }).count()) === 1);
  await p.getByRole('option', { name: 'Server-Sent Events', exact: true }).click();
  await settle(p);
  let tab = (await h.stored.tabs()).tab;
  check('(8) picking it makes a GET with Accept: text/event-stream, tagged SSE', tab.method === 'GET' && tab.headers.some((x) => x.name === 'Accept' && x.value === 'text/event-stream') && (await p.locator('.tab-strip lit-material-tab[selected] .method-tag').innerText()) === 'SSE', JSON.stringify(tab.headers));
  await pickMethod('GET');
  tab = (await h.stored.tabs()).tab;
  check('(8) back to GET takes the Accept header out', !tab.headers.some((x) => /event-stream/.test(x.value || '')));
  const width = async () => (await p.locator('hitop-request .url-bar .method').boundingBox()).width;
  const wGet = await width();
  await pickMethod('POST');
  check('(R) the method box keeps its width (GET vs POST)', Math.abs((await width()) - wGet) < 1, `${wGet} vs ${await width()}`);
  await pickMethod('GET');

  // (8) An event stream sent without the Accept header still shows live events (web; the
  // desktop has no live stream yet).
  if (!TAURI) {
    await tabUrl(`${base}/sse`, { headers: [{ name: '', value: '' }] });
    await p.getByRole('button', { name: 'Send', exact: true }).click();
    const live = await p.locator('section.sse .sse-events li').first().waitFor({ timeout: 10000 }).then(() => true, () => false);
    check('(8) text/event-stream without Accept switches to the live Events view', live && (await p.locator('section.sse').innerText()).includes('Live'));
    await p.keyboard.press('Escape');
    await settle(p, 600);
    check('(7) Esc stops the stream', (await p.locator('section.sse[data-state="streaming"]').count()) === 0);
  }

  // --- (7) Waiting shows its time; Esc cancels after a click on Send ---
  await tabUrl(`${base}/slow`);
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await settle(p, 300);
  const t1 = await p.locator('.waiting .wait-time').innerText();
  await settle(p, 900);
  const t2 = await p.locator('.waiting .wait-time').innerText();
  check('(7) in flight: "Waiting · n s", ticking', (await p.locator('.waiting').innerText()).includes('Waiting') && parseFloat(t2) > parseFloat(t1), `${t1} → ${t2}`);
  await p.keyboard.press('Escape');
  await settle(p, 600);
  check('(7) Esc cancels it (focus left on the page, not in the editor)', (await p.locator('.response[data-state="cancelled"]').count()) === 1);

  // --- (7) Engine errors as a headline and next step, raw text under Details ---
  const closed = await new Promise((resolve) => { const x = http.createServer(); x.listen(0, '127.0.0.1', () => { const port = x.address().port; x.close(() => resolve(port)); }); });
  await tabUrl(`http://127.0.0.1:${closed}/nothing`);
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.locator('.response .error-card').waitFor({ timeout: 15000 });
  const card = await p.locator('.response .error-card').innerText();
  const head = await p.locator('.response .error-card lit-material-status').innerText();
  check('(7) a refused connection reads as one, with what to try', TAURI ? head.includes('Connection refused') && card.includes('is the server running') && (await p.locator('.response .error-details').count()) === 1 : head.includes("couldn't reach") && card.includes('CORS'), `${head} | ${card.replace(/\n/g, ' ')}`);

  // --- (7) A failing check: red Tests chip, "n of m failed", red rule on the header ---
  await tabUrl(`${base}/json`, { postRequestScript: "assert.status(200);\nassert.status(201);" });
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.locator('.response-meta').waitFor({ timeout: 15000 });
  await settle(p, 600);
  const summary = await p.locator('.checks-summary').innerText();
  const red = await p.locator('.checks-summary').evaluate((el) => getComputedStyle(el).color);
  const rule = await p.locator('.response[data-checks="failed"] > .response-meta').evaluate((el) => getComputedStyle(el).boxShadow).catch(() => '');
  check('(7) a failed check: "1 of 2 tests failed" in the fail colour, red rule on the header', summary.includes('1 of 2 tests failed') && red === 'rgb(255, 156, 156)' && rule.includes('255, 156, 156'), `${summary} ${red} ${rule}`);
  check('(7) ...and the Tests tab says so', (await p.getByRole('tab', { name: /Tests 1 of 2 failed/ }).count()) === 1);
  await tabUrl(`${base}/json`, { postRequestScript: '' });

  // --- (2) Pasting curl into the URL field fills the request; Undo puts it back ---
  await tabUrl(`${base}/before`);
  const curl = `curl -X POST '${base}/post' \\\n  -H 'Content-Type: application/json' \\\n  -H 'X-Trace: t1' \\\n  -d '{"name": "Ada"}'`;
  await p.locator('hitop-request .url-field .url input').evaluate((input, text) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', text);
    input.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true, composed: true }));
  }, curl);
  await settle(p, 600);
  tab = (await h.stored.tabs()).tab;
  check('(2) pasted curl fills method, URL, headers and body', tab.method === 'POST' && tab.url === `${base}/post` && tab.headers.length === 2 && tab.headers[1].value === 't1' && tab.requestBody === '{"name": "Ada"}', JSON.stringify([tab.method, tab.url, tab.headers, tab.requestBody]));
  check('(2) ...not the whole command as the URL, and the tab is named from it', !tab.url.includes('curl') && (await p.locator('.tab-strip lit-material-tab[selected]').innerText()).includes('/post'));
  const toastText = await p.locator('#toast').evaluate((el) => el.textContent + ' ' + (el.message ?? ''));
  check('(2) a toast says so, with Undo', toastText.includes('Imported from curl') && (await p.locator('#toast').getByRole('button', { name: 'Undo' }).count()) === 1, toastText);
  await p.locator('#toast').getByRole('button', { name: 'Undo' }).click();
  await settle(p);
  tab = (await h.stored.tabs()).tab;
  check('(2) Undo puts the request back', tab.method === 'GET' && tab.url === `${base}/before` && !tab.requestBody, JSON.stringify([tab.method, tab.url]));
  // Import takes a curl command as one new request.
  const before = (await h.stored.collections()).find((c) => c.name === 'My requests').requests.length;
  await store('app.openDialog("import");');
  await p.locator('#dialog #json').waitFor();
  await p.locator('#dialog #json').evaluate((el, text) => {
    el.value = text;
    el.dispatchEvent(new Event('input', { bubbles: true }));
  }, `curl -u ada:pw ${base}/me`);
  await p.locator('#dialog').getByRole('button', { name: 'Import', exact: true }).click();
  await settle(p, 800);
  cols = await h.stored.collections();
  tab = (await h.stored.tabs()).tab;
  check('(2) Import takes curl as one new request in the collection', cols.find((c) => c.name === 'My requests').requests.length === before + 1 && tab.url === `${base}/me` && tab.auth?.type === 'basic' && tab.auth.username === 'ada', JSON.stringify([tab.url, tab.auth]));

  // --- (10) Signed-out Organization page: the empty state's Sign in renders ---
  await store('await app.openOrg("general");');
  await settle(p, 600);
  const signIn = p.locator('hitop-org lit-material-empty-state').getByRole('button', { name: 'Sign in' });
  check('(10) the signed-out Organization page has a Sign in button', (await signIn.count()) === 1 && (await signIn.isVisible()));
  // A portal link signed out: Sign in first, named after where it goes.
  await p.evaluate(() => (location.hash = '#org=o-acme&section=billing'));
  await p.locator('#dialog h2.signin-title').waitFor({ timeout: 10000 }).catch(() => {});
  await settle(p, 800);
  const title = await p.locator('#dialog h2.signin-title').first().innerText().catch(() => '');
  check('(10) a signed-out #org= link asks to sign in to open that section', title.includes('Sign in to open Plan & billing'), title);
  await p.keyboard.press('Escape');
  await settle(p);

  check('no page errors', errors.length === 0, errors.join(' | '));
  srv.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
