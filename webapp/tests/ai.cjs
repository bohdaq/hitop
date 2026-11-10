// The agent (src/ai.js, src/agent.js, views/ai-view.js) in its side sheet, against a mocked Claude
// API (no key, no network): the key lives in Settings on this device; Write tests and Change this
// request arrive as a patch with a diff and a dry run against the last response, applied only
// when asked; a request from text opens as a request; what is sent hides secret values and
// credential headers, and a hidden value is never written back; without a key the sheet points
// to Settings. U14: an explanation of a failure leads to a proposed fix as a patch, nothing
// changes before Apply, and Run with checks applies it and sends the request with its tests
// (a local Node server answers).
//   BASE=http://localhost:7879 node tests/ai.cjs
const http = require('http');
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
  const hits = [];
  const api = http.createServer((req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    hits.push(`${req.method} ${req.url}`);
    if (req.url === '/orders/7') return res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end('{"id":7,"status":"paid"}');
    res.writeHead(404, { ...cors, 'Content-Type': 'application/json' }).end('{"error":"not found"}');
  });
  await new Promise((r) => api.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${api.address().port}`;
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
  const sheet = p.locator('#agent');
  const tab = () => p.evaluate(() => { const t = document.querySelector('hitop-app').store.tab; return { m: t.method, u: t.url, b: t.requestBody, h: t.headers, s: t.postRequestScript }; });

  // No key: the sheet says where to add one.
  await p.getByRole('button', { name: 'More options' }).click();
  await p.locator('#line-menu').getByText('Ask AI').click();
  await settle(p);
  check('Ask AI opens the agent side sheet', await sheet.evaluate((el) => el.open));
  check('without a key the agent points to Settings', (await sheet.innerText()).includes('Add one in Settings') && sent.length === 0);
  await sheet.getByRole('button', { name: 'Open Settings' }).click();
  await settle(p);
  await p.locator('section[aria-label="AI assistant"]').getByLabel('Anthropic API key').fill('sk-ant-test-key');
  await p.keyboard.press('Tab');
  await settle(p);
  check('the key is kept on this device', (await p.evaluate(() => JSON.parse(localStorage.getItem('hitop_ai')).key)) === 'sk-ant-test-key');

  // A response with a secret in it, then Write tests: a patch of Script after, dry-run first.
  await p.evaluate(() => {
    const s = document.querySelector('hitop-app').store;
    s.show('request');
    s.updateTab({ url: 'https://api.example.test/orders/7', headers: [{ name: 'Authorization', value: 'Bearer live-token-123' }, { name: '', value: '' }] });
    s.patchTab(s.tab.id, { statusCode: 200, responseType: 'json', response: '{"id":7,"status":"paid"}', responseHeaders: { 'content-type': 'application/json' } });
    s.emit();
  });
  await settle(p);
  answer = 'pm.test("status is 200", () => pm.response.to.have.status(200));\npm.test("order 7", () => pm.expect(pm.response.json().id).to.eql(7));';
  await p.getByRole('button', { name: 'More options' }).click();
  await p.locator('#line-menu').getByText('Ask AI').click();
  await sheet.getByRole('button', { name: 'Apply', exact: true }).waitFor({ timeout: 10000 });
  const req = sent.at(-1);
  const prompt = JSON.stringify(req.body.messages);
  check('Write tests sends the response, the key in its header, Opus 5.5 with the fallback', req.headers.authorization === 'Bearer sk-ant-test-key' && req.body.model === 'claude-opus-5-5' && req.body.fallbacks === 'default' && prompt.includes('"status\\":\\"paid'), JSON.stringify(req.body).slice(0, 300));
  check('...without credential headers', !prompt.includes('live-token-123') && prompt.includes('Authorization: ••••'));
  check('the tests arrive as a diff', (await sheet.locator('lit-material-diff').count()) === 1 && (await sheet.locator('lit-material-diff').evaluate((el) => el.modified.includes('order 7') && !el.original.includes('order 7'))));
  await sheet.locator('.agent-dry').waitFor({ timeout: 10000 });
  check('...dry-run against the last response before anything changes', (await sheet.locator('.agent-dry').innerText()).includes('2 of 2 pass') && !(await tab()).s?.includes('order 7'), await sheet.locator('.agent-dry').innerText());
  await sheet.getByRole('button', { name: 'Apply', exact: true }).click();
  await settle(p);
  check('Apply puts the tests in Script after', (await tab()).s.includes('order 7'));
  await p.keyboard.press('Escape');
  await settle(p);
  check('Escape closes the sheet', !(await sheet.evaluate((el) => el.open)));

  // Change this request: a patch with the hidden header kept and a hidden body refused.
  answer = JSON.stringify({
    summary: 'Use PATCH with a JSON body and add a check on the status.',
    changes: {
      method: 'PATCH',
      url: null,
      headers: [{ name: 'Authorization', value: 'Bearer ••••' }, { name: 'Content-Type', value: 'application/json' }],
      body: '{"status":"cancelled"}',
      preRequestScript: '// token: ••••',
      postRequestScript: null,
    },
  });
  await p.getByRole('button', { name: 'More options' }).click();
  await p.locator('#line-menu').getByText('Ask AI').click();
  await settle(p);
  await sheet.getByRole('tab', { name: 'Change this request' }).click();
  await sheet.getByLabel(/What should change/).fill('cancel the order with PATCH');
  await sheet.getByRole('button', { name: /Propose a change/ }).click();
  await sheet.locator('.agent-patch').waitFor({ timeout: 10000 });
  const edit = sent.at(-1);
  check('Change this request asks for a patch in a JSON schema, with the scripts and the instruction', edit.body.output_config.format.type === 'json_schema' && JSON.stringify(edit.body.messages).includes('cancel the order with PATCH') && JSON.stringify(edit.body.messages).includes('Script after the request'));
  check('the patch is shown with its summary and what was left out', (await sheet.locator('.agent-summary').innerText()).includes('PATCH') && (await sheet.locator('.agent-patch').innerText()).includes('Left out: preRequestScript'));
  check('nothing changed before Apply', (await tab()).m !== 'PATCH');
  await sheet.getByRole('button', { name: 'Discard' }).click();
  await settle(p);
  check('Discard drops the patch', (await sheet.locator('.agent-patch').count()) === 0 && (await tab()).m !== 'PATCH');
  await sheet.getByRole('button', { name: /Propose a change/ }).click();
  await sheet.getByRole('button', { name: 'Apply', exact: true }).waitFor({ timeout: 10000 });
  await sheet.getByRole('button', { name: 'Apply', exact: true }).click();
  await settle(p);
  const after = await tab();
  const auth = after.h.find((x) => x.name === 'Authorization');
  check('Apply changes the request; the hidden header keeps its real value', after.m === 'PATCH' && after.b === '{"status":"cancelled"}' && auth?.value === 'Bearer live-token-123' && after.h.some((x) => x.name === 'Content-Type'), JSON.stringify(after));

  // U14: Explain a failure, then Propose a fix: the fix is a patch; Run with checks applies and sends.
  await p.evaluate((b) => {
    const s = document.querySelector('hitop-app').store;
    s.updateTab({ url: `${b}/order/7` });
    s.patchTab(s.tab.id, { statusCode: 404, responseType: 'json', response: '{"error":"not found"}', responseHeaders: { 'content-type': 'application/json' }, tests: [{ name: 'status is 200', pass: false, detail: 'got 404' }] });
    s.emit();
  }, base);
  await settle(p);
  answer = 'The path is /order/7 but the orders live under /orders/7, so the server answers 404.';
  await sheet.getByRole('tab', { name: 'Explain' }).click();
  await sheet.getByRole('button', { name: /Propose a fix/ }).waitFor({ timeout: 10000 });
  check('U14 an explanation offers a fix', (await sheet.locator('.ai-answer').innerText()).includes('/orders/7'));
  answer = JSON.stringify({ summary: 'Use the /orders path.', changes: { method: null, url: `${base}/orders/7`, headers: null, body: null, preRequestScript: null, postRequestScript: null } });
  await sheet.getByRole('button', { name: /Propose a fix/ }).click();
  await sheet.locator('.agent-patch').waitFor({ timeout: 10000 });
  const fixAsk = JSON.stringify(sent.at(-1).body.messages);
  check('U14 the fix is asked as a patch, with the explanation as the instruction', sent.at(-1).body.output_config?.format?.type === 'json_schema' && fixAsk.includes('Fix this request so that it succeeds') && fixAsk.includes('orders live under /orders/7'));
  check('U14 ...shown as a diff of the URL, on Change this request', (await sheet.locator('lit-material-diff').evaluate((el) => el.original.includes('/order/7') && el.modified.includes('/orders/7'))) && (await sheet.getByRole('tab', { name: 'Change this request' }).getAttribute('aria-selected')) === 'true');
  check('U14 ...and nothing changed before Apply', (await tab()).u === `${base}/order/7` && !hits.some((x) => x.includes('/orders/7')));
  await sheet.getByRole('button', { name: /Run with checks/ }).click();
  await p.waitForFunction(() => document.querySelector('hitop-app').store.tab.statusCode === 200 && (document.querySelector('hitop-app').store.tab.tests || []).length > 0, null, { timeout: 15000 }).catch(() => {});
  const ran = await p.evaluate(() => { const t = document.querySelector('hitop-app').store.tab; return { u: t.url, status: t.statusCode, tests: (t.tests || []).map((x) => x.pass) }; });
  check('U14 Run with checks applies the fix and sends the request with its tests', ran.u === `${base}/orders/7` && ran.status === 200 && ran.tests.length >= 2 && ran.tests.every(Boolean) && hits.some((x) => x.endsWith('/orders/7')), JSON.stringify(ran));

  // A request from text.
  answer = JSON.stringify({ name: 'Create user', method: 'POST', url: '${baseUrl}/users', headers: [{ name: 'Content-Type', value: 'application/json' }], body: '{"name":"Ada"}' });
  await sheet.getByRole('tab', { name: 'Request from text' }).click();
  await sheet.getByLabel(/curl command/).fill("curl -X POST https://api.example.test/users -d '{\"name\":\"Ada\"}'");
  await sheet.getByRole('button', { name: /Make the request/ }).click();
  await sheet.getByRole('button', { name: 'Open as a request' }).click();
  await settle(p);
  const made = await tab();
  check('a request from text opens as a request', made.m === 'POST' && made.u === '${baseUrl}/users' && made.b.includes('Ada') && sent.at(-1).body.output_config.format.type === 'json_schema', JSON.stringify(made));
  check('...and closes the sheet', !(await sheet.evaluate((el) => el.open)));

  check('no page errors', errors.length === 0, errors.join(' | '));
  api.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
