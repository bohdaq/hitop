// Server-Sent Events in the web app: a request with Accept: text/event-stream shows its events as
// they arrive (through hitop-server's POST /api/proxy/stream for public hosts), Stop ends it and
// keeps what came; an event stream sent without that header offers to add it. Web only: the
// desktop sends through Rust and has no live stream yet.
//   BASE=http://localhost:7879 node tests/sse.cjs   (needs the internet: stream.wikimedia.org)
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

(async () => {
  if (TAURI) {
    console.log('[sse] web only: skipped in Tauri mode');
    process.exit(0);
  }
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
  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
