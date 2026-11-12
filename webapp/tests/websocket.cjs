// WebSocket requests: switch a tab to WebSocket, connect to a local echo
// server, send typed and saved messages (variables filled in), see the
// timestamped log (text and binary), disconnect, save the request in the
// model hitop-core reads, and reopen it. In web mode the browser's
// WebSocket can't send handshake headers and the page says so; in Tauri
// mode (the desktop build) the mocked Rust side sends them.
//   BASE=http://localhost:7879 node tests/websocket.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const crypto = require('crypto');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

// A WebSocket echo server, by hand (RFC 6455, no extensions): echoes text
// and binary frames, answers the text `bin` with bytes 00 ff 10, closes
// with 4000 "bye" on `close`, picks the first subprotocol offered and
// records each handshake's headers.
function echoServer() {
  const handshakes = [];
  const server = http.createServer((req, res) => res.writeHead(426).end('WebSocket only'));
  const frame = (op, payload) => {
    const n = payload.length;
    const head = n < 126 ? Buffer.from([0x80 | op, n]) : Buffer.from([0x80 | op, 126, n >> 8, n & 255]);
    return Buffer.concat([head, payload]);
  };
  server.on('upgrade', (req, socket) => {
    handshakes.push({ url: req.url, headers: req.headers });
    const accept = crypto.createHash('sha1').update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    const offered = (req.headers['sec-websocket-protocol'] || '').split(',').map((x) => x.trim()).filter(Boolean);
    socket.write(['HTTP/1.1 101 Switching Protocols', 'Upgrade: websocket', 'Connection: Upgrade', `Sec-WebSocket-Accept: ${accept}`, ...(offered.length ? [`Sec-WebSocket-Protocol: ${offered[0]}`] : []), '', ''].join('\r\n'));
    let buf = Buffer.alloc(0);
    socket.on('data', (chunk) => {
      buf = Buffer.concat([buf, chunk]);
      while (buf.length >= 2) {
        const op = buf[0] & 15;
        let len = buf[1] & 127;
        let off = 2;
        if (len === 126) (len = buf.readUInt16BE(2)), (off = 4);
        else if (len === 127) (len = Number(buf.readBigUInt64BE(2))), (off = 10);
        if (buf.length < off + 4 + len) return;
        const mask = buf.subarray(off, off + 4);
        const data = Buffer.from(buf.subarray(off + 4, off + 4 + len).map((b, i) => b ^ mask[i % 4]));
        buf = buf.subarray(off + 4 + len);
        if (op === 8) {
          socket.end(frame(8, data.subarray(0, 2)));
          return;
        }
        if (op === 9) socket.write(frame(10, data));
        if (op === 1 && data.toString() === 'close') {
          const body = Buffer.concat([Buffer.from([4000 >> 8, 4000 & 255]), Buffer.from('bye')]);
          socket.end(frame(8, body));
          return;
        }
        if (op === 1 && data.toString() === 'bin') socket.write(frame(2, Buffer.from([0, 255, 16])));
        else if (op === 1 || op === 2) socket.write(frame(op, data));
      }
    });
    socket.on('error', () => {});
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, handshakes })));
}

