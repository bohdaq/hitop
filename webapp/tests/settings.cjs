// The desktop's usage ping: the first-run question (No is the default) and
// the Settings page, in Tauri mode; the web app has neither, and neither has
// a desktop build without the ping (Mac App Store).
//   HITOP_MODE=tauri node tests/settings.cjs   (web mode: BASE=... node tests/settings.cjs)
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

(async () => {
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const settingsButton = () => p.locator('aside').getByRole('button', { name: 'Settings', exact: true });

  if (!TAURI) {
    await p.goto(h.url, { waitUntil: 'networkidle' });
    await settle(p);
    check('web app: no Settings page', (await settingsButton().count()) === 0);
    check('web app: no usage ping question', (await p.locator('lit-material-confirm-dialog').count()) === 0);
  } else {
    h.mock.telemetry.asked = false;
    await p.goto(h.url, { waitUntil: 'networkidle' });
    const dialog = p.locator('lit-material-confirm-dialog[open]');
    await dialog.waitFor({ state: 'attached', timeout: 10000 });
    await settle(p);
    const text = await dialog.innerText().catch(() => '');
    const shown = await dialog.evaluate((d) => ({ headline: d.headline, message: d.message, focused: d.shadowRoot.activeElement?.className, label: d.shadowRoot.activeElement?.textContent.trim() }));
    check('[tauri] first start asks about the usage ping', shown.headline === 'Send an anonymous usage ping?' && shown.message.includes('random id'), JSON.stringify(shown) + text);
    check('[tauri] No has the focus', shown.focused === 'cancel' && shown.label === 'No', JSON.stringify(shown));
    await p.keyboard.press('Escape');
    await settle(p);
    check('[tauri] Escape answers No', h.mock.telemetry.asked === true && h.mock.telemetry.enabled === false && h.mock.calls.includes('telemetry_set'), JSON.stringify(h.mock.telemetry));

    await p.reload({ waitUntil: 'networkidle' });
    await settle(p);
    check('[tauri] asked only once', (await p.locator('lit-material-confirm-dialog[open]').count()) === 0);

    await settingsButton().click();
    await settle(p);
    const off = await p.locator('main').innerText();
    check('[tauri] Settings shows the ping, off', off.includes('Usage ping') && off.includes('POST https://hitop.rws8.tech/api/ping') && off.includes('"surface": "desktop"') && off.includes('<a random id'), off.slice(0, 400));
    await p.getByRole('switch', { name: 'Send an anonymous usage ping once a week' }).click();
    await settle(p);
    const on = await p.locator('main').innerText();
    check('[tauri] switch turns it on and shows the id it sends', h.mock.telemetry.enabled === true && on.includes('"id": "0123456789abcdef0123456789abcdef"'), on.slice(0, 400));
    await p.getByRole('switch', { name: 'Send an anonymous usage ping once a week' }).click();
    await settle(p);
    check('[tauri] and off again', h.mock.telemetry.enabled === false && h.mock.telemetry.id === null);
  }
  check('no page errors', errors.length === 0, errors.join(' || '));
  await h.finish();

  if (TAURI) {
    // A desktop build without the ping: no question, no page.
    const r2 = [];
    const m = await launch({ results: r2 });
    m.mock.telemetry = null;
    m.mock.calls.length = 0;
    await m.page.goto(m.url, { waitUntil: 'networkidle' });
    await m.page.waitForTimeout(600);
    r2.push(`${(await m.page.locator('aside').getByRole('button', { name: 'Settings', exact: true }).count()) === 0 ? 'PASS' : 'FAIL'} [tauri] build without the ping: no Settings page`);
    r2.push(`${(await m.page.locator('lit-material-confirm-dialog[open]').count()) === 0 && !m.mock.calls.includes('telemetry_set') ? 'PASS' : 'FAIL'} [tauri] build without the ping: no question`);
    await m.finish();
  }
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
