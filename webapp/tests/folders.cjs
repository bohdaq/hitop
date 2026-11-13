// Collection folders and .zip files, with local mocks only: a Node HTTP
// server for the requests, and for the folder either an in-memory
// FileSystemDirectoryHandle (web: what showDirectoryPicker returns) or a
// real temp directory behind the mocked Tauri dialog (HITOP_MODE=tauri).
// The folder is cli/fixtures/sample (the CLI's own test folder) plus a
// request with fields the editor doesn't show, which must survive a save.
//   BASE=http://localhost:7879 node tests/folders.cjs
const fs = require('fs');
const http = require('http');
const os = require('os');
const path = require('path');
const { unzipSync, strFromU8 } = require('fflate');
const { launch, checker } = require('./harness.cjs');
const { listTree } = require('./tauri-mock.cjs');

const { results, check } = checker();
const settle = (p) => p.waitForTimeout(500);

// The API the sample folder talks to.
function mockApi() {
  const seen = [];
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (c) => (body += c));
    req.on('end', () => {
      const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
      if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
      seen.push({ method: req.method, url: req.url, headers: req.headers, body });
      const json = (status, v) => res.writeHead(status, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify(v));
      if (req.method === 'GET' && req.url.startsWith('/users/1')) return json(200, { name: 'Ada' });
      if (req.method === 'POST' && req.url === '/orders') return json(201, { id: 7 });
      if (req.method === 'GET' && req.url === '/orders/7') return json(200, { status: req.headers['x-api-key'] === 'local-key' ? 'paid' : 'unauthorized' });
      json(404, { error: 'not found' });
    });
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, seen, base: `http://127.0.0.1:${server.address().port}` })));
}

// An in-memory directory with the File System Access API's shape.
function fakeDirectoryPicker(files) {
  const mkdir = (name) => ({ kind: 'directory', name, children: new Map() });
  const root = mkdir('sample');
  const put = (p, text) => {
    const parts = p.split('/');
    let d = root;
    for (const part of parts.slice(0, -1)) {
      if (!d.children.has(part)) d.children.set(part, mkdir(part));
      d = d.children.get(part);
    }
    d.children.set(parts.at(-1), { kind: 'file', name: parts.at(-1), text });
  };
  for (const [p, t] of Object.entries(files)) put(p, t);
  const handle = (node) => {
    if (node.kind === 'file') {
      return {
        kind: 'file',
        name: node.name,
        getFile: async () => ({ text: async () => node.text }),
        createWritable: async () => {
          let next = '';
          return { write: async (t) => (next += t), close: async () => (node.text = next) };
        },
      };
    }
    return {
      kind: 'directory',
      name: node.name,
      async *entries() {
        for (const [name, child] of node.children) yield [name, handle(child)];
      },
      async getDirectoryHandle(name, { create } = {}) {
        if (!node.children.has(name)) {
          if (!create) throw new DOMException('not found', 'NotFoundError');
          node.children.set(name, mkdir(name));
        }
        return handle(node.children.get(name));
      },
      async getFileHandle(name, { create } = {}) {
        if (!node.children.has(name)) {
          if (!create) throw new DOMException('not found', 'NotFoundError');
          node.children.set(name, { kind: 'file', name, text: '' });
        }
        return handle(node.children.get(name));
      },
      async removeEntry(name) {
        node.children.delete(name);
      },
      requestPermission: async () => 'granted',
      queryPermission: async () => 'granted',
    };
  };
  window.showDirectoryPicker = async () => handle(root);
  window.__folderFiles = () => {
    const out = {};
    const walk = (node, prefix) => {
      for (const [name, child] of node.children) child.kind === 'file' ? (out[prefix + name] = child.text) : walk(child, `${prefix}${name}/`);
    };
    walk(root, '');
    return out;
  };
}

