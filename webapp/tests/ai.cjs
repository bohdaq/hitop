// Ask AI (src/ai.js, views/ai-view.js) against a mocked Claude API (no key, no network): the key
// lives in Settings on this device; Write tests streams a script and adds it to Script after;
// a request from text opens as a request; what is sent hides secret values and credential
// headers; without a key the dialog points to Settings.
//   BASE=http://localhost:7879 node tests/ai.cjs
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p, ms = 400) => p.waitForTimeout(ms);

// A streamed Messages API answer (SSE) whose text is `text`.
function sse(text, model = 'claude-opus-5-5') {
  const ev = (type, data) => `event: ${type}\ndata: ${JSON.stringify({ type, ...data })}\n\n`;
  const half = Math.ceil(text.length / 2);
  return (
    ev('message_start', { message: { id: 'msg_1', type: 'message', role: 'assistant', model, content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 10, output_tokens: 0 } } }) +
    ev('content_block_start', { index: 0, content_block: { type: 'text', text: '' } }) +
    ev('content_block_delta', { index: 0, delta: { type: 'text_delta', text: text.slice(0, half) } }) +
    ev('content_block_delta', { index: 0, delta: { type: 'text_delta', text: text.slice(half) } }) +
    ev('content_block_stop', { index: 0 }) +
    ev('message_delta', { delta: { stop_reason: 'end_turn', stop_sequence: null }, usage: { output_tokens: 20 } }) +
    ev('message_stop', {})
  );
}

(async () => {
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const sent = [];
  let answer = '';
  await p.route('https://api.anthropic.com/**', async (route) => {
    sent.push({ headers: await route.request().allHeaders(), body: JSON.parse(route.request().postData() || '{}') });
    await route.fulfill({ status: 200, headers: { 'content-type': 'text/event-stream', 'access-control-allow-origin': '*' }, body: sse(answer) });
  });
  await p.goto(h.url, { waitUntil: 'networkidle' });

  // No key: the dialog says where to add one.
  await p.getByRole('button', { name: 'Ask AI' }).click();
  await settle(p);
  check('without a key Ask AI points to Settings', (await p.locator('#dialog').innerText()).includes('Add one in Settings') && sent.length === 0);
  await p.locator('#dialog').getByRole('button', { name: 'Open Settings' }).click();
  await settle(p);
  await p.locator('section[aria-label="AI assistant"]').getByLabel('Anthropic API key').fill('sk-ant-test-key');
  await p.keyboard.press('Tab');
  await settle(p);
  check('the key is kept on this device', (await p.evaluate(() => JSON.parse(localStorage.getItem('hitop_ai')).key)) === 'sk-ant-test-key');

  // A response with a secret in it, then Write tests.
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.show('request');
    s.updateTab({ url: 'https://api.example.test/orders/7', headers: [{ name: 'Authorization', value: 'Bearer live-token-123' }, { name: '', value: '' }] });
    s.patchTab(s.tab.id, { statusCode: 200, responseType: 'json', response: '{"id":7,"status":"paid"}', responseHeaders: { 'content-type': 'application/json' } });
    s.emit();
  });
  await settle(p);
  answer = 'pm.test("status is 200", () => pm.response.to.have.status(200));\npm.test("order 7", () => pm.expect(pm.response.json().id).to.eql(7));';
  await p.getByRole('button', { name: 'Ask AI' }).click();
  await p.locator('#dialog').getByRole('button', { name: 'Add to Script after' }).waitFor({ timeout: 10000 });
  const req = sent.at(-1);
  const prompt = JSON.stringify(req.body.messages);
  check('Write tests sends the response, the key in its header, Opus 5.5 with the fallback', req.headers.authorization === 'Bearer sk-ant-test-key' && req.body.model === 'claude-opus-5-5' && req.body.fallbacks === 'default' && prompt.includes('"status\\":\\"paid'), JSON.stringify(req.body).slice(0, 300));
  check('...without credential headers', !prompt.includes('live-token-123') && prompt.includes('Authorization: ••••'));
  await p.locator('#dialog').getByRole('button', { name: 'Add to Script after' }).click();
  await settle(p);
  check('Add to Script after puts the tests there', (await p.evaluate(() => document.querySelector('hitop-app').store.tab.postRequestScript)).includes('order 7'));

  // A request from text.
  answer = JSON.stringify({ name: 'Create user', method: 'POST', url: '${baseUrl}/users', headers: [{ name: 'Content-Type', value: 'application/json' }], body: '{"name":"Ada"}' });
  await p.getByRole('button', { name: 'Ask AI' }).click();
  await settle(p);
  await p.locator('#dialog').getByRole('tab', { name: 'Request from text' }).click();
  await p.locator('#dialog').getByLabel(/curl command/).fill("curl -X POST https://api.example.test/users -d '{\"name\":\"Ada\"}'");
  await p.locator('#dialog').getByRole('button', { name: /Make the request/ }).click();
  await p.locator('#dialog').getByRole('button', { name: 'Open as a request' }).click();
  await settle(p);
  const tab = await p.evaluate(() => { const t = document.querySelector('hitop-app').store.tab; return { m: t.method, u: t.url, b: t.requestBody }; });
  check('a request from text opens as a request', tab.m === 'POST' && tab.u === '${baseUrl}/users' && tab.b.includes('Ada') && sent.at(-1).body.output_config.format.type === 'json_schema', JSON.stringify(tab));

  check('no page errors', errors.length === 0, errors.join(' | '));
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
