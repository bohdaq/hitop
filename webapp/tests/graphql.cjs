// GraphQL bodies: the query and variables editors, the schema fetched by
// introspection (through the app's transport), completion from it in the
// query editor, the JSON POST the server gets (variables filled in), and the
// saved request. A local GraphQL server (graphql-js) only.
//   BASE=http://localhost:7879 node tests/graphql.cjs   (HITOP_MODE=tauri: the desktop build)
const http = require('http');
const { launch, checker, TAURI } = require('./harness.cjs');
const { results, check } = checker();
const settle = (p) => p.waitForTimeout(400);

// A GraphQL endpoint at /graphql that records what it got; anything else is 404.
async function graphqlServer() {
  const { buildSchema, graphql } = await import('graphql');
  const schema = buildSchema('type Query { user(id: ID!): User, users: [User] } type User { id: ID! name: String email: String }');
  const people = { 42: { id: '42', name: 'Ada', email: 'ada@example.com' } };
  const rootValue = { user: ({ id }) => people[id] ?? null, users: () => Object.values(people) };
  const got = [];
  const server = http.createServer(async (req, res) => {
    const cors = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': '*' };
    if (req.method === 'OPTIONS') return res.writeHead(204, cors).end();
    if (req.url !== '/graphql' || req.method !== 'POST') return res.writeHead(404, cors).end('not found');
    let text = '';
    for await (const chunk of req) text += chunk;
    let body;
    try {
      body = JSON.parse(text);
    } catch {
      return res.writeHead(400, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify({ errors: [{ message: 'body is not JSON' }] }));
    }
    got.push({ body, contentType: req.headers['content-type'] });
    const result = await graphql({ schema, rootValue, source: body.query, variableValues: body.variables, operationName: body.operationName });
    res.writeHead(200, { ...cors, 'Content-Type': 'application/json' }).end(JSON.stringify(result));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { server, got };
}

(async () => {
  const { server, got } = await graphqlServer();
  const base = `http://127.0.0.1:${server.address().port}`;
  const h = await launch({ results });
  const p = h.page;
  const errors = [];
  p.on('pageerror', (e) => errors.push(e.message));
  const type = async (text) => p.keyboard.type(text, { delay: 15 });
  const section = (name) => p.getByRole('tablist', { name: 'Request details' }).getByRole('tab', { name: new RegExp(`^${name}`) }).click();
  const send = async () => {
    await p.getByRole('button', { name: 'Send' }).click();
    await p.waitForFunction(() => document.querySelector('.response-meta, .error-text') && !document.querySelector('[aria-busy="true"]'), null, { timeout: 30000 });
    await settle(p);
  };
  // The completion list once it offers `want` (or what it offered by then).
  const completions = async (want) => {
    await p.waitForFunction((w) => document.querySelector('.cm-tooltip-autocomplete')?.textContent.includes(w), want, { timeout: 5000 }).catch(() => {});
    return p.locator('.cm-tooltip-autocomplete li').allInnerTexts();
  };
  const tab = async () => (await h.stored.tabs()).tab;

  await p.goto(h.url, { waitUntil: 'networkidle' });
  // The endpoint and the id as globals, so ${variables} are filled in.
  await p.locator('aside').getByRole('button', { name: 'Environments' }).click();
  await settle(p);
  for (const [k, v] of [['base', base], ['uid', '42']]) {
    await p.getByRole('button', { name: 'Add global' }).click();
    await type(k);
    await p.keyboard.press('Tab');
    await type(v);
    await settle(p);
  }

  await p.locator('aside .new-request').click(); // the sidebar's +: a new request (FV1)
  await p.getByRole('combobox', { name: 'Method' }).click();
  await p.getByRole('option', { name: 'POST' }).click();
  await p.getByLabel('URL').fill('${base}/graphql');
  await section('Body');
  await p.getByRole('combobox', { name: 'Body type' }).click();
  await p.getByRole('option', { name: 'GraphQL' }).click();
  await settle(p);
  check('GraphQL body: query and variables editors', (await p.locator('.graphql-query .cm-editor').count()) === 1 && (await p.locator('.graphql-variables .cm-editor').count()) === 1);

  // Introspection through the app's transport.
  await p.getByRole('button', { name: 'Fetch schema' }).click();
  await p.waitForFunction(() => /Schema: \d+ types/.test(document.querySelector('.graphql-tools')?.textContent || ''), null, { timeout: 15000 });
  const intro = got[got.length - 1];
  check('introspection went to the endpoint as GraphQL', intro?.body?.operationName === 'IntrospectionQuery' && intro.body.query.includes('__schema') && intro.contentType === 'application/json', JSON.stringify(intro?.body?.operationName));

  // Completion from the schema: root fields, then the fields of User.
  await p.locator('.graphql-query .cm-content').click();
  await type('query U($id: ID!) { us');
  const roots = await completions('users');
  check('completion offers the root fields', roots.some((x) => x.startsWith('user')) && roots.some((x) => x.startsWith('users')), roots.join(' | '));
  await p.keyboard.press('Escape');
  await type('er(id: $id) { em');
  const fields = await completions('email');
  check('completion offers the fields of User', fields.some((x) => x.startsWith('email')), fields.join(' | '));
  await p.keyboard.press('Escape');
  await type('ail name } }');
  await p.keyboard.press('Escape');
  await p.locator('.graphql-variables .cm-content').click();
  await type('{"id": "${uid}"}');
  await p.getByLabel('Operation name').fill('U');
  await settle(p);
  const query = 'query U($id: ID!) { user(id: $id) { email name } }';
  check('the tab keeps query, variables and operation name', (await tab()).requestBody === query && (await tab()).graphqlVariables === '{"id": "${uid}"}' && (await tab()).graphqlOperationName === 'U', JSON.stringify(await tab()).slice(0, 300));

  await send();
  const last = got[got.length - 1];
  check('sent as a JSON POST of query, variables (filled in) and operationName', JSON.stringify(last?.body) === JSON.stringify({ query, variables: { id: '42' }, operationName: 'U' }) && last.contentType === 'application/json', JSON.stringify(last));
  const shown = await p.locator('.response-body').innerText();
  check('the response shows the data', shown.includes('ada@example.com'), shown.slice(0, 200));
  if (TAURI) check('[tauri] sent through Rust (send_request)', h.sent.some((s) => s.url === `${base}/graphql` && JSON.parse(s.body).operationName === 'U'));

  // The Code panel sends the same body (hitop-core's snippet generator).
  await p.getByRole('button', { name: 'More options' }).click();
  await p.locator('#line-menu').getByText('Code', { exact: true }).click();
  await p.locator('#dialog .snippet').waitFor({ timeout: 10000 });
  const curl = await p.locator('#dialog .snippet').evaluate((el) => el.textContent);
  check('Code panel: curl posts the GraphQL JSON body', curl.includes(`--data-raw '${JSON.stringify({ query, variables: { id: '42' }, operationName: 'U' }).replace('{"id":"42"}', '{"id": "42"}')}'`) && curl.includes("-H 'Content-Type: application/json'"), curl);
  await p.locator('#dialog').getByRole('button', { name: 'Close' }).click();
  await settle(p);

  // Saved as a GraphQL request, in the model hitop-core reads.
  await p.getByRole('button', { name: 'Save request' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog #name').evaluate((el) => (el.value = 'Who'));
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await settle(p);
  const saved = () => h.stored.collections().then((cols) => cols.flatMap((c) => c.requests).find((r) => r.name === 'Who'));
  let r = await saved();
  check('saved with bodyType graphql, the query as body and graphql {variables, operationName}', r?.bodyType === 'graphql' && r.body === query && r.graphql?.variables === '{"id": "${uid}"}' && r.graphql.operationName === 'U' && (r.bodyMode ?? 'raw') === 'raw', JSON.stringify(r));

  // After a reload the tab is still a GraphQL body.
  await p.reload({ waitUntil: 'networkidle' });
  await section('Body');
  await settle(p);
  check('after a reload: GraphQL body, variables kept', (await p.locator('.graphql-variables .cm-content').innerText()).includes('${uid}'));
  // Schemas live in memory: after a reload it is fetched again.
  check('no schema after a reload until fetched again', (await p.locator('.graphql-tools').innerText()).includes('Fetch schema'));

  // A switch to raw keeps the variables but the request is no longer GraphQL.
  await p.getByRole('combobox', { name: 'Body type' }).click();
  await p.getByRole('option', { name: /Raw/ }).click();
  await p.getByRole('button', { name: 'Save request' }).click();
  await p.waitForTimeout(300);
  await p.locator('#dialog').getByRole('button', { name: 'Save' }).click();
  await settle(p);
  r = await saved();
  check('raw again: no bodyType, variables kept (not sent)', r && !r.bodyType && r.graphql?.variables === '{"id": "${uid}"}', JSON.stringify(r));
  await p.getByRole('combobox', { name: 'Body type' }).click();
  await p.getByRole('option', { name: 'GraphQL' }).click();
  await settle(p);

  // An endpoint that isn't GraphQL: the error is shown, nothing breaks.
  await p.getByLabel('URL').fill('${base}/nope');
  // Leaving the URL field shows its hint and moves the page: do it before clicking.
  await p.keyboard.press('Tab');
  await settle(p);
  await p.getByRole('button', { name: /Fetch schema/ }).click();
  await p.waitForFunction(() => /Couldn't fetch the schema/.test(document.querySelector('.graphql-tools')?.textContent || ''), null, { timeout: 15000 });
  check('a failed introspection says why', (await p.locator('.graphql-tools').innerText()).includes('404'), await p.locator('.graphql-tools').innerText());

  // Variables that aren't JSON: not sent, the reason shown.
  await p.getByLabel('URL').fill('${base}/graphql');
  await p.locator('.graphql-variables .cm-content').click();
  await p.keyboard.press('ControlOrMeta+a');
  await type('{id: 1}');
  const before = got.length;
  await p.getByRole('button', { name: 'Send' }).click();
  await settle(p);
  check('invalid variables are not sent, and the app says why', got.length === before && (await p.locator('body').innerText()).includes('not valid JSON'));

  if (process.env.SHOT) await p.screenshot({ path: process.env.SHOT });
  check('no page errors', errors.length === 0, errors.join(' || '));
  server.close();
  await h.finish();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
