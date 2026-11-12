// ${secret:vault|aws|azure|op:...} references: in the desktop build they are
// resolved through Rust (secret_refs_resolve, mocked here) for one send, fill
// the request and variable values, and are masked in history; the browser
// can't reach a secret manager and says so instead of sending. Local Node
// server only.
//   BASE=http://localhost:7879 node tests/secret-refs.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

const VAULT = 'vault-T21-s3cr3t';
const AZURE = 'az-T21-s3cr3t';

function api() {
  const hits = [];
  const server = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    const url = new URL(req.url, 'http://x');
    hits.push(req.url);
    if (url.pathname === '/slow') return setTimeout(() => res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify({ query: Object.fromEntries(url.searchParams) })), 1500);
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify({ query: Object.fromEntries(url.searchParams) }));
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, hits })));
}

(async () => {
  const { server, hits } = await api();
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const type = async (text) => p.keyboard.type(text, { delay: 10 });
  const send = async () => {
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => document.querySelector('.response-meta, .error-text') && !document.querySelector('[aria-busy="true"]'), null, { timeout: 30000 });
    await settle(p);
  };
  // A request that can't be prepared isn't sent: the reason is a notification.
  const note = () => p.locator('#toast').evaluate((el) => [el.message, el.textContent, el.shadowRoot?.textContent].filter(Boolean).join(' '));
  const sendRefused = async () => {
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => {
      const t = document.querySelector('#toast');
      return t && /wasn't sent/.test([t.message, t.textContent, t.shadowRoot?.textContent].join(' '));
    }, null, { timeout: 30000 });
    return note();
  };
  if (TAURI) {
    h.mock.secretManager.set('secret:vault:kv/app#token', VAULT);
    h.mock.secretManager.set('secret:azure:kv-test/api-key', AZURE);
  }

  await p.goto(h.url, { waitUntil: 'networkidle' });
  // An environment whose `key` is an Azure reference, and the base URL.
  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await settle(p);
  await p.getByRole('button', { name: 'New environment' }).first().click();
  await p.waitForTimeout(300);
  await type('Prod');
  await p.keyboard.press('Enter');
  await settle(p);
  for (const [k, v] of [['base', base], ['key', '${secret:azure:kv-test/api-key}']]) {
    await p.getByRole('button', { name: 'Add variable' }).click();
    await type(k);
    await p.keyboard.press('Tab');
    await type(v);
    await settle(p);
  }

  await p.locator('aside .new-request').click(); // the sidebar's +: a new request (FV1)
  await p.getByLabel('URL').fill('${base}/echo?t=${secret:vault:kv/app#token}&k=${key}');
  if (!TAURI) {
    const error = await sendRefused();
    check('the browser names the references it cannot resolve', error.includes('resolve in the hitop desktop app and the CLI') && error.includes('${secret:vault:kv/app#token}') && error.includes('${secret:azure:kv-test/api-key}'), error);
    check('nothing was sent', hits.length === 0, hits.join(' '));
  } else {
    await send();
    const body = await p.locator('.response-body').innerText();
    check('[tauri] the request carries the resolved values', hits.length === 1 && hits[0].includes(`t=${VAULT}`) && hits[0].includes(`k=${AZURE}`), hits.join(' '));
    check('[tauri] the response view shows the echo', body.includes(VAULT), body.slice(0, 200));
    check('[tauri] one resolve call with both references', JSON.stringify(h.mock.refCalls) === JSON.stringify([['secret:vault:kv/app#token', 'secret:azure:kv-test/api-key']]), JSON.stringify(h.mock.refCalls));
    await settle(p);
    const stored = ['history.json', 'environments.json', 'collections.json', 'contexts.json'].map((f) => h.mock.read(f, '')).join('\n');
    check('[tauri] no value in history or storage files', !stored.includes(VAULT) && !stored.includes(AZURE), stored.slice(0, 400));
    check('[tauri] the environment keeps the reference', stored.includes('${secret:azure:kv-test/api-key}'));
    // A reference the secret manager doesn't have: named, nothing sent.
    await p.getByLabel('URL').fill('${base}/echo?x=${secret:op:Dev/Missing/field}');
    const error = await sendRefused();
    check('[tauri] a failing reference is named', error.includes('${secret:op:Dev/Missing/field} (op): not found') && !error.includes('Cannot create'), error);
    check('[tauri] nothing sent for it', hits.length === 1, hits.join(' '));
    // Another tab's send while one is in flight: the slow one's history entry is still masked
    // (resolved values are kept for masking, not cleared by the next send).
    await p.getByLabel('URL').fill('${base}/slow?t=${secret:vault:kv/app#token}');
    await p.getByRole('button', { name: 'Send' }).click();
    await p.keyboard.press('Alt+Shift+KeyN'); // a new tab while the slow one waits
    await p.waitForTimeout(200);
    await p.getByLabel('URL').fill('${base}/echo?k=${key}');
    await p.getByLabel('URL').press('Enter');
    await p.waitForFunction(() => !document.querySelector('[aria-busy="true"]'), null, { timeout: 30000 });
    await p.waitForTimeout(2500);
    const after = ['history.json', 'collections.json'].map((f) => h.mock.read(f, '')).join('\n');
    check('[tauri] two tabs in flight: no value in history', hits.some((u) => u.startsWith('/slow')) && !after.includes(VAULT) && !after.includes(AZURE), after.slice(0, 400));
  }

  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
