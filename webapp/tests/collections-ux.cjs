// Requests and collections UX (HITOP Next, "how do I add a request to a collection"): the
// sidebar's "+" on collections and folders, the save dialog's "New collection…", Mod+S on a
// draft, folders made in the sidebar, Move to…, drag between collections, the palette and the
// collection page, in web and Tauri mode.
//   BASE=http://localhost:7879 node tests/collections-ux.cjs
//   HITOP_MODE=tauri node tests/collections-ux.cjs
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

(async () => {
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await settle(p);
  const aside = p.locator('aside');
  const dialog = p.locator('#dialog');
  const field = (id, value) => p.locator(`#dialog #${id}`).evaluate((el, v) => (el.value = v), value);
  const pick = (id, value) => p.locator(`#dialog #${id}`).evaluate((el, v) => ((el.value = v), el.dispatchEvent(new Event('change', { bubbles: true }))), value);
  const named = (name) => h.stored.collections().then((cols) => cols.find((c) => c.name === name));
  const row = (name) => aside.locator('.tree-row.leaf').filter({ has: p.locator('span', { hasText: new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')}(●.*)?$`) }) }).first();

  // --- first open: the way in is visible ---
  check('sidebar: a New collection button next to New request', await aside.getByRole('button', { name: 'New collection', exact: true }).isVisible());
  check('collection rows carry "+ Add a request to …" (keyboard reachable)', (await aside.getByRole('button', { name: 'Add a request to My requests' }).count()) === 1);

  // --- a new collection from the sidebar, and "+" on its row (FV1: the request lands in it) ---
  await aside.getByRole('button', { name: 'New collection', exact: true }).click();
  await dialog.locator('#name').waitFor();
  await field('name', 'Users API');
  await dialog.getByRole('button', { name: 'Create' }).click();
  await settle(p);
  let users = await named('Users API');
  check('New collection makes it (numeric id) and opens its page', typeof users?.id === 'number' && (await p.locator('main h1').first().innerText()) === 'Users API', JSON.stringify(users));
  await aside.getByRole('button', { name: 'Add a request to Users API' }).click();
  await settle(p);
  users = await named('Users API');
  const t = (await h.stored.tabs()).tab;
  check('"+" on a collection adds "Untitled" to it at once; the tab is linked to it', users.requests.length === 1 && users.requests[0].name === 'Untitled' && t?.loadedCollectionId === users.id && t?.loadedRequestId === users.requests[0].id, JSON.stringify(users.requests));
  check('…its row shows, selected, and the status bar has nothing to save yet', (await aside.locator('.tree-row.leaf[aria-current="page"]', { hasText: 'Untitled' }).count()) === 1 && (await p.locator('.statusbar').innerText()).includes('nothing to save yet'));
  check('…no "Save to…" on the request line (it is in a collection)', (await p.locator('hitop-request .save-draft').count()) === 0);
  await row('Untitled').click({ button: 'right' });
  await p.locator('#request-menu').getByText('Rename').click();
  await dialog.locator('#name').waitFor();
  await field('name', 'Ping');
  await dialog.getByRole('button', { name: 'Rename' }).click();
  await settle(p);
  users = await named('Users API');
  check('…renamed from the sidebar menu, the tab follows', users.requests[0].name === 'Ping' && (await h.stored.tabs()).tab?.title === 'Ping');
  await aside.getByRole('button', { name: 'Add a request to Users API' }).click();
  await settle(p);
  await p.locator('hitop-request .request-line .url input').first().fill('https://example.com/users');
  await settle(p);
  await p.keyboard.press('Control+KeyS');
  await settle(p);
  users = await named('Users API');
  check('Mod+S saves a new request in place under its automatic name (no dialog)', (await dialog.locator('#name').count()) === 0 && users.requests[1]?.name === 'GET /users' && users.requests[1].url === 'https://example.com/users' && (users.requests[1].bodyMode ?? 'raw') === 'raw', JSON.stringify(users.requests[1]));
  check('…and the sidebar shows it as the GET tag and /users', (await row('/users').innerText()).replace(/\s+/g, ' ').includes('GET /users'));
  await row('/users').click({ button: 'right' });
  await p.locator('#request-menu').getByText('Rename').click();
  await dialog.locator('#name').waitFor();
  await field('name', 'List');
  await dialog.getByRole('button', { name: 'Rename' }).click();
  await settle(p);
  users = await named('Users API');
  check('saved there, in order', users.requests.map((r) => r.name).join() === 'Ping,List', users.requests.map((r) => r.name).join());

  // --- folders from the sidebar: the collection menu, "+" on the folder, file paths ---
  await aside.getByRole('button', { name: 'More for Users API' }).click();
  await p.locator('#collection-menu').getByText('New folder…').click();
  await dialog.locator('#name').waitFor();
  await field('name', 'admin');
  await dialog.getByRole('button', { name: 'Create' }).click();
  await settle(p);
  const folderRow = aside.locator('.tree-row.folder', { hasText: 'admin' });
  check('a new folder shows in the sidebar before any request is in it', (await folderRow.count()) === 1 && (await aside.locator('.tree-hint', { hasText: 'No requests yet' }).count()) >= 1);
  await aside.getByRole('button', { name: 'Add a request to admin' }).click();
  await settle(p);
  users = await named('Users API');
  check('"+" on a folder adds the request in it: file = admin/untitled', users.requests.find((r) => r.name === 'Untitled')?.file === 'admin/untitled', JSON.stringify(users.requests.map((r) => [r.name, r.file])));
  await p.locator('hitop-request .request-line .url input').first().fill('https://example.com/me');
  await p.keyboard.press('Control+KeyS');
  await settle(p);
  users = await named('Users API');
  check('…its first save names its file after it: admin/get-me (the folder format path)', users.requests.find((r) => r.name === 'GET /me')?.file === 'admin/get-me', JSON.stringify(users.requests.map((r) => [r.name, r.file])));
  await row('/me').click({ button: 'right' });
  await p.locator('#request-menu').getByText('Rename').click();
  await dialog.locator('#name').waitFor();
  await field('name', 'Me');
  await dialog.getByRole('button', { name: 'Rename' }).click();
  await settle(p);
  check('…renamed, shown under the folder (the file keeps its path)', (await named('Users API')).requests.find((r) => r.name === 'Me')?.file === 'admin/get-me' && (await aside.locator('.tree-folder .tree-row.leaf', { hasText: 'Me' }).count()) === 1);

  await aside.getByRole('button', { name: 'More for folder admin' }).click();
  await p.locator('#folder-menu').getByText('Rename…').click();
  await dialog.locator('#name').waitFor();
  await field('name', 'staff');
  await dialog.getByRole('button', { name: 'Rename' }).click();
  await settle(p);
  users = await named('Users API');
  check('renaming a folder moves its requests\' files', users.requests.find((r) => r.name === 'Me')?.file === 'staff/get-me' && (await aside.locator('.tree-row.folder', { hasText: 'staff' }).count()) === 1);

  // --- Move to… from the request menu: into a folder, then to another collection ---
  await row('List').click({ button: 'right' });
  await p.locator('#request-menu').getByText('Move to…').click();
  await dialog.locator('#target').waitFor();
  const targets = await dialog.locator('#target lit-material-select-option').allInnerTexts();
  check('Move to… lists collections and their folders', targets.some((t) => t.trim() === 'Users API') && targets.some((t) => t.includes('Users API › staff')) && targets.some((t) => t.trim() === 'My requests'), targets.join('|'));
  await pick('target', `${users.id}|staff`);
  await dialog.getByRole('button', { name: 'Move' }).click();
  await settle(p);
  users = await named('Users API');
  check('moved into the folder: file = staff/list, order kept', users.requests.find((r) => r.name === 'List')?.file === 'staff/list' && users.requests.map((r) => r.name).join() === 'Ping,Me,List', JSON.stringify(users.requests.map((r) => [r.name, r.file])));
  const def = await named('My requests');
  await row('Ping').click({ button: 'right' });
  await p.locator('#request-menu').getByText('Move to…').click();
  await dialog.locator('#target').waitFor();
  await pick('target', `${def.id}|`);
  await dialog.getByRole('button', { name: 'Move' }).click();
  await settle(p);
  const [defAfter, usersAfter] = [await named('My requests'), await named('Users API')];
  check('moved to another collection: gone here, there without a file, its tab follows', !usersAfter.requests.some((r) => r.name === 'Ping') && defAfter.requests.at(-1)?.name === 'Ping' && !('file' in defAfter.requests.at(-1)) && (await h.stored.tabs()).tabs.some((t) => t.title === 'Ping' && t.loadedCollectionId === def.id));

  // --- drag a request onto another collection's list (lit-material sortable group) ---
  const handle = aside.locator('.request-item', { hasText: 'Me' }).locator('.sort-handle');
  await aside.locator('.request-item', { hasText: 'Me' }).hover();
  const from = await handle.boundingBox();
  const target = await aside.locator(`lit-material-sortable[data-collection="${def.id}"][data-folder=""]`).boundingBox();
  await p.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await p.mouse.down();
  await p.mouse.move(from.x + from.width / 2, from.y + from.height / 2 + 8, { steps: 3 });
  await p.mouse.move(target.x + target.width / 2, target.y + 4, { steps: 12 });
  await p.mouse.move(target.x + target.width / 2, target.y + 6, { steps: 2 });
  await p.mouse.up();
  await settle(p);
  const defDrop = await named('My requests');
  check('dragging a request onto another collection moves it there (file dropped at the top level)', defDrop.requests.some((r) => r.name === 'Me' && !r.file) && !(await named('Users API')).requests.some((r) => r.name === 'Me'), JSON.stringify(defDrop.requests.map((r) => [r.name, r.file])));

  // --- the palette knows the new actions ---
  await p.keyboard.press('Control+KeyK');
  await p.waitForTimeout(300);
  await p.keyboard.type('>new collection');
  await p.waitForTimeout(300);
  await p.keyboard.press('Enter');
  await dialog.locator('#name').waitFor();
  check('palette: "New collection" opens the dialog', (await dialog.innerText()).includes('New collection'));
  await field('name', 'Empty one');
  await dialog.getByRole('button', { name: 'Create' }).click();
  await settle(p);
  const empty = await named('Empty one');
  check('a new collection opens its page, which offers New request', (await p.locator('main h1').first().innerText()) === 'Empty one' && (await p.locator('main lit-material-empty-state').getByRole('button', { name: 'New request' }).count()) === 1);
  check('…and the sidebar row says "No requests yet" with a way in', (await aside.locator('.tree-hint', { hasText: 'No requests yet' }).count()) >= 1);
  await p.locator('main .page-actions').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  check('New request on a collection page adds the request to that collection', (await h.stored.tabs()).tab?.loadedCollectionId === empty.id && (await named('Empty one')).requests.length === 1);
  await p.evaluate(() => document.querySelector('hitop-app').go('collections')); // UX2: the overview (crumb menu → All collections)
  await settle(p);
  await aside.locator('.tree-row.collection', { hasText: 'Users API' }).click();
  await settle(p);
  await aside.locator('.new-request').click();
  await settle(p);
  check('the sidebar\'s New request uses the collection on screen, not the first one', (await h.stored.tabs()).tab?.loadedCollectionId === users.id);
  await p.keyboard.press('Control+KeyK');
  await p.waitForTimeout(300);
  await p.keyboard.type('>move the request');
  await p.waitForTimeout(300);
  const moveItem = await p.locator('lit-material-command-palette').evaluate((el) => {
    const items = [...(el.shadowRoot ?? el).querySelectorAll('[role="option"], li, .item')];
    return items.map((i) => `${i.textContent.trim().slice(0, 60)}${i.getAttribute('aria-disabled') === 'true' || i.hasAttribute('disabled') ? ' [disabled]' : ''}`).find((t) => t.includes('Move the request')) || '';
  });
  check('palette: Move the request… is there, on for a new request (it is in a collection)', moveItem.includes('Move the request') && !moveItem.includes('[disabled]'), moveItem);
  await p.keyboard.press('Escape');

  // --- storage stays the React app's shape ---
  const cols = await h.stored.collections();
  check('collections keep the shared shape (numeric ids, requests arrays, no new top-level keys)', cols.every((c) => typeof c.id === 'number' && Array.isArray(c.requests) && Object.keys(c).every((k) => ['id', 'name', 'requests', 'variables', 'auth', 'org', 'workspace', 'createdAt', 'updatedAt'].includes(k))), JSON.stringify(cols.map((c) => Object.keys(c))));
  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
