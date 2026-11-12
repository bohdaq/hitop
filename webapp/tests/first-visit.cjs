// FV1 (HITOP_NEXT_UI.md): the first visit and new requests that land in a collection, in web and
// Tauri mode.
//   BASE=http://localhost:7879 node tests/first-visit.cjs
//   HITOP_MODE=tauri node tests/first-visit.cjs
const fs = require('fs');
const os = require('os');
const path = require('path');
const { launch, checker, inApp, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);
const mod = process.platform === 'darwin' ? 'Meta' : 'Control';
const SAMPLE = 'Sample: JSONPlaceholder';

(async () => {
  // --- a first visit: nothing stored ---
  const h = await launch({ viewport: { width: 1440, height: 900 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await settle(p);
  const side = p.locator('aside nav.tree');
  const collectionRows = (await side.locator('.tree-row.collection').allInnerTexts()).map((t) => t.trim());
  check('first visit: the sample and an empty My requests', collectionRows.length === 2 && collectionRows[0] === SAMPLE && collectionRows[1] === 'My requests', collectionRows.join());
  const tabs = (await p.locator('.tab-strip lit-material-tab').allInnerTexts()).map((t) => t.replace(/\s+/g, ' ').trim());
  check('...with "List posts" open', tabs.length === 1 && tabs[0].includes('List posts'), tabs.join());
  const focused = await p.evaluate(() => Boolean(document.activeElement?.closest?.('.request-line .url')));
  check('...and the URL field focused', focused);
  check('...its sidebar row selected', (await side.locator('.tree-row.leaf[aria-current="page"]').innerText()).includes('List posts'));
  check('Get started stays in the sidebar', await p.locator('.getting-started').isVisible());
  const empty = await p.locator('.response.empty').innerText();
  check('the empty response pane says to press Send, with the proxy note, and offers no sample/import', empty.includes('Press Send (') && empty.includes('to see the response here') && (TAURI || empty.includes('proxy')) && !/Try a sample|Import from Postman/.test(empty), empty.replace(/\n/g, ' | '));
  const mine = side.locator('li', { has: p.locator('.tree-row.collection', { hasText: 'My requests' }) });
  const hint = mine.locator('.tree-hint');
  check('the empty My requests row offers Add request and Import', (await hint.getByRole('button', { name: 'Add request' }).count()) === 1 && (await hint.getByRole('button', { name: /^Import Postman/ }).count()) === 1);
  check('the request editor opens on Params for the sample request (its URL has a query)', (await p.locator('.sub-tabs lit-material-tab[selected]').innerText()).includes('Params'));
  check('the status bar does not repeat the collection and request (the title bar has them)', !(await p.locator('.statusbar').innerText()).includes('›'), await p.locator('.statusbar').innerText());
  const envText = await p.locator('header .env-select').evaluate((el) => el.shadowRoot?.textContent ?? el.textContent);
  check('the environment picker says "No environments"', envText.includes('No environments'), envText.replace(/\s+/g, ' ').slice(0, 120));
  check('...and offers "Add environment…"', (await p.locator('header .env-select lit-material-select-option', { hasText: 'Add environment' }).count()) === 1);
  await p.locator('header .env-select').evaluate((el) => {
    el.value = '__add_environment';
    el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await settle(p);
  check('...which opens the Environments page', (await p.locator('main h1').first().innerText()).includes('Environments'));
  await p.keyboard.press(`${mod}+l`); // back to the request (Go to the URL field)
  await settle(p);
  const firstStored = await h.stored.collections();
  check('the first visit stores the two collections', firstStored.length === 2 && firstStored[1].name === 'My requests' && firstStored[1].requests.length === 0, firstStored.map((c) => c.name).join());

  // --- + adds "Untitled" to the collection on screen ---
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  let cols = await h.stored.collections();
  let sample = cols.find((c) => c.name === SAMPLE);
  check('+ adds "Untitled" to the current collection (the sample)', sample.requests.length === 4 && sample.requests[3].name === 'Untitled' && sample.requests[3].url === '', sample.requests.map((r) => r.name).join());
  const selected = side.locator('.tree-row.leaf[aria-current="page"]');
  check('...its sidebar row appears and is selected', (await selected.count()) === 1 && (await selected.innerText()).includes('Untitled'), await selected.innerText().catch(() => ''));
  const tab = (await h.stored.tabs()).tab;
  check('...the tab is linked to it', tab.loadedRequestId === sample.requests[3].id && tab.loadedCollectionId === sample.id);
  check('...the URL field is focused', await p.evaluate(() => Boolean(document.activeElement?.closest?.('.request-line .url'))));
  check('...the editor opens on Params', (await p.locator('.sub-tabs lit-material-tab[selected]').innerText()).includes('Params'));
  check('...no unsaved dot while it is untouched', (await p.locator('.tab-strip lit-material-tab[selected] .dirty-dot').count()) === 0);
  await p.keyboard.type('https://api.example.com/users?page=2');
  await settle(p);
  const title = (await p.locator('.tab-strip lit-material-tab[selected]').innerText()).replace(/\s+/g, ' ').trim();
  check('typing a URL names it "GET /users"', title.startsWith('GET /users'), title);
  check('...the sidebar row follows (the GET tag and /users)', (await selected.innerText()).replace(/\s+/g, ' ').includes('GET /users'), await selected.innerText());
  check('...and shows the unsaved dot', (await p.locator('.tab-strip lit-material-tab[selected] .dirty-dot').count()) === 1 && (await selected.locator('.dirty-dot').count()) === 1);
  check('...the status bar says unsaved', (await p.locator('.statusbar').innerText()).includes('Unsaved'));
  await p.keyboard.press(`${mod}+s`);
  await settle(p);
  sample = (await h.stored.collections()).find((c) => c.name === SAMPLE);
  check('Mod+S saves it under its automatic name', sample.requests[3].name === 'GET /users' && sample.requests[3].url === 'https://api.example.com/users?page=2' && (await p.locator('#dialog #name').count()) === 0, JSON.stringify(sample.requests[3]).slice(0, 200));
  check('...the dot goes and the status bar says saved', (await p.locator('.tab-strip lit-material-tab[selected] .dirty-dot').count()) === 0 && (await p.locator('.statusbar').innerText()).includes('Saved'));
  const url = p.locator('hitop-request .request-line .url');
  await url.click();
  await p.keyboard.press(`${mod}+a`);
  await p.keyboard.type('https://api.example.com/users/42');
  await settle(p);
  check('until renamed, the name keeps following the URL', (await p.locator('.tab-strip lit-material-tab[selected]').innerText()).includes('GET /users/42'));
  await p.keyboard.press(`${mod}+s`);
  await settle(p);
  await p.locator('#tab-actions').click();
  await p.locator('#tab-menu lit-material-list-item', { hasText: 'Rename' }).click();
  await p.locator('#dialog #name').waitFor();
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'One user'));
  await p.locator('#dialog').getByRole('button', { name: 'Rename' }).click();
  await settle(p);
  await url.click();
  await p.keyboard.press('End');
  await p.keyboard.type('/x');
  await settle(p);
  const renamed = (await p.locator('.tab-strip lit-material-tab[selected]').innerText()).replace(/\s+/g, ' ');
  sample = (await h.stored.collections()).find((c) => c.name === SAMPLE);
  check('a rename stops that: the name stays', renamed.includes('One user') && sample.requests[3].name === 'One user', renamed);
  await p.keyboard.press(`${mod}+s`);
  await settle(p);

  // --- closing an untouched new request takes it out again ---
  const count = async () => (await h.stored.collections()).find((c) => c.name === SAMPLE).requests.length;
  const before = await count();
  await p.keyboard.press('Alt+Shift+KeyN');
  await settle(p);
  check('Alt+Shift+N adds one too', (await count()) === before + 1);
  await p.keyboard.press('Alt+Shift+KeyW');
  await settle(p);
  check('closing it untouched removes it, without asking', (await count()) === before && (await p.locator('lit-material-confirm-dialog[open]').count()) === 0);
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  await p.keyboard.type('https://api.example.com/draft');
  await settle(p);
  await p.keyboard.press('Alt+Shift+KeyW');
  const ask = p.locator('lit-material-confirm-dialog[open]');
  await ask.getByRole('button', { name: 'Discard' }).waitFor({ timeout: 5000 });
  const askText = (await p.getByText(/never saved/).isVisible().catch(() => false)) ? 'never saved' : '';
  await ask.getByRole('button', { name: 'Discard' }).click();
  await settle(p);
  check('one with an unsaved URL asks first, then comes out too', (await count()) === before && /never saved/.test(askText), askText);

  // --- the palette and My requests' own Add request ---
  await p.keyboard.press(`${mod}+k`);
  await p.locator('lit-material-command-palette').getByRole('combobox').waitFor({ timeout: 5000 });
  await p.keyboard.type('>New request in', { delay: 20 });
  await settle(p);
  await p.keyboard.press('Enter');
  await settle(p);
  check('the palette\'s New request adds one too', (await count()) === before + 1);
  await p.keyboard.press('Alt+Shift+KeyW');
  await settle(p);
  await hint.getByRole('button', { name: 'Add request' }).click();
  await settle(p);
  cols = await h.stored.collections();
  const myReq = cols.find((c) => c.name === 'My requests');
  check('My requests\' Add request adds it there', myReq.requests.length === 1 && myReq.requests[0].name === 'Untitled' && (await h.stored.tabs()).tab.loadedCollectionId === myReq.id);
  await p.keyboard.press('Alt+Shift+KeyW');
  await settle(p);
  check('...and closing it leaves My requests empty again (its hints back)', (await h.stored.collections()).find((c) => c.name === 'My requests').requests.length === 0 && (await hint.getByRole('button', { name: /^Import Postman/ }).count()) === 1);

  // --- a collection that can't take a request: My requests instead ---
  // A viewer's shared collection: the role comes from /v1/workspaces (orgs.js → store.setOrgs);
  // here the sample is made to look like one through the same store calls.
  const ro = await inApp(p, `
    const s = app.store;
    const col = s.collections.find((x) => x.name === a);
    s.folders.set(col.id, { ref: 'hitop-sync:ws-ro', label: 'Shared', connected: false, error: null, written: null, secrets: {} });
    c.update((d) => (d.links[col.id] = { workspace: 'ws-ro', ref: 'hitop-sync:ws-ro' }));
    s.setOrgs([], new Map(), new Map([['ws-ro', 'viewer']]));
    s.openRequest(col.requests[0], col.id);
    return s.canTakeRequest(col);`, SAMPLE);
  check('a viewer\'s shared collection can\'t take a request', ro.ok && ro.value === false, JSON.stringify(ro));
  const sampleCount = await count();
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);
  cols = await h.stored.collections();
  check('+ there falls back to My requests', (await count()) === sampleCount && cols.find((c) => c.name === 'My requests').requests.length === 1 && (await h.stored.tabs()).tab.loadedCollectionId === cols.find((c) => c.name === 'My requests').id);
  const toast = await p.locator('#toast').evaluate((el) => [el.message, el.textContent, el.shadowRoot?.textContent].filter(Boolean).join(' '));
  check('...and says so', toast.includes("can't take new requests here") && toast.includes('My requests'), toast);
  await inApp(p, `
    const s = app.store;
    const col = s.collections.find((x) => x.name === a);
    s.folders.delete(col.id);
    c.update((d) => delete d.links[col.id]);
    s.setOrgs([], new Map(), new Map());`, SAMPLE);
  check('no page errors (first visit)', errors.length === 0, errors.join(' | '));
  await h.browser.close();
  h.mock?.cleanup();

  // --- an old draft in storage is filed into My requests; an empty one is dropped ---
  const work = { id: 501, name: 'Work', requests: [{ id: 601, name: 'Saved one', url: 'https://work.example.com/a', method: 'GET', headers: [], body: '', preRequestScript: '', postRequestScript: '' }] };
  const draftTab = (id, url, title) => ({ id, title, url, method: 'POST', headers: [{ name: '', value: '' }], requestBody: url ? '{"a":1}' : '', response: null, responseHeaders: null, statusCode: null, responseType: '', loading: false, preRequestScript: '', postRequestScript: '', loadedRequestId: null, loadedCollectionId: null });
  const savedTab = { ...draftTab(701, 'https://work.example.com/a', 'Saved one'), method: 'GET', requestBody: '', loadedRequestId: 601, loadedCollectionId: 501 };
  const oldTabs = [savedTab, draftTab(702, 'https://api.example.com/orders', 'POST api.example.com/orders'), draftTab(703, '', 'New request')];
  let seedDir = null;
  if (TAURI) {
    seedDir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-fv1-'));
    fs.writeFileSync(path.join(seedDir, 'collections.json'), JSON.stringify([work]));
  }
  const h2 = await launch({ seed: { hitop_collections: JSON.stringify([work]), hitop_tabs: JSON.stringify(oldTabs), hitop_current_tab: '1' }, tauriSeed: seedDir, results });
  if (TAURI) {
    await h2.ctx.addInitScript((tabsJson) => {
      if (!sessionStorage.getItem('seeded')) {
        localStorage.setItem('hitop_tabs', tabsJson);
        localStorage.setItem('hitop_current_tab', '1');
        sessionStorage.setItem('seeded', '1');
      }
    }, JSON.stringify(oldTabs));
  }
  const q = h2.page;
  await q.goto(h2.url, { waitUntil: 'networkidle' });
  await settle(q);
  const cols2 = await h2.stored.collections();
  const filed = cols2.find((c) => c.name === 'My requests');
  check('stored collections: no first-visit sample (storage was not empty)', !cols2.some((c) => c.name === SAMPLE), cols2.map((c) => c.name).join());
  check('an old draft with content is filed into a new My requests', filed?.requests.length === 1 && filed.requests[0].url === 'https://api.example.com/orders' && filed.requests[0].method === 'POST' && filed.requests[0].body === '{"a":1}' && filed.requests[0].name === 'POST /orders', JSON.stringify(filed?.requests));
  const t2 = await h2.stored.tabs();
  check('...its tab is linked to it; the empty draft is dropped; the saved tab stays', t2.tabs.length === 2 && t2.tabs[1].loadedRequestId === filed?.requests[0].id && t2.tabs[1].loadedCollectionId === filed?.id && t2.tabs[0].loadedRequestId === 601, JSON.stringify(t2.tabs.map((t) => [t.title, t.loadedRequestId])));
  check('...and it opens saved (no unsaved dot)', (await q.locator('.tab-strip lit-material-tab .dirty-dot').count()) === 0);
  await h2.finish();
  if (seedDir) fs.rmSync(seedDir, { recursive: true, force: true });
  if (results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
