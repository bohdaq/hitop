// What the e2e suites share, in either mode:
//   web    (default) the web app at BASE/app/, requests through hitop-server's
//          /api/proxy, data in localStorage
//   tauri  (HITOP_MODE=tauri) the desktop build (dist-tauri/, `npm run
//          build:tauri`) at http://tauri.localhost/, with Tauri's invoke
//          mocked in Node (tauri-mock.cjs): requests, scripts, storage files,
//          keychain and folders
// Suites read what was sent (`sent`, the proxy's envelope in both modes) and
// what was stored (`stored.*`) through here, so the same checks run in both.
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const { TauriMock } = require('./tauri-mock.cjs');

const MODE = process.env.HITOP_MODE === 'tauri' ? 'tauri' : 'web';
const TAURI = MODE === 'tauri';
const BASE = process.env.BASE || 'http://localhost:7879';
const TAURI_ORIGIN = 'http://tauri.localhost';
const DIST = process.env.TAURI_DIST || path.join(__dirname, '../dist-tauri');
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.png': 'image/png', '.json': 'application/json' };

function checker() {
  const results = [];
  const check = (name, ok, extra = '') => results.push(`${ok ? 'PASS' : 'FAIL'} ${name}${extra ? ' — ' + extra : ''}`);
  return { results, check };
}

// { browser, ctx, page, mode, url, sent, stored, download, mock, finish }
// `seed`: web → localStorage key → value; tauri → a directory of storage files.
async function launch({ viewport = { width: 1280, height: 860 }, seed = null, tauriSeed = null, results } = {}) {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport, acceptDownloads: true });
  const sent = [];
  let mock = null;
  if (TAURI) {
    mock = await new TauriMock({ seed: tauriSeed }).ready();
    mock.sent = sent;
    await ctx.exposeBinding('__hitopInvoke', (_source, cmd, args) => mock.invoke(cmd, args));
    await ctx.addInitScript(() => {
      // Callbacks as Tauri's core.js keeps them: channels (ws_connect) call back through runCallback.
      const callbacks = new Map();
      let next = 1;
      window.__TAURI_INTERNALS__ = {
        invoke: (cmd, args) => window.__hitopInvoke(cmd, args ?? {}),
        transformCallback: (cb) => (callbacks.set(next, cb), next++),
        unregisterCallback: (id) => callbacks.delete(id),
        runCallback: (id, data) => callbacks.get(id)?.(data),
      };
    });
    await ctx.route(`${TAURI_ORIGIN}/**`, (route) => {
      let rel = decodeURIComponent(new URL(route.request().url()).pathname).replace(/^\/+/, '') || 'index.html';
      const file = path.join(DIST, rel);
      if (!file.startsWith(DIST) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) return route.fulfill({ status: 404, body: 'not found' });
      route.fulfill({ status: 200, contentType: TYPES[path.extname(file)] || 'application/octet-stream', body: fs.readFileSync(file) });
    });
  } else if (seed) {
    await ctx.addInitScript((f) => {
      if (!sessionStorage.getItem('seeded')) {
        for (const [k, v] of Object.entries(f)) localStorage.setItem(k, v);
        sessionStorage.setItem('seeded', '1');
      }
    }, seed);
  }
  const page = await ctx.newPage();
  // Channel messages from the mock's Rust side into the page.
  if (mock) mock.deliver = (id, data) => page.evaluate(([i, d]) => window.__TAURI_INTERNALS__.runCallback(i, d), [id, data]).catch(() => {});
  if (!TAURI) page.on('request', (r) => r.url().includes('/api/proxy') && sent.push(JSON.parse(r.postData())));
  const proxyCalls = [];
  page.on('request', (r) => r.url().includes('/api/proxy') && proxyCalls.push(r.url()));
  const url = TAURI ? `${TAURI_ORIGIN}/index.html` : `${BASE}/app/`;
  const ls = (key) => page.evaluate((k) => JSON.parse(localStorage.getItem(k) || 'null'), key);

  // The desktop saves through invoke after the app's 250 ms batching: wait
  // until no storage command has run for a moment.
  const quiet = async () => {
    if (!TAURI) return;
    for (let i = 0; i < 40; i++) {
      const n = mock.calls.length;
      await page.waitForTimeout(150);
      if (mock.calls.length === n && Date.now() - mock.lastCall > 300) return;
    }
  };
  const stored = {
    tabs: async () => {
      const [tabs, current] = await page.evaluate(() => [JSON.parse(localStorage.getItem('hitop_tabs')), +localStorage.getItem('hitop_current_tab')]);
      return { tabs, current, tab: tabs?.[current] };
    },
    collections: async () => (TAURI ? (await quiet(), mock.stored('collections')) : (await ls('hitop_collections')) || []),
    contexts: async () => (TAURI ? (await quiet(), mock.stored('contexts')) : (await ls('hitop_contexts')) || {}),
    history: async () => (TAURI ? (await quiet(), mock.stored('history')) : (await ls('hitop_history')) || []),
    // { list, active }; on the desktop secret values are in `mock.keychain`, not here.
    environments: async () => {
      if (TAURI) {
        await quiet();
        const e = mock.stored('environments');
        return { list: e.environments || [], active: e.active ?? null };
      }
      return { list: (await ls('hitop_environments')) || [], active: await page.evaluate(() => localStorage.getItem('hitop_active_environment')) };
    },
  };

  // Runs `click` and returns the text of the file it saves.
  const download = async (click) => {
    if (TAURI) {
      const n = mock.writes.length;
      await click();
      for (let i = 0; i < 100 && mock.writes.length === n; i++) await page.waitForTimeout(50);
      return fs.readFileSync(mock.writes[mock.writes.length - 1]);
    }
    const [d] = await Promise.all([page.waitForEvent('download'), click()]);
    return fs.readFileSync(await d.path());
  };

  const finish = async () => {
    if (results && TAURI) results.push(`${proxyCalls.length === 0 ? 'PASS' : 'FAIL'} [tauri] nothing goes through the web proxy — ${proxyCalls.length} calls`);
    console.log(`[${MODE}]\n${results ? results.join('\n') : ''}`);
    await browser.close();
    mock?.cleanup();
    if (results?.some((r) => r.startsWith('FAIL'))) process.exit(1);
  };
  return { browser, ctx, page, mode: MODE, tauri: TAURI, url, sent, stored, download, mock, finish };
}

module.exports = { launch, checker, MODE, TAURI, BASE };
