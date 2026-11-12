// UX2 (reviews/2026-11-12-ux-audit.md, HITOP_NEXT_UI.md UX2): a check per top-10 item, in both
// modes, against a local server (CORS open, so the web build calls it straight from the browser).
//   BASE=http://localhost:7879 node tests/ux2.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker, inApp, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

// /json, /slow (answers after 4 s), /sse (an event every 200 ms, never ends), /png (a 2×1 PNG).
function server() {
  // A 2×1 PNG, made here (zlib + CRC) so it is valid byte for byte.
  const zlib = require('zlib');
  const crc = (buf) => {
    let c = ~0;
    for (const b of buf) {
      c ^= b;
      for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
    }
    return (~c) >>> 0;
  };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.from([0, 0, 0, 2, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
  const PNG = Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(Buffer.from([0, 255, 0, 0, 0, 0, 255]))), chunk('IEND', Buffer.alloc(0))]);
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
    if (u.pathname === '/pdf') return res.writeHead(200, { ...cors, 'content-type': 'application/pdf' }).end(Buffer.from('%PDF-1.4\n%%EOF\n'));
    if (u.pathname === '/bin') return res.writeHead(200, { ...cors, 'content-type': 'application/octet-stream' }).end(Buffer.from([0, 1, 2, 0xff, 0x41, 0x42, 0x43]));
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
  if (process.env.UX2_DEBUG) p.on('console', (m) => m.type() === 'error' && console.log('CONSOLE', m.text().slice(0, 300)));
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
  check('(8) picking it makes a GET with Accept: text/event-stream, tagged SSE', tab.method === 'GET' && tab.headers.some((x) => x.name === 'Accept' && x.value === 'text/event-stream') && (await p.locator('.tab-strip lit-material-tab[selected] .method-tag').getAttribute('data-tag')) === 'SSE', JSON.stringify(tab.headers));
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
  // (4) The header never overflows (a failed check, the waterfall on the desktop) at 1440, 1024 and 390.
  for (const [w, hgt] of [[1440, 900], [1024, 768], [390, 844]]) {
    await p.setViewportSize({ width: w, height: hgt });
    await settle(p, 500);
    const fit = await p.locator('.response-meta').evaluate((el) => [el.scrollWidth, el.clientWidth]);
    check(`(4) at ${w} px the response header fits (wraps rather than overflows)`, fit[0] <= fit[1] + 1, fit.join(' > '));
  }
  await p.setViewportSize({ width: 1440, height: 900 });
  await settle(p, 500);
  if (TAURI) {
    // The waterfall's legend (phases, Total) on focus.
    await p.locator('.timing-inline').focus();
    await settle(p, 200);
    const card = await p.locator('.timing-card').evaluate((el) => ({ h: el.getBoundingClientRect().height, shown: getComputedStyle(el).display !== 'none', text: el.querySelector('lit-material-timing-waterfall')?.shadowRoot?.textContent ?? '' }));
    check('(4) the waterfall\'s legend opens on focus, with the total', card.shown && card.h > 0 && /Total/.test(card.text), JSON.stringify(card).slice(0, 200));
  }
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
  const importedReq = cols.find((c) => c.name === 'My requests').requests.find((r) => r.id === tab.loadedRequestId);
  check('(2) Import takes curl as one new request in the collection, saved', cols.find((c) => c.name === 'My requests').requests.length === before + 1 && importedReq?.url === `${base}/me` && importedReq?.auth?.type === 'basic' && importedReq.auth.username === 'ada', JSON.stringify(importedReq));
  await p.locator('#toast').getByRole('button', { name: 'Undo' }).click();
  await settle(p, 600);
  check('(2) ...and its Undo takes it out again', (await h.stored.collections()).find((c) => c.name === 'My requests').requests.length === before);

  // --- (4) The response gets the room: one header line, tools on the tab row (1440×900) ---
  await tabUrl(`${base}/json`, { method: 'GET' });
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.locator('.response-meta').waitFor({ timeout: 15000 });
  await settle(p, 500);
  const box = async (sel) => p.locator(sel).first().boundingBox();
  const meta = await box('.response-meta');
  const tabbar = await box('.response-tabbar');
  const tools = await box('.response-tabbar .body-tools');
  const bodyTop = (await box('.response-scroll')).y;
  check('(4) one response header line, the body tools on the tab row', meta.height < 50 && tools.y < tabbar.y + 10 && tabbar.height < 50, JSON.stringify({ meta, tabbar, tools }));
  check('(4) the body starts high up (y < 340 at 1440×900)', bodyTop < 340, String(bodyTop));
  check('(4) no duplicate "reused connection" or Total in the header', !/reused connection|Total/.test(await p.locator('.response-meta').innerText()));
  const sendBox = await box('.request-line .send');
  const moreBox = await box('.request-line .more-send');
  check('(4) Save and ⋯ stay on the URL row', Math.abs(sendBox.y + sendBox.height / 2 - (moreBox.y + moreBox.height / 2)) < 4);
  // Mod+B folds the side pane and remembers it.
  const paneShown = () => p.locator('#side-pane').isVisible();
  check('(4) at 1440 the side pane shows', await paneShown());
  await p.keyboard.press('Control+b');
  await settle(p);
  const layout = await p.evaluate(() => JSON.parse(localStorage.getItem('hitop_layout') || '{}'));
  check('(4) Mod+B hides the side pane, remembered in hitop_layout', !(await paneShown()) && layout.sidePane === false, JSON.stringify(layout));
  await p.keyboard.press('Control+b');
  await settle(p);
  check('(4) ...and brings it back', (await paneShown()) && (await p.evaluate(() => JSON.parse(localStorage.getItem('hitop_layout')).sidePane)) === true);
  // At 1024×768, not chosen yet: folded with a request open; Save folds into ⋯.
  await p.evaluate(() => {
    const l = JSON.parse(localStorage.getItem('hitop_layout') || '{}');
    delete l.sidePane;
    localStorage.setItem('hitop_layout', JSON.stringify(l));
    document.querySelector('hitop-app').sidePref = undefined;
  });
  await p.setViewportSize({ width: 1024, height: 768 });
  await settle(p, 600);
  check('(4) at 1024 the side pane starts folded with a request open', !(await paneShown()));
  const s2 = await box('.request-line .send');
  const m2 = await box('.request-line .more-send');
  check('(4) at 1024 Send and ⋯ share the URL row; Save is in ⋯', Math.abs(s2.y - m2.y) < 4 && !(await p.locator('.request-line .line-save').isVisible()) && (await p.locator('#line-menu .menu-save').evaluate((el) => getComputedStyle(el).display)) !== 'none');
  check('(4) at 1024 the body starts above the fold', (await box('.response-scroll')).y < 400, String((await box('.response-scroll')).y));
  await p.setViewportSize({ width: 390, height: 844 });
  await settle(p, 600);
  check('(4) at 390 px nothing scrolls sideways', await p.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1), String(await p.evaluate(() => document.documentElement.scrollWidth)));
  await p.setViewportSize({ width: 1440, height: 900 });
  await settle(p, 600);

  // --- (3) Rail pages: the crumb names them, Esc and the rail's Collections return to the workspace ---
  const rail = (name) => p.locator('lit-material-navigation-rail-item', { hasText: name });
  await rail('History').click();
  await settle(p);
  check('(3) the crumb names the page', (await p.locator('header.topbar .crumb-name').innerText()).trim() === 'History');
  await p.keyboard.press('Escape');
  await settle(p);
  check('(3) Esc on a rail page returns to the workspace', (await p.locator('main hitop-request').count()) === 1 && (await p.locator('.tab-strip').count()) === 1);
  await rail('Settings').click();
  await settle(p);
  await rail('Collections').click();
  await settle(p);
  check('(3) the rail\'s Collections returns to the tabs and the open request, not the overview', (await p.locator('main hitop-request').count()) === 1 && (await p.locator('main hitop-collections').count()) === 0);

  // --- (6) Binary bodies by Content-Type; Find starts empty; Diff only against the same request ---
  const sendAndWait = async () => {
    await p.getByRole('button', { name: 'Send', exact: true }).click();
    await p.locator('.response-meta').waitFor({ timeout: 15000 });
    await settle(p, 500);
  };
  await tabUrl(`${base}/png`, { method: 'GET', postRequestScript: '' });
  await sendAndWait();
  const img = await p.locator('.response img.response-image').evaluate((el) => ({ w: el.naturalWidth, h: el.naturalHeight })).catch(() => null);
  const facts = await p.locator('.binary-facts').innerText().catch(() => '');
  check('(6) image/png shows as an image with its size and dimensions', img?.w === 2 && img?.h === 1 && facts.includes('Image') && facts.includes('image/png') && facts.includes('2 × 1') && (await p.locator('.response .response-text').count()) === 0, `${JSON.stringify(img)} ${facts}`);
  await tabUrl(`${base}/pdf`);
  await sendAndWait();
  check('(6) a PDF opens in a frame without scripts (a blob URL)', ((await p.locator('iframe.response-pdf').getAttribute('src')) || '').startsWith('blob:') && (await p.locator('iframe.response-pdf').getAttribute('sandbox')) === 'allow-same-origin');
  await tabUrl(`${base}/bin`);
  await sendAndWait();
  const hex = await p.locator(".hex-dump").innerText().catch(() => "");
  check('(6) other binary: "Binary · 7 B", Download and a hex dump', (await p.locator('.binary-facts').innerText()).includes('Binary · 7 B') && hex.startsWith('00000000  00 01 02 ff 41 42 43') && hex.includes('|....ABC|') && (await p.locator('.binary-facts').getByRole('button', { name: 'Download' }).count()) === 1, hex);
  await tabUrl(`${base}/json`);
  await sendAndWait();
  await p.getByLabel('Find in body').fill('ok');
  await settle(p);
  await sendAndWait();
  check('(6) Find starts empty on a new response', (await p.getByLabel('Find in body').inputValue()) === '');
  const respTab = (name) => p.locator('.response lit-material-tabs[aria-label="Response details"] lit-material-tab').filter({ hasText: name });
  await respTab(/^\s*Diff\s*$/).click();
  await settle(p);
  check('(6) Diff offers only earlier responses to the same request', (await p.locator('.diff-pane .compare-summary').count()) === 1 && !(await p.locator('#diff-left').evaluate((el) => [...el.querySelectorAll('lit-material-select-option')].map((o) => o.textContent).join('|'))).includes('/png'));
  check('(6) ...in a padded panel', (await p.locator('.diff-pane').evaluate((el) => getComputedStyle(el).paddingLeft)) === '20px');
  await tabUrl(`${base}/json?other=1`);
  await sendAndWait();
  await respTab(/^\s*Diff\s*$/).click();
  await settle(p);
  check('(6) another URL: Diff says there is nothing earlier', (await p.locator('.diff-pane').innerText()).includes('has no earlier response to GET'), (await p.locator('.response').innerText()).slice(0, 300).replace(/\n/g, ' '));

  // --- (9) The collection tree is one Tab stop with arrow keys ---
  const tree = p.locator('nav.tree');
  const stops = await tree.evaluate((el) => [...el.querySelectorAll('button, [tabindex]')].filter((b) => b.tabIndex >= 0 && b.offsetParent !== null && !b.closest('.tree-hint')).length);
  check('(9) the tree is one Tab stop (roving tabindex)', stops === 1, String(stops));
  check('(9) ARIA tree roles', (await tree.locator('[role="tree"]').count()) === 1 && (await tree.locator('[role="treeitem"]').count()) > 3 && (await tree.locator('[role="treeitem"][aria-level="1"][aria-expanded]').count()) >= 2);
  await tree.locator('[role="treeitem"][tabindex="0"]').focus();
  const focusedKey = () => p.evaluate(() => document.activeElement?.dataset?.key ?? '');
  const k0 = await focusedKey();
  await p.keyboard.press('Home');
  const kHome = await focusedKey();
  await p.keyboard.press('ArrowDown');
  const kDown = await focusedKey();
  check('(9) Home and Down move the focus', kHome.startsWith('c:') && kDown !== kHome && kDown !== '', `${k0} ${kHome} ${kDown}`);
  await p.keyboard.press('Home');
  await p.keyboard.press('ArrowLeft');
  await settle(p);
  check('(9) Left folds the collection', (await tree.locator('[role="treeitem"][aria-level="1"]').first().getAttribute('aria-expanded')) === 'false');
  await p.keyboard.press('ArrowRight');
  await settle(p);
  check('(9) Right opens it again', (await tree.locator('[role="treeitem"][aria-level="1"]').first().getAttribute('aria-expanded')) === 'true');
  await p.keyboard.press('ArrowRight');
  check('(9) Right on an open row goes to its first child', (await focusedKey()).startsWith('v:') || (await focusedKey()).startsWith('r:'));
  // A request row: F2 renames, Shift+F10 opens its menu, Alt+Down moves it.
  const myId = (await h.stored.collections()).find((c) => c.name === 'My requests').id;
  const firstReq = tree.locator(`[data-tree="request"][data-collection="${myId}"]`).first();
  await firstReq.focus();
  await p.keyboard.press('F2');
  await settle(p);
  check('(9) F2 renames', (await p.locator('#dialog #name').count()) === 1);
  await p.keyboard.press('Escape');
  await settle(p);
  await firstReq.focus();
  await p.keyboard.press('Shift+F10');
  await settle(p);
  check('(9) Shift+F10 opens the row\'s menu, with Run, Copy as cURL and keys', (await p.locator('#request-menu').evaluate((el) => el.open || el.hasAttribute('open') || el.matches(':popover-open') || Boolean(el.shadowRoot?.querySelector(':popover-open')))) && (await p.locator('#request-menu').innerText()).includes('Copy as cURL'));
  await p.keyboard.press('Escape');
  await settle(p);
  // Two requests in My requests to swap.
  if ((await h.stored.collections()).find((c) => c.name === 'My requests').requests.length < 2) {
    await store('app.newRequest(a);', myId);
    await settle(p);
  }
  const order = async () => (await h.stored.collections()).find((c) => c.name === 'My requests').requests.map((r) => r.id);
  const before9 = await order();
  await firstReq.focus();
  await p.keyboard.press('Alt+ArrowDown');
  await settle(p);
  const after9 = await order();
  check('(9) Alt+Down moves the request down in its list, focus stays on it', after9[1] === before9[0] && after9[0] === before9[1] && (await focusedKey()) === `r:${myId}:${before9[0]}`, `${before9} → ${after9}`);

  // --- Smaller findings (a few of them) ---
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  await p.keyboard.type(`${base}/json`);
  await settle(p);
  const tagNow = await p.locator('.tab-strip lit-material-tab[selected] .method-tag').getAttribute('data-tag');
  const titleNow = (await p.locator('.tab-strip lit-material-tab[selected] .tab-title').innerText()).trim();
  check('(R) an auto-named tab has its coloured tag, its title without the method', tagNow === 'GET' && titleNow === '/json', `${tagNow} ${titleNow}`);
  await p.locator('[aria-label="Request details"] lit-material-tab', { hasText: 'Body' }).click();
  await settle(p);
  await p.getByRole('button', { name: 'Switch to POST' }).click();
  await settle(p);
  check('(R) a GET body offers Switch to POST', (await h.stored.tabs()).tab.method === 'POST');
  check('(R) the toast sits at the bottom', await p.locator('#toast').evaluate((el) => getComputedStyle(el).top === 'auto' || el.getBoundingClientRect().top > innerHeight / 2));
  const envOptions = await p.locator('.env-select').first().locator('lit-material-select-option').allInnerTexts();
  check('(R) the environment menu always has a way to manage environments', envOptions.some((t) => /Add environment|Manage environments/.test(t)), envOptions.join('|'));
  await p.keyboard.press('Alt+Shift+KeyW');
  await settle(p);
  // (Switch to POST left an edit: the Discard question.)
  if (await p.locator('lit-material-confirm-dialog[open]').count()) await p.locator('lit-material-confirm-dialog[open]').getByRole('button', { name: 'Discard' }).click();
  await settle(p);

  // --- Shortcuts added by UX2, and Save and close when discarding ---
  await p.keyboard.press('Alt+Shift+KeyF');
  await settle(p);
  check('(R) Alt+Shift+F goes to the sidebar filter', await p.evaluate(() => document.activeElement?.classList.contains('tree-filter')));
  await p.keyboard.press('Escape');
  await tabUrl(`${base}/json`);
  await sendAndWait();
  await p.keyboard.press('Alt+Shift+KeyR');
  await settle(p);
  check('(R) Alt+Shift+R goes to the response tabs', await p.evaluate(() => Boolean(document.activeElement?.closest?.('lit-material-tabs[aria-label="Response details"]'))));
  await p.keyboard.press('Alt+Shift+KeyE');
  await settle(p);
  check('(R) Alt+Shift+E goes to the environment picker', await p.evaluate(() => Boolean(document.activeElement?.closest?.('.env-select'))), await p.evaluate(() => document.activeElement?.outerHTML.slice(0, 120)));
  await p.keyboard.press('Escape');
  await settle(p);
  // A saved request with an edit: closing it offers Save and close.
  const savedTab = (await h.stored.tabs()).tab;
  await tabUrl(`${base}/json?saved=1`);
  await settle(p);
  await p.keyboard.press('Alt+Shift+KeyW');
  await p.locator('lit-material-confirm-dialog .discard-save').waitFor({ timeout: 5000 });
  await p.locator('lit-material-confirm-dialog .discard-save').click();
  await settle(p, 600);
  const savedReq = (await h.stored.collections()).flatMap((c) => c.requests).find((r) => r.id === savedTab.loadedRequestId);
  check('(R) the Discard question offers Save and close: saved, tab closed', savedReq?.url === `${base}/json?saved=1` && !(await h.stored.tabs()).tabs.some((t) => t.id === savedTab.id), JSON.stringify(savedReq?.url));

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
