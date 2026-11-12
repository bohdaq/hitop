// gRPC requests. Web mode: calls go through hitop-server's gRPC bridge (server reflection, public
// servers only): Send is on, the page says so, and a server on localhost is refused with that
// reason; the request is still edited, saved in the model hitop-core reads and reopened.
// Tauri mode (the desktop build, mocked Rust side in tauri-mock.cjs): load
// the schema by reflection, service and method pickers, a message template,
// unary and server-streaming calls with metadata and auth filled in, an
// error status with trailers, a refused bidirectional method, and .proto
// files when the server has no reflection.
//   BASE=http://localhost:7879 node tests/grpc.cjs   (HITOP_MODE=tauri: the desktop build)
const path = require('path');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);
const FIXTURES = path.join(__dirname, '../../cli/crates/hitop-core/tests/fixtures/grpc');

(async () => {
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const type = async (text) => p.keyboard.type(text, { delay: 5 });
  const section = (name) => p.getByRole('tablist', { name: 'Request details' }).getByRole('tab', { name: new RegExp(`^${name}`) }).click();
  const log = () => p.locator('.grpc-log').innerText().catch(() => '');
  const waitLog = (text) => p.waitForFunction((t) => document.querySelector('.grpc-log')?.textContent.includes(t), text, { timeout: 10000 }).then(() => true, () => false);
  const chip = () => p.locator('.grpc-log-panel .status-chip').innerText();
  const pick = async (label, name) => {
    await p.getByRole('combobox', { name: label }).click();
    await p.getByRole('option', { name, exact: true }).click();
    await settle(p);
  };
  const setMessage = async (text) => {
    await p.locator('.grpc-message .cm-content').click();
    await p.keyboard.press('ControlOrMeta+a');
    await p.keyboard.press('Delete');
    await type(text);
    await settle(p);
  };

  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await settle(p);
  for (const [k, v] of [['grpcHost', '127.0.0.1:50051'], ['who', 'ada']]) {
    await p.getByRole('button', { name: 'Add global' }).click();
    await type(k);
    await p.keyboard.press('Tab');
    await type(v);
    await settle(p);
  }

  await p.locator('aside .new-request').click(); // the sidebar's +: a new request (FV1)
  await pick('Method', 'gRPC');
  const tabs = await p.getByRole('tablist', { name: 'Request details' }).locator('lit-material-tab').allTextContents();
  check('gRPC tab: Schema, Auth, Metadata, Message', tabs.map((t) => t.trim()).join() === 'Schema,Auth,Metadata,Message', tabs.join());
  const send = p.locator('.request-line').getByRole('button', { name: 'Send' });
  const line = await p.locator('hitop-request').innerText();
  if (TAURI) {
    check('[tauri] Send is on and there is no desktop-only note', !(await send.isDisabled()) && !line.includes('available in the desktop app'), line.slice(0, 300));
  } else {
    check('[web] Send is on, through the gRPC bridge', !(await send.isDisabled()) && !line.includes('available in the desktop app'), line.slice(0, 300));
  }

  await p.getByLabel('Server').fill('${grpcHost}');
  await section('Schema');
  await p.getByRole('switch', { name: 'Plaintext (no TLS)' }).click();
  await section('Metadata');
  await settle(p);
  await p.getByRole('button', { name: 'Add header' }).click();
  await type('x-tenant');
  await p.keyboard.press('Tab');
  await type('t-${who}');
  await section('Auth');
  await p.getByRole('combobox', { name: 'Type' }).click();
  await p.getByRole('option', { name: /Bearer/ }).click();
  await p.getByLabel('Token').fill('tok-${who}');
  await section('Message');
  await settle(p);

  if (TAURI) {
    await p.getByRole('button', { name: 'Load schema' }).click();
    await p.waitForFunction(() => document.querySelector('.grpc-schema-source'), null, { timeout: 10000 }).catch(() => {});
    const src = await p.locator('.grpc-schema-source').innerText().catch(() => '');
    const service = await p.getByRole('combobox', { name: 'Service' }).innerText().catch(() => '');
    check('[tauri] schema by reflection: Greeter picked, SayHello first', src.includes('2 services from server reflection v1') && service.includes('hitop.test.Greeter') && src.includes('hitop.test.HelloRequest'), `${src} | ${service}`);
    await p.getByRole('button', { name: 'Template' }).click();
    await settle(p);
    const tpl = await p.locator('.grpc-message .cm-content').innerText();
    check('[tauri] Template fills in the request message', tpl.includes('"mood": "MOOD_UNSPECIFIED"') && tpl.includes('"key": ""'), tpl);
    await setMessage('{"name": "${who}"}');
    await send.click();
    check('[tauri] unary call: headers, the message, status OK', (await waitLog('Status OK')) && (await log()).includes('x-served-by: hitop-test') && (await log()).includes('"message": "Hello, ada"') && (await chip()) === 'OK', await log());
    const c = h.mock.grpcCalls[h.mock.grpcCalls.length - 1];
    const meta = Object.fromEntries(c?.metadata || []);
    check(
      '[tauri] grpc_call got the filled-in call: address, plaintext, metadata, bearer auth, message',
      c?.address === '127.0.0.1:50051' && c.plaintext === true && meta['x-tenant'] === 't-ada' && meta.Authorization === 'Bearer tok-ada' && c.service === 'hitop.test.Greeter' && c.method === 'SayHello' && c.message === '{"name": "ada"}' && c.baseDir === null,
      JSON.stringify(c),
    );
    const times = await p.locator('.grpc-log time').allInnerTexts();
    check('[tauri] every event has a time', times.length >= 4 && times.every((t) => /^\d\d:\d\d:\d\d\.\d{3}$/.test(t)), times.join(' '));

    await pick('RPC method', 'StreamHellos (server stream)');
    await setMessage('{"name": "bo", "times": 3}');
    await p.getByRole('button', { name: 'Clear' }).click();
    await send.click();
    await waitLog('Status OK');
    const streamed = await p.locator('.grpc-event.message').count();
    check('[tauri] server streaming: three messages, then OK', streamed === 3 && (await log()).includes('Hello #2, bo') && (await p.locator('.grpc-log-panel').innerText()).includes('3 messages'), await log());

    await pick('RPC method', 'Fail');
    await send.click();
    check('[tauri] an error status with its trailers', (await waitLog('NOT_FOUND (5): no user named bo')) && (await log()).includes('x-reason: test') && (await chip()) === 'NOT_FOUND', await log());

    // Cancel: the chip says Cancelled (not the previous call's NOT_FOUND), and a late answer is ignored.
    h.mock.grpcDelayMs = 400;
    await send.click();
    await p.getByRole('button', { name: /Cancel/ }).click();
    const cancelled = (await chip()) === 'Cancelled' && (await p.locator('.grpc-log-panel .status-chip').getAttribute('data-state')) === 'cancelled';
    await p.waitForTimeout(600);
    check('[tauri] a cancelled call shows Cancelled (data-state) and nothing arrives after it', cancelled && (await chip()) === 'Cancelled' && h.mock.grpcCancelled.length > 0, `${await chip()} ${await log()}`);
    h.mock.grpcDelayMs = 20;

    await pick('RPC method', 'Chat (bidi)');
    const warn = await p.locator('.editor-panel').innerText();
    await send.click();
    check('[tauri] a bidirectional method: a note, and refused before calling', warn.includes('streams from the client') && (await waitLog('bidirectional streaming')) && (await chip()) === 'Failed', await log());

    await pick('RPC method', 'SayHello');
    await setMessage('{"name": ');
    await send.click();
    check('[tauri] invalid JSON is refused', await waitLog("isn't a valid hitop.test.HelloRequest"), await log());
    await setMessage('{"name": "${who}"}');

    // No reflection: the error is shown; .proto files work instead.
    h.mock.grpcServer.reflection = false;
    await p.getByRole('button', { name: 'Reload schema' }).click();
    await settle(p);
    check('[tauri] no reflection service: the error says so', (await p.locator('.grpc-schema-error').innerText().catch(() => '')).includes('no reflection service'));
    await section('Schema');
    await pick('Schema from', '.proto files');
    await p.getByLabel('.proto files').fill(path.join(FIXTURES, 'greeter.proto'));
    await p.getByLabel('.proto files').press('Tab');
    await section('Message');
    await p.getByRole('button', { name: 'Reload schema' }).click();
    await settle(p);
    const err = await p.locator('.grpc-schema-error').innerText().catch(() => '');
    check('[tauri] a missing import is reported', err.includes('common.proto'), err);
    await section('Schema');
    await p.getByLabel('Import paths').fill(path.join(FIXTURES, 'include'));
    await p.getByLabel('Import paths').press('Tab');
    await section('Message');
    await p.getByRole('button', { name: 'Reload schema' }).click();
    await settle(p);
    check('[tauri] schema from .proto files', (await p.locator('.grpc-schema-source').innerText().catch(() => '')).includes('from .proto files'));
    await send.click();
    check('[tauri] a call with the .proto schema', await waitLog('Hello, ada'), await log());
    const last = h.mock.grpcCalls[h.mock.grpcCalls.length - 1];
    check('[tauri] protos and import paths sent to Rust', last?.protos?.[0]?.endsWith('greeter.proto') && last.importPaths?.[0]?.endsWith('include'), JSON.stringify(last));
  } else {
    await p.getByLabel('Service').fill('hitop.test.Greeter');
    await p.getByLabel('RPC method').fill('SayHello');
    await setMessage('{"name": "${who}"}');
    await section('Schema');
    check('[web] the Schema section says reflection through the bridge', (await p.locator('.grpc-bridge-note').innerText()).includes('reflection'));
    await section('Message');
    await p.getByRole('button', { name: 'Load schema' }).click();
    await p.locator('.grpc-schema-error').waitFor({ timeout: 15000 });
    const why = await p.locator('.grpc-schema-error').innerText();
    check('[web] a server on localhost is refused by the bridge, with the reason', why.includes('private or reserved'), why);
  }

  // Saved in the model hitop-core reads (both modes).
  await p.getByRole('button', { name: 'Save request' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'Hello'));
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await settle(p);
  const r = await h.stored.collections().then((cols) => cols.flatMap((c) => c.requests).find((x) => x.name === 'Hello'));
  // Web: server reflection, so no .proto files.
  const want = TAURI
    ? { service: 'hitop.test.Greeter', method: 'SayHello', message: '{"name": "${who}"}', protos: [path.join(FIXTURES, 'greeter.proto')], importPaths: [path.join(FIXTURES, 'include')], plaintext: true }
    : { service: 'hitop.test.Greeter', method: 'SayHello', message: '{"name": "${who}"}', plaintext: true };
  check('saved: kind grpc, the grpc section, metadata as headers, auth', r?.kind === 'grpc' && JSON.stringify(r.grpc) === JSON.stringify(want) && r.url === '${grpcHost}' && r.headers.some((x) => x.name === 'x-tenant' && x.value === 't-${who}') && r.auth?.type === 'bearer', JSON.stringify(r));
  const listed = await p.locator('aside').innerText();
  check('the sidebar lists it as GRPC', /GRPC\s*Hello/.test(listed), listed.slice(0, 300));

  // After a reload: still a gRPC tab with its fields; no log kept.
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  const tab = (await h.stored.tabs()).tab;
  check('after a reload: gRPC tab with service, method, message; no log', tab?.kind === 'grpc' && tab.grpcService === 'hitop.test.Greeter' && tab.grpcMessage === '{"name": "${who}"}' && !JSON.stringify(tab).includes('Hello, ada') && (await p.locator('.grpc-log').count()) === 0, JSON.stringify(tab).slice(0, 400));

  // Back to HTTP: a GET tab again; the gRPC fields stay with the tab.
  await pick('Method', 'GET');
  check('switching back to GET shows the URL field again', (await p.getByLabel('URL').count()) === 1 && (await p.locator('.grpc-log-panel').count()) === 0);

  check('no page errors', errors.length === 0, errors.join('; '));
  await h.finish();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
