// Parameter toggles, form bodies (multipart with a file, URL-encoded), the
// cookie jar and the code editor, against a running hitop-server. Uses
// httpbin.org, which echoes what it receives.
//   BASE=http://localhost:7879 node tests/extras.cjs
const { launch, checker } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

(async () => {
  const h = await launch({ viewport: { width: 1280, height: 900 }, results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const sent = h.sent;
  const last = () => sent[sent.length - 1];
  const tab = async () => (await h.stored.tabs()).tab;
  const section = (name) => p.getByRole('tab', { name: new RegExp(`^${name}`) }).click();
  const send = async () => {
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => document.querySelector('.response-meta, .error-text') && !document.querySelector('[aria-busy="true"]'), null, { timeout: 30000 });
    await settle(p);
  };
  const responseJson = async () => JSON.parse(await p.locator('.response-body').evaluate((el) => el.textContent.replace(/^JSON\s*(Copy)?/, '')));

  await p.goto(h.url, { waitUntil: 'networkidle' });
  // A new request to work on (FV1: the first visit opens the sample's List posts).
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await settle(p);

  // Parameter toggles
  await p.getByLabel('URL').fill('https://httpbin.org/get?keep=1&drop=2');
  await section('Params');
  await p.getByRole('checkbox', { name: 'Send drop' }).click();
  await settle(p);
  let t = await tab();
  check('unchecked parameter leaves the URL', t.url === 'https://httpbin.org/get?keep=1' && t.disabledParams?.[0]?.key === 'drop', `${t.url} ${JSON.stringify(t.disabledParams)}`);
  await p.getByRole('checkbox', { name: 'Send drop' }).click();
  await settle(p);
  check('checking it again puts it back', (await tab()).url === 'https://httpbin.org/get?keep=1&drop=2', (await tab()).url);

  // Multipart with a text field and a file
  await p.getByRole('combobox', { name: 'Method' }).click();
  await p.getByRole('option', { name: 'POST' }).click();
  await p.getByLabel('URL').fill('https://httpbin.org/post');
  await section('Body');
  await p.getByRole('combobox', { name: 'Body type' }).click();
  await p.getByRole('option', { name: /multipart/ }).click();
  await p.getByRole('button', { name: 'Add field' }).click();
  await p.keyboard.type('title', { delay: 10 });
  await p.getByLabel('Value', { exact: true }).last().fill('Report');
  await p.getByRole('button', { name: 'Add field' }).click();
  await p.keyboard.type('upload', { delay: 10 });
  await p.getByRole('combobox', { name: 'Type' }).last().click();
  await p.getByRole('option', { name: 'File' }).click();
  await p.locator('.form-row input[type=file]').setInputFiles({ name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('hello from hitop') });
  await settle(p);
  await send();
  check('multipart sent as base64 through the proxy', last().bodyEncoding === 'base64' && /multipart\/form-data; boundary=/.test(last().headers.find((h) => h[0] === 'Content-Type')?.[1] || ''));
  let echo = await responseJson();
  check('server got the field and the file', echo.form?.title === 'Report' && echo.files?.upload === 'hello from hitop', JSON.stringify({ form: echo.form, files: echo.files }));
  t = await tab();
  check('file data is not stored, only its name', JSON.stringify(t.formFields).includes('notes.txt') && !JSON.stringify(t.formFields).includes('hello from hitop'), JSON.stringify(t.formFields));

  // URL-encoded form
  await p.getByRole('combobox', { name: 'Body type' }).click();
  await p.getByRole('option', { name: 'URL encoded form' }).click();
  await send();
  echo = await responseJson();
  check('URL-encoded form', echo.form?.title === 'Report' && /x-www-form-urlencoded/.test(echo.headers?.['Content-Type'] || ''), JSON.stringify(echo.form));

  // Cookie jar: a response sets a cookie, the next request sends it
  await p.getByRole('combobox', { name: 'Method' }).click();
  await p.getByRole('option', { name: 'GET', exact: true }).click();
  await p.getByLabel('URL').fill('https://httpbin.org/response-headers?Set-Cookie=session%3Dabc123%3B%20Path%3D%2F');
  await send();
  await p.getByLabel('URL').fill('https://httpbin.org/cookies');
  await send();
  echo = await responseJson();
  check('cookie kept and sent back', echo.cookies?.session === 'abc123', JSON.stringify(echo));
  await p.locator('aside').getByRole('button', { name: 'Cookies', exact: true }).click();
  await settle(p);
  const cookiesPage = await p.locator('main').innerText();
  check('cookies page lists it', cookiesPage.includes('httpbin.org') && cookiesPage.includes('session'));
  if (h.mock) {
    // The desktop: hitop-core's jar in Rust adds the header and keeps the cookie.
    const out = last();
    check('[tauri] Rust jar sends the cookie, not the page', out.cookies === true && !out.headers.some(([k]) => k.toLowerCase() === 'cookie'), JSON.stringify(out.headers));
    check('[tauri] jar kept in cookies/global.json', h.mock.loadJar().some((c) => c.name === 'session' && c.domain === 'httpbin.org'), JSON.stringify(h.mock.loadJar()));
  }
  await p.getByRole('switch', { name: 'Keep and send cookies' }).click();
  await settle(p);
  await p.keyboard.press('Control+KeyL'); // back to the request
  await settle(p);
  await send();
  echo = await responseJson();
  check('turning the jar off stops sending', !echo.cookies?.session, JSON.stringify(echo.cookies));

  // Code editor: highlighting and syntax errors
  await section('Script before');
  await p.locator('.cm-content').click();
  await p.keyboard.type("const id = getContext('id');\nsetUrl(", { delay: 5 });
  await p.waitForTimeout(1200);
  const marks = await p.locator('.cm-lintRange-error, .cm-lint-marker-error').count();
  check('syntax error marked', marks > 0, `${marks} marks`);
  check('script highlighted', (await p.locator('.cm-line span').count()) > 2);
  check('script text stored', (await tab()).preRequestScript === "const id = getContext('id');\nsetUrl(", JSON.stringify((await tab()).preRequestScript));

  check('no page errors', errors.length === 0, errors.join(' || '));
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error('CRASH', e.message.split('\n')[0]);
  process.exit(1);
});
