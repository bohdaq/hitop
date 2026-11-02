// The desktop's sections of the Settings page, in Tauri mode: the usage
// ping's first-run question (No is the default) and switch, and the secret
// managers (values to secret-managers.json, tokens to the keychain only). The
// web app's Settings page has neither (tests/ux12.cjs checks the shared
// sections); a desktop build without the ping (Mac App Store) has the secret
// managers only, and no question.
//   HITOP_MODE=tauri node tests/settings.cjs   (web mode: BASE=... node tests/settings.cjs)
const fs = require('fs');
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
    check('web app: no usage ping question', (await p.locator('lit-material-confirm-dialog').count()) === 0);
    // Since 1.2 the web app has a Settings page too, without the desktop's sections.
    await settingsButton().click();
    await settle(p);
    const text = await p.locator('main').innerText();
    check('web app: Settings has theme, requests, storage and shortcuts', ['Appearance', 'Requests', 'Storage', 'Keyboard shortcuts'].every((t) => text.includes(t)), text.slice(0, 300));
    check('web app: Settings has no usage ping and no secret managers', !text.includes('Usage ping') && (await p.locator('section[aria-label="Secret managers"]').count()) === 0);
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

    // Secret managers: values to the settings file, the token to the keychain.
    const TOKEN = 'hvs.settings-e2e-s3cr3t';
    const section = p.locator('section[aria-label="Secret managers"]');
    check('[tauri] Settings has the secret managers', (await section.count()) === 1 && (await section.innerText()).includes('system keychain'));
    const fill = (id, v) => p.locator(`#${id}`).evaluate((el, x) => (el.value = x), v);
    await fill('sm-vault-addr', ' http://127.0.0.1:8200 ');
    await fill('sm-vault-token', TOKEN);
    await fill('sm-aws-profile', 'work');
    await fill('sm-hitop-op-bin', '/opt/1password/op');
    await p.getByRole('button', { name: 'Save secret manager settings' }).click();
    await settle(p);
    const file = JSON.parse(h.mock.read('secret-managers.json', '{}'));
    check('[tauri] values go to secret-managers.json, trimmed', JSON.stringify(file) === JSON.stringify({ vaultAddr: 'http://127.0.0.1:8200', awsProfile: 'work', opPath: '/opt/1password/op' }), JSON.stringify(file));
    check('[tauri] the token goes to the keychain only', h.mock.keychain.get('secret-managers/VAULT_TOKEN') === TOKEN && !h.mock.read('secret-managers.json', '').includes(TOKEN));
    const saved = await section.innerText();
    const tokenField = await p.locator('#sm-vault-token').evaluate((el) => ({ value: el.value, note: el.supportingText }));
    check('[tauri] saved token: field cleared, shown as saved, never its value', tokenField.value === '' && tokenField.note === 'Saved in your system keychain' && !saved.includes(TOKEN) && saved.includes('Saved.'), JSON.stringify(tokenField));
    const stores = fs.readdirSync(h.mock.config).map((f) => fs.readFileSync(require('path').join(h.mock.config, f), 'utf8')).join('\n');
    check('[tauri] no file in the settings folder holds the token', !stores.includes(TOKEN));

    // A blank token field leaves the saved one; Remove takes it out.
    await fill('sm-vault-addr', 'http://vault.test:8200');
    await p.getByRole('button', { name: 'Save secret manager settings' }).click();
    await settle(p);
    check('[tauri] saving again keeps the token', h.mock.keychain.get('secret-managers/VAULT_TOKEN') === TOKEN && JSON.parse(h.mock.read('secret-managers.json', '{}')).vaultAddr === 'http://vault.test:8200');
    await p.getByRole('button', { name: 'Remove VAULT_TOKEN' }).click();
    await settle(p);
    check('[tauri] Remove deletes it from the keychain', !h.mock.keychain.has('secret-managers/VAULT_TOKEN') && (await section.innerText()).includes('Removed VAULT_TOKEN'));

    // A variable the app's environment sets wins, and the page says so.
    h.mock.smShell = ['AWS_DEFAULT_REGION', 'VAULT_ADDR'];
    await p.reload({ waitUntil: 'networkidle' });
    await settle(p);
    await settingsButton().click();
    await settle(p);
    const notes = await p.evaluate(() => Object.fromEntries(['sm-aws-region', 'sm-vault-addr', 'sm-aws-profile'].map((id) => [id, document.querySelector(`#${id}`)?.supportingText ?? null])));
    check(
      "[tauri] the environment's variables are named as winning",
      notes['sm-aws-region'] === "AWS_DEFAULT_REGION is set in this app's environment: that value wins" && notes['sm-vault-addr'].startsWith('VAULT_ADDR is set') && notes['sm-aws-profile'] === '',
      JSON.stringify(notes),
    );
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
    const button = m.page.locator('aside').getByRole('button', { name: 'Settings', exact: true });
    const hasPage = (await button.count()) === 1;
    if (hasPage) await button.click();
    await m.page.waitForTimeout(400);
    const text = hasPage ? await m.page.locator('main').innerText() : '';
    r2.push(`${hasPage && text.includes('Secret managers') && !text.includes('Usage ping') ? 'PASS' : 'FAIL'} [tauri] build without the ping: Settings has the secret managers only`);
    r2.push(`${(await m.page.locator('lit-material-confirm-dialog[open]').count()) === 0 && !m.mock.calls.includes('telemetry_set') ? 'PASS' : 'FAIL'} [tauri] build without the ping: no question`);
    await m.finish();
  }
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