(async () => {
  const api = await mockApi();
  const files = listTree(path.join(__dirname, '../../cli/fixtures/sample'));
  files['environments/local.yaml'] = files['environments/local.yaml'].replace('http://localhost:8080', api.base);
  // Fields the editor doesn't know: saved examples and another tool's key.
  files['requests/zz-notes.yaml'] = 'name: Notes\nmethod: GET\nurl: ${base}/users/1\nexamples:\n- name: ok\n  status: 200\nx-owner: team-a\n';

  const h = await launch({ viewport: { width: 1280, height: 900 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  let dir = null;
  if (h.tauri) {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-folder-'));
    for (const [rel, text] of Object.entries(files)) {
      fs.mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
      fs.writeFileSync(path.join(dir, rel), text);
    }
  } else {
    await h.ctx.addInitScript(fakeDirectoryPicker, files);
  }
  const folderFiles = async () => (h.tauri ? listTree(dir) : p.evaluate(() => window.__folderFiles()));

  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.evaluate(() => document.querySelector('hitop-app').go('collections')); // UX2: the overview (crumb menu → All collections)
  if (h.tauri) h.mock.nextFolder = dir;
  await p.getByRole('button', { name: 'Open folder' }).click();
  await p.locator('h1', { hasText: 'Sample API' }).waitFor({ timeout: 10000 });
  const side = await p.locator('aside').innerText();
  check('folder opens as a collection, in its order', /Get user[\s\S]*Create order[\s\S]*Get order[\s\S]*Notes/.test(side), side.replace(/\n/g, ' | ').slice(0, 200));
  check('says it saves to the folder', (await p.locator('.folder-status').innerText()).includes('Saved to the folder'));

  // The folder's environments, by name and folder
  await p.getByRole('combobox', { name: 'Environment' }).click();
  await p.getByRole('option', { name: 'local (Sample API)' }).click();
  await settle(p);
  await p.getByRole('button', { name: /Run collection/ }).click();
  await p.locator('.run h2', { hasText: 'Last run' }).waitFor({ timeout: 20000 });
  const run = await p.locator('.run-list').innerText();
  check('folder collection runs with its environment (local mock)', (await p.locator('.run-list .status-chip.ok').count()) === 4, run.replace(/\n/g, ' | '));
  check('script context passes between folder requests', api.seen.some((r) => r.url === '/orders/7' && r.headers['x-api-key'] === 'local-key'), JSON.stringify(api.seen.map((r) => r.url)));

  // Live save: edit and save a request, rename the collection
  await p.locator('aside').getByRole('treeitem', { name: /^GET Notes/ }).click();
  await p.getByLabel('URL').fill('${base}/users/1?v=2');
  await p.getByRole('button', { name: 'Save request' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await settle(p);
  let now = await folderFiles();
  const notes = now['requests/zz-notes.yaml'] || '';
  check('saved request written to its file', notes.includes('url: ${base}/users/1') && notes.includes('name: v') && notes.includes("value: '2'"), notes);
  check('fields the editor does not show survive the save', notes.includes('examples:') && notes.includes('x-owner: team-a'), notes);
  check('other files untouched', now['requests/get-user.yaml'] === files['requests/get-user.yaml'] && now['environments/prod.yaml'] === files['environments/prod.yaml']);

  // FV1: a new request in a folder collection is written at once, an untouched one closed leaves
  // no file, and its first save gives the file its name; re-read, the tab still finds it.
  const fileCount = Object.keys(now).length;
  await p.locator('aside').getByRole('button', { name: 'Add a request to Sample API' }).click();
  await settle(p);
  now = await folderFiles();
  check('FV1 + on a folder collection writes requests/untitled.yaml', 'requests/untitled.yaml' in now && Object.keys(now).length === fileCount + 1, Object.keys(now).join(' '));
  await p.keyboard.press('Alt+Shift+KeyW');
  await settle(p);
  now = await folderFiles();
  check('FV1 ...closing it untouched leaves no file', !('requests/untitled.yaml' in now) && Object.keys(now).length === fileCount, Object.keys(now).join(' '));
  await p.locator('aside').getByRole('button', { name: 'Add a request to Sample API' }).click();
  await settle(p);
  await p.getByLabel('URL').fill('${base}/users/2');
  await p.keyboard.press('Control+KeyS');
  await settle(p);
  now = await folderFiles();
  const added = Object.keys(now).filter((k) => !(k in files) && k.startsWith('requests/'));
  check('FV1 Mod+S leaves one file, named after it (requests/get-users-2.yaml)', added.join() === 'requests/get-users-2.yaml' && (now['requests/get-users-2.yaml'] || '').includes('url: ${base}/users/2'), added.join(' '));
  const reread = await p.evaluate(async () => {
    const s = document.querySelector('hitop-app').store;
    const id = await s.reloadFolder(s.tab.loadedCollectionId);
    const r = s.loadedRequest(s.tab);
    return { ok: Boolean(id && r), name: r?.name };
  });
  check('FV1 ...read again from the folder, the tab still finds its request', reread.ok && reread.name === 'GET /users/2', JSON.stringify(reread));
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.deleteRequest(s.loadedRequest(s.tab), s.tab.loadedCollectionId);
  });
  await settle(p);
  now = await folderFiles();
  check('FV1 ...deleted, its file goes', Object.keys(now).length === fileCount && !('requests/get-users-2.yaml' in now), Object.keys(now).join(' '));
  // DD1: a folder made in the sidebar is written to hitop.yaml (`folders`); moving a folder moves its files; Undo puts them back.
  const moved = await p.evaluate(async () => {
    const app = document.querySelector('hitop-app');
    const s = app.store;
    const c = s.collections.find((x) => x.name === 'Sample API');
    app.addFolder(c.id, '', 'archive');
    await new Promise((r) => setTimeout(r, 600));
    const r = s.moveNode({ kind: 'folder', from: c.id, path: 'orders', to: c.id, folder: 'archive' });
    await new Promise((r2) => setTimeout(r2, 800));
    window.__dd1Undo = r.undo;
    return { ok: r.ok, path: r.path };
  });
  await settle(p);
  now = await folderFiles();
  check('DD1 an empty folder made in the sidebar is kept in hitop.yaml', /folders:[\s\S]*- archive/.test(now['hitop.yaml']), now['hitop.yaml']);
  check('DD1 a folder moved into another: its files move, the old ones go', moved.ok && moved.path === 'archive/orders' && 'requests/archive/orders/create-order.yaml' in now && !('requests/orders/create-order.yaml' in now), Object.keys(now).join(' '));
  const undone = await p.evaluate(async () => window.__dd1Undo());
  await settle(p);
  await p.waitForTimeout(600);
  now = await folderFiles();
  check('DD1 Undo puts the files back', undone === true && 'requests/orders/create-order.yaml' in now && !('requests/archive/orders/create-order.yaml' in now), Object.keys(now).join(' '));
  await p.locator('aside').getByRole('treeitem', { name: 'Sample API', exact: true }).click();
  await p.getByRole('button', { name: 'Rename collection' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'Sample API v2'));
  await p.locator('#dialog').getByRole('button', { name: 'Rename' }).click();
  await settle(p);
  now = await folderFiles();
  check('rename written to hitop.yaml', now['hitop.yaml'].includes('name: Sample API v2'), now['hitop.yaml']);
  if (h.tauri) check('[tauri] folder collection kept out of collections.json', !JSON.stringify(h.mock.stored('collections')).includes('Sample API'));

  // Export as zip, then import it back
  const zip = await h.download(() => p.getByRole('button', { name: 'Export as zip' }).click());
  const entries = unzipSync(new Uint8Array(zip));
  const names = Object.keys(entries).sort();
  check('zip holds the folder', names.includes('sample-api-v2/hitop.yaml') && names.includes('sample-api-v2/requests/orders/create-order.yaml') && names.includes('sample-api-v2/environments/prod.yaml'), names.join(' '));
  const prodYaml = strFromU8(entries['sample-api-v2/environments/prod.yaml'] || new Uint8Array());
  check('zip keeps secrets as names only', prodYaml.includes('token:\n    secret: true'), prodYaml);
  const zipPath = path.join(os.tmpdir(), `hitop-e2e-${process.pid}.zip`);
  fs.writeFileSync(zipPath, zip);
  await p.evaluate(() => document.querySelector('hitop-app').go('collections')); // UX2: the overview (crumb menu → All collections)
  await p.locator('#zip-input').setInputFiles(zipPath);
  await settle(p);
  const side2 = await p.locator('aside').innerText();
  check('zip import adds the collection', (side2.match(/Sample API v2/g) || []).length === 2, side2.replace(/\n/g, ' | ').slice(0, 240));
  fs.rmSync(zipPath);

  // Closing leaves the files
  await p.locator('aside').getByRole('treeitem', { name: 'Sample API v2' }).first().click();
  await p.getByRole('button', { name: 'Close folder' }).click();
  await p.getByRole('button', { name: 'Close', exact: true }).click();
  await settle(p);
  check('close folder keeps its files', Object.keys(await folderFiles()).length === Object.keys(files).length);

  check('no page errors', errors.length === 0, errors.join(' || '));
  api.server.close();
  if (dir) fs.rmSync(dir, { recursive: true, force: true });
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message.split('\n')[0]);
  process.exit(1);
});
