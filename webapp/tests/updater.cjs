// SU1 desktop self-update (docs/archive/HITOP_NEXT_UI.md SU1): the GitHub Release builds'
// updater (desktop/src-tauri/src/updates.rs, feature `updater`) driven by
// platform/tauri-updater.js. In Tauri mode the mock plays the updater and
// process plugins: no update; an update → the bar → Restart to update downloads
// (progress), installs and relaunches; Later; the switch off → no check; Check
// now; a failed check is one quiet line in Settings; the daily check; a build
// without the updater (Mac App Store) shows nothing. Web mode: no Updates anywhere.
//   HITOP_MODE=tauri node tests/updater.cjs   (web mode: BASE=... node tests/updater.cjs)
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);
const HOUR = 60 * 60 * 1000;

async function start(updater, { before } = {}) {
  const h = await launch({ results });
  if (h.mock) h.mock.updater = updater;
  const errors = [];
  h.page.on('pageerror', (e) => errors.push(e.message));
  if (before) await before(h);
  await h.page.goto(h.url, { waitUntil: 'networkidle' });
  await settle(h.page);
  return { h, p: h.page, errors };
}

const bar = (p) => p.locator('hitop-update-bar .update-bar');
const settings = async (p) => {
  await p.locator('aside').getByRole('button', { name: 'Settings', exact: true }).click();
  await settle(p);
  return p.locator('section[aria-label="Updates"]');
};
const noDialog = async (p) => (await p.locator('lit-material-confirm-dialog[open], lit-material-dialog[open]').count()) === 0;

