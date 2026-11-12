// UX2 screenshots: a response with a failed check at 1440×900 and 1024×768, a binary (image)
// response, and the request view at 390 px, in a fresh app.
//   HITOP_MODE=tauri node tests/shots-ux2.cjs <dir>   (Tauri mode: the waterfall's timings)
const http = require('http');
const path = require('path');
const { launch, inApp } = require('./harness.cjs');

(async () => {
  const dir = process.argv[2] || '.';
  const server = http.createServer((req, res) => {
    const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    res.writeHead(200, { ...cors, 'content-type': 'application/json' }).end(JSON.stringify({ id: 7, status: 'paid', total: { amount: 6480, currency: 'EUR' }, items: [{ sku: 'TEE-0193', qty: 2 }] }));
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ viewport: { width: 1440, height: 900 } });
  const p = h.page;
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.waitForTimeout(800);
  await inApp(p, 'app.store.updateTab({ url: a + "/orders/7", postRequestScript: "assert.status(200);\\nassert.equal(getResponseValue(\\"status\\"), \\"pending\\");" }); app.store.emit();', base);
  await p.getByRole('button', { name: 'Send', exact: true }).click();
  await p.locator('.response-meta').waitFor();
  await p.waitForTimeout(800);
  await p.screenshot({ path: path.join(dir, `failed-check-1440-${h.mode}.png`) });
  await p.setViewportSize({ width: 1024, height: 768 });
  await p.waitForTimeout(800);
  await p.screenshot({ path: path.join(dir, `failed-check-1024-${h.mode}.png`) });
  await p.setViewportSize({ width: 390, height: 844 });
  await p.waitForTimeout(800);
  await p.screenshot({ path: path.join(dir, `request-390-${h.mode}.png`), fullPage: false });
  await p.setViewportSize({ width: 1440, height: 900 });
  await p.waitForTimeout(500);
  await p.locator('.timing-inline').focus();
  await p.waitForTimeout(300);
  await p.screenshot({ path: path.join(dir, `waterfall-legend-1440-${h.mode}.png`) });
  server.close();
  await h.finish();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