(async () => {
  const { server, handshakes } = await echoServer();
  const base = `ws://127.0.0.1:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const type = async (text) => p.keyboard.type(text, { delay: 10 });
  const section = (name) => p.getByRole('tablist', { name: 'Request details' }).getByRole('tab', { name: new RegExp(`^${name}`) }).click();
  const log = () => p.locator('.ws-log').innerText().catch(() => '');
  const waitLog = (text) => p.waitForFunction((t) => document.querySelector('.ws-log')?.textContent.includes(t), text, { timeout: 10000 }).then(() => true, () => false);
  const status = () => p.locator('.ws-log-panel .status-chip').innerText();

  await p.goto(h.url, { waitUntil: 'networkidle' });
  // The server and a name as globals, so ${variables} are filled in.
  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await settle(p);
  for (const [k, v] of [['wsBase', base], ['who', 'ada']]) {
    await p.getByRole('button', { name: 'Add global' }).click();
    await type(k);
    await p.keyboard.press('Tab');
    await type(v);
    await settle(p);
  }

  await p.locator('aside .new-request').click(); // the sidebar's +: a new request (FV1)
  await p.getByRole('combobox', { name: 'Method' }).click();
  await p.getByRole('option', { name: 'WebSocket' }).click();
  await settle(p);
  const first = [await p.getByRole('button', { name: 'Connect' }).count(), await p.getByRole('tab', { name: /^Messages/ }).count(), await p.locator('.ws-log-panel lit-material-empty-state').getAttribute('headline')];
  check('WebSocket tab: Connect button, Messages tab, an empty log', first[0] === 1 && first[1] === 1 && first[2] === 'Connect to see messages', JSON.stringify(first));
  await p.getByLabel('URL').fill('${wsBase}/feed?room=${who}');
  await section('Headers');
  await settle(p);
  await p.getByRole('button', { name: 'Add header' }).click();
  await type('X-Client');
  await p.keyboard.press('Tab');
  await type('hitop-${who}');
  await p.getByLabel('Subprotocols').fill('chat.v2, chat.v1');
  await p.getByLabel('Subprotocols').press('Tab');
  await settle(p);
  const note = await p.locator('.editor-panel').innerText();
  if (TAURI) check('[tauri] no browser note: the desktop sends headers', !note.includes("can't set WebSocket handshake headers"), note.slice(0, 200));
  else check("[web] the page says a browser can't send handshake headers", note.includes("can't set WebSocket handshake headers"), note.slice(0, 300));
  await section('Auth');
  await p.getByRole('combobox', { name: 'Type' }).click();
  await p.getByRole('option', { name: /Bearer/ }).click();
  await p.getByLabel('Token').fill('t-${who}');
  await settle(p);

  await p.getByRole('button', { name: 'Connect' }).click();
  const connected = await p.waitForFunction(() => document.querySelector('.ws-log-panel .status-chip')?.textContent.includes('Connected'), null, { timeout: 10000 }).then(() => true, () => false);
  check('connects: status Connected, subprotocol chat.v2 picked', connected && (await p.locator('.ws-log-panel').innerText()).includes('Subprotocol chat.v2'), await p.locator('.ws-log-panel').innerText());
  const hs = handshakes[handshakes.length - 1];
  check('handshake: variables filled in the URL and subprotocols', hs?.url === '/feed?room=ada' && hs.headers['sec-websocket-protocol'] === 'chat.v2, chat.v1', JSON.stringify(hs));
  if (TAURI) {
    check('[tauri] handshake headers and auth reach the server (through Rust)', hs?.headers['x-client'] === 'hitop-ada' && hs.headers.authorization === 'Bearer t-ada', JSON.stringify(hs?.headers));
    check('[tauri] ws_connect got the filled-in request', h.mock.wsConnects.some((c) => c.url === `${base}/feed?room=ada` && c.protocols.join() === 'chat.v2,chat.v1'));
  } else {
    check('[web] no custom header or auth sent, and the log says so', !hs?.headers['x-client'] && !hs?.headers.authorization && (await log()).includes('Not sent: X-Client, Authorization'), await log());
  }

  // Typed messages, variables filled in.
  await section('Messages');
  await p.locator('.ws-draft .cm-content').click();
  await type('hello ${who}');
  await p.locator('.ws-compose').getByRole('button', { name: 'Send' }).click();
  await p.waitForFunction(() => document.querySelectorAll('.ws-event.received').length >= 1, null, { timeout: 10000 }).catch(() => {});
  check('typed message sent and echoed, variables filled in', /Sent[\s\S]*hello ada[\s\S]*Received[\s\S]*hello ada/.test(await log()), await log());
  const times = await p.locator('.ws-log time').allInnerTexts();
  check('every event has a time', times.length >= 3 && times.every((t) => /^\d\d:\d\d:\d\d\.\d{3}$/.test(t)), times.join(' '));

  // Saved messages: save the draft, rename it, add another, send by name.
  await p.getByRole('button', { name: 'Save as message' }).click();
  await settle(p);
  await p.locator('.ws-saved-row').first().getByLabel('Name').fill('greet');
  await p.locator('.ws-compose .cm-content').click();
  await p.keyboard.press('ControlOrMeta+a');
  await type('bin');
  await p.getByRole('button', { name: 'Save as message' }).click();
  await settle(p);
  await p.locator('.ws-saved-row').nth(1).getByLabel('Name').fill('binary');
  await settle(p);
  check('saved messages keep their names', (await p.locator('.ws-saved-row').nth(1).getByLabel('Name').inputValue().catch(() => '')) === 'binary' || (await p.locator('.ws-saved-row').count()) === 2);
  await p.locator('.ws-saved-row').nth(1).getByRole('button', { name: /Send/ }).click();
  check('a binary message is shown as hex and base64', (await waitLog('Binary, 3 bytes')) && (await log()).includes('hex 00 ff 10') && (await log()).includes('base64 AP8Q'), await log());

  if (process.env.SHOT) await p.screenshot({ path: process.env.SHOT });

  // Saved in the model hitop-core reads.
  await p.getByRole('button', { name: 'Save request' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'Feed'));
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await settle(p);
  const saved = () => h.stored.collections().then((cols) => cols.flatMap((c) => c.requests).find((r) => r.name === 'Feed'));
  const r = await saved();
  check(
    'saved: kind websocket, subprotocols and named messages',
    r?.kind === 'websocket' && JSON.stringify(r.websocket) === JSON.stringify({ subprotocols: ['chat.v2', 'chat.v1'], messages: [{ name: 'greet', content: 'hello ${who}' }, { name: 'binary', content: 'bin' }] }) && r.url === '${wsBase}/feed?room=${who}',
    JSON.stringify(r),
  );
  const listed = await p.locator('aside').innerText();
  check('the sidebar lists it as WS', /WS\s*Feed/.test(listed), listed.slice(0, 300));

  // The server closes: code and reason in the log, status back to not connected.
  await p.locator('.ws-compose .cm-content').click();
  await p.keyboard.press('ControlOrMeta+a');
  await type('close');
  await p.locator('.ws-compose').getByRole('button', { name: 'Send' }).click();
  check('a close from the server shows its code and reason', (await waitLog('Closed 4000 bye')) && (await status()).includes('Not connected'), await log());

  // Connect again, then disconnect from the page.
  await p.getByRole('button', { name: 'Connect' }).click();
  await p.waitForFunction(() => document.querySelector('.ws-log-panel .status-chip')?.textContent.includes('Connected'), null, { timeout: 10000 }).catch(() => {});
  await p.getByRole('button', { name: 'Disconnect' }).click();
  check('Disconnect closes with 1000', (await waitLog('Closed 1000')) && (await status()).includes('Not connected'), await log());
  await p.getByRole('button', { name: 'Clear' }).click();
  check('Clear empties the log', (await p.locator('.ws-log').count()) === 0);

  // After a reload: still a WebSocket tab with its messages; the log isn't kept.
  await p.reload({ waitUntil: 'networkidle' });
  await settle(p);
  const tab = (await h.stored.tabs()).tab;
  check('after a reload: WebSocket tab, saved messages back, no log kept', tab?.kind === 'websocket' && (await p.locator('.ws-saved-row').count()) === 2 && !JSON.stringify(tab).includes('Received') && (await p.locator('.ws-log').count()) === 0, JSON.stringify(tab).slice(0, 300));

  // A server that isn't there: the error is shown.
  await p.getByLabel('URL').fill('ws://127.0.0.1:9/');
  await p.getByRole('button', { name: 'Connect' }).click();
  check('a refused connection is reported', await waitLog("Couldn't connect"), await log());
  // Not a WebSocket URL: refused before connecting.
  await p.getByLabel('URL').fill('ftp://x');
  await p.getByRole('button', { name: 'Connect' }).click();
  await settle(p);
  check('a URL that is not ws:// is refused', (await p.locator('body').innerText()).includes('not a WebSocket URL'));

  // Back to HTTP: a GET tab again.
  await p.getByRole('combobox', { name: 'Method' }).click();
  await p.getByRole('option', { name: 'GET', exact: true }).click();
  await settle(p);
  check('switching back to GET shows Send again', (await p.getByRole('button', { name: 'Send' }).count()) >= 1 && (await p.locator('.ws-log-panel').count()) === 0);

  check('no page errors', errors.length === 0, errors.join('; '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