(async () => {
  if (!TAURI) {
    const { h, p } = await start(null);
    const section = await settings(p);
    check('web app: Settings has no Updates section', (await section.count()) === 0);
    check('web app: no update bar', (await p.locator('hitop-update-bar').count()) === 0);
    await h.finish();
    return;
  }

  // A build without the updater (Mac App Store, .deb/.rpm): nothing about updates.
  {
    const { h, p, errors } = await start(null);
    const section = await settings(p);
    check('[tauri] no updater: no Updates section, no bar', (await section.count()) === 0 && (await p.locator('hitop-update-bar').count()) === 0);
    check('[tauri] no updater: no check or install calls', !h.mock.calls.some((c) => c.startsWith('plugin:updater') || c.startsWith('plugin:process')), h.mock.calls.filter((c) => c.includes('updater')).join());
    check('[tauri] no updater: no page errors', errors.length === 0, errors.join(' | '));
    await h.browser.close();
    h.mock.cleanup();
  }

  // Up to date: one check at startup, no bar; Settings says so and names this version.
  {
    const u = { version: '1.2.0', available: null };
    const { h, p, errors } = await start(u);
    check('[tauri] up to date: checked once at startup', u.checks === 1, String(u.checks));
    check('[tauri] up to date: no bar', (await bar(p).count()) === 0);
    const section = await settings(p);
    const text = await section.innerText();
    check('[tauri] Settings → Updates: version, switch on, up to date', text.includes('This is HITOP 1.2.0') && text.includes('HITOP is up to date') && (await section.getByRole('switch', { name: 'Check for updates automatically' }).evaluate((s) => s.checked)), text);
    check('[tauri] Updates is in the Settings index', (await p.locator('.settings-index').getByRole('button', { name: 'Updates' }).count()) === 1);
    check('[tauri] up to date: no page errors', errors.length === 0, errors.join(' | '));
    await h.browser.close();
    h.mock.cleanup();
  }

  // An update: the bar → Restart to update downloads with progress, installs, relaunches.
  {
    const u = { version: '1.2.0', available: '1.3.0' };
    const { h, p, errors } = await start(u);
    const b = bar(p);
    await b.waitFor({ timeout: 5000 }).catch(() => {});
    const text = (await b.innerText().catch(() => '')).replace(/\s+/g, ' ');
    check('[tauri] update: the bar says HITOP 1.3.0 is available', text.includes('HITOP 1.3.0 is available') && text.includes('Restart to update') && text.includes('Later'), text);
    check('[tauri] update: the bar is a status, not a dialog', (await b.getAttribute('role')) === 'status' && (await noDialog(p)));
    // Something unsaved just before: it reaches Rust before the download starts.
    await p.evaluate(() => {
      const app = document.querySelector('hitop-app');
      app.store.collections = [...app.store.collections, { id: 777, name: 'Saved before the update', requests: [] }];
      app.store.emit();
    });
    const seen = [];
    const watch = setInterval(() => b.innerText().then((t) => seen.push(t.replace(/\s+/g, ' ')), () => {}), 40);
    await b.getByRole('button', { name: 'Restart to update' }).click();
    for (let i = 0; i < 50 && !u.restarts; i++) await p.waitForTimeout(100);
    clearInterval(watch);
    const calls = h.mock.calls;
    check('[tauri] Restart to update installs the update it found', JSON.stringify(u.installs) === JSON.stringify([u.rids[0]]), JSON.stringify(u));
    check('[tauri] then relaunches through the process plugin', u.restarts === 1 && calls.lastIndexOf('plugin:process|restart') > calls.indexOf('plugin:updater|download_and_install'), calls.slice(-5).join());
    check('[tauri] the download shows progress', seen.some((t) => /Downloading HITOP 1\.3\.0… \d\.\d MB of 4\.0 MB/.test(t)), seen.slice(0, 4).join(' / '));
    check('[tauri] pending saves reach Rust before the download', calls.lastIndexOf('save_collections') < calls.indexOf('plugin:updater|download_and_install') && JSON.stringify(h.mock.stored('collections')).includes('Saved before the update'), calls.slice(-8).join());
    check('[tauri] update: no page errors', errors.length === 0, errors.join(' | '));
    await h.browser.close();
    h.mock.cleanup();
  }

  // Later: the bar goes away; Settings still offers the update.
  {
    const u = { version: '1.2.0', available: '1.3.0' };
    const { h, p } = await start(u);
    await bar(p).waitFor({ timeout: 5000 }).catch(() => {});
    await bar(p).getByRole('button', { name: 'Later' }).click();
    await settle(p);
    check('[tauri] Later hides the bar and installs nothing', (await bar(p).count()) === 0 && !u.installs, JSON.stringify(u));
    const section = await settings(p);
    const text = await section.innerText();
    check('[tauri] Settings still says 1.3.0 is available, with Restart to update', text.includes('HITOP 1.3.0 is available') && (await section.getByRole('button', { name: 'Restart to update' }).count()) === 1, text);
    await h.browser.close();
    h.mock.cleanup();
  }

  // The switch off: no check at the next start; Check now still checks.
  {
    const u = { version: '1.2.0', available: null };
    const { h, p } = await start(u);
    let section = await settings(p);
    await section.getByRole('switch', { name: 'Check for updates automatically' }).click();
    await settle(p);
    const stored = await p.evaluate(() => JSON.parse(localStorage.getItem('hitop_updates')));
    check('[tauri] switch off is kept (hitop_updates)', stored?.auto === false, JSON.stringify(stored));
    u.checks = 0;
    await p.reload({ waitUntil: 'networkidle' });
    await settle(p);
    check('[tauri] switch off: no check at startup', u.checks === 0, String(u.checks));
    section = await settings(p);
    check('[tauri] switch off: shown off, says so', !(await section.getByRole('switch', { name: 'Check for updates automatically' }).evaluate((s) => s.checked)) && (await section.innerText()).includes('Automatic checks are off'));
    u.available = '1.3.0';
    await section.getByRole('button', { name: 'Check now' }).click();
    await settle(p);
    const text = await section.innerText();
    check('[tauri] Check now checks and offers the update', u.checks === 1 && text.includes('HITOP 1.3.0 is available'), `${u.checks} ${text}`);
    check('[tauri] Check now: the bar appears too', (await bar(p).count()) === 1);
    await h.browser.close();
    h.mock.cleanup();
  }

  // A failed check: one quiet line in Settings, no bar, no dialog, no toast.
  {
    const u = { version: '1.2.0', available: null, checkError: 'error sending request for url (https://github.com/bohdaq/hitop/releases/latest/download/latest.json)\nCaused by: dns error' };
    const { h, p, errors } = await start(u);
    check('[tauri] failed check: no bar, no dialog', (await p.locator('hitop-update-bar .update-bar').count()) === 0 && (await noDialog(p)));
    check('[tauri] failed check: no toast', !(await p.locator('#toast').evaluate((t) => t.open ?? t.hasAttribute('open')).catch(() => false)));
    const section = await settings(p);
    const lines = await section.locator('.update-error').allInnerTexts();
    check('[tauri] failed check: one line in Settings', lines.length === 1 && lines[0].startsWith("Couldn't check for updates: error sending request") && !lines[0].includes('dns error'), JSON.stringify(lines));
    u.checkError = null;
    await section.getByRole('button', { name: 'Check now' }).click();
    await settle(p);
    check('[tauri] the next check clears the line', (await section.locator('.update-error').count()) === 0 && (await section.innerText()).includes('HITOP is up to date'));
    check('[tauri] failed check: no page errors', errors.length === 0, errors.join(' | '));
    await h.browser.close();
    h.mock.cleanup();
  }

  // A failed install: the app keeps running; the bar steps aside and Settings says why.
  {
    const u = { version: '1.2.0', available: '1.3.0', installError: 'signature verification failed' };
    const { h, p } = await start(u);
    await bar(p).waitFor({ timeout: 5000 }).catch(() => {});
    await bar(p).getByRole('button', { name: 'Restart to update' }).click();
    for (let i = 0; i < 30 && (await bar(p).count()); i++) await p.waitForTimeout(100);
    check('[tauri] failed install: no relaunch, bar gone, no dialog', !u.restarts && (await bar(p).count()) === 0 && (await noDialog(p)), JSON.stringify(u));
    const section = await settings(p);
    const lines = await section.locator('.update-error').allInnerTexts();
    check("[tauri] failed install: one line in Settings", lines.length === 1 && lines[0] === "The update didn't install: signature verification failed", JSON.stringify(lines));
    await h.browser.close();
    h.mock.cleanup();
  }

  // Every 24 hours while the app stays open.
  {
    const u = { version: '1.2.0', available: null };
    const { h, p } = await start(u, { before: (x) => x.page.clock.install() });
    for (let i = 0; i < 20 && !u.checks; i++) await p.clock.runFor(100);
    check('[tauri] daily: checked at startup', u.checks === 1, String(u.checks));
    await p.clock.runFor(23 * HOUR);
    check('[tauri] daily: not again within a day', u.checks === 1, String(u.checks));
    await p.clock.runFor(2 * HOUR);
    check('[tauri] daily: again after 24 hours', u.checks === 2, String(u.checks));
    await p.clock.runFor(24 * HOUR);
    check('[tauri] daily: and the day after', u.checks === 3, String(u.checks));
    await h.finish();
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
