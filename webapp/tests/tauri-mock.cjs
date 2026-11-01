// The desktop app's Rust side, mocked in Node for the web app's Tauri build
// (HITOP_MODE=tauri). Each command does what desktop/src-tauri/src/commands.rs
// does:
//   storage   files in a temp config dir (collections.json, history.json,
//             contexts.json, environments.json), read and written through
//             hitop-core's model in WASM (hitop-wasm `normalize`), so what
//             Rust would refuse fails here too
//   requests  Node's fetch: no CORS and no proxy, like reqwest
//   scripts   the same JavaScript preamble scripting.rs runs in boa, in node:vm
//   folders   real directories, read and written by hitop-core in WASM
//   secrets   an in-memory keychain
//   ping      the usage ping setting (commands.rs telemetry_*): answered
//             already unless a test sets `telemetry.asked = false`;
//             `telemetry = null` is a build without it (Mac App Store)
//   websocket ws_connect/ws_send/ws_close through Node's WebSocket (headers
//             sent, as Rust's client does), events on the page's channel
//   grpc      grpc_list/grpc_describe/grpc_call/grpc_cancel against a canned
//             server (`grpcServer`: the hitop.test.Greeter service of
//             hitop-core's tests/fixtures/grpc, reflection on or off), as
//             commands.rs answers: errors before calling, events on the channel
//   cookies   hitop-core's jar (cookies.rs, same rules as features.js) in
//             <config>/cookies/global.json
//   git       the `git` command runs hitop-git itself (cli/crates/hitop-git,
//             example git-json: the same json::call desktop/src-tauri/src/git.rs
//             calls) on real repositories; tokens in the in-memory keychain
//             under git/<host>/token, as git.rs keeps them. Build it first:
//             cargo build --manifest-path cli/Cargo.toml -p hitop-git --example git-json --no-default-features
//             (or point HITOP_GIT_JSON at the binary)
const fs = require('fs');
const os = require('os');
const path = require('path');
const vm = require('vm');
const { execFile } = require('child_process');

const GIT_JSON = process.env.HITOP_GIT_JSON || path.join(__dirname, '../../cli/target/debug/examples/git-json');

const WASM = path.join(__dirname, '../src/wasm/pkg/hitop_wasm_bg.wasm');

async function loadFormat() {
  const { formatFromBytes } = await import(path.join(__dirname, '../src/wasm/format.js'));
  return formatFromBytes(fs.readFileSync(WASM));
}

// scripting.rs: the pm.* shim, loaded only for scripts that may use it,
// with the script in a function that receives pm and the legacy names.
const PM_SHIM = fs.readFileSync(path.join(__dirname, '../../shared/pm-shim.js'), 'utf8');
const usesPostmanApi = (script) => /\b(pm|postman|tests|responseBody|responseCode|responseTime|environment|globals)\b/.test(script);
const SET_VAR = 'function (k, v) { __varUpdates[k] = v; if (v === null) delete __vars[k]; else __vars[k] = v; }';
const SET_GLOBAL = 'function (k, v) { __globalUpdates[k] = v; }';
const withShim = (api, script) =>
  `${PM_SHIM}\nconst __p = __hitopPm(${api});\n(function (pm, tests, postman, environment, globals, responseBody, responseCode, responseTime) {\n${script}\n}).call(undefined, __p.pm, __p.tests, __p.postman, __p.environment, __p.globals, __p.responseBody, __p.responseCode, __p.responseTime);\n__p.finish();\n`;

// scripting.rs: run_pre_request_script's preamble and postamble.
function preScript(a) {
  const src = `
const __vars = ${JSON.stringify(a.variables || {})};
const __ctx  = ${JSON.stringify(a.context || {})};
const __ctxUpdates = {};
let __url    = ${JSON.stringify(a.url)};
let __body   = ${JSON.stringify(a.body)};
let __headers = ${JSON.stringify(a.headers)};
const __varUpdates = {};
const __globals = ${JSON.stringify(a.globals || {})};
const __globalUpdates = {};
const __method = ${JSON.stringify(a.method)};
function __removeHeader(name) { const n = String(name).toLowerCase(); __headers = __headers.filter(h => String(h[0]).toLowerCase() !== n); }
function getVariable(k) { return __vars[k]; }
function getContext(k)  { return __ctxUpdates[k] !== undefined ? __ctxUpdates[k] : __ctx[k]; }
function setContext(k, v) { __ctxUpdates[k] = v; }
function setUrl(u)  { __url = u; }
function setBody(b) { __body = b; }
function setHeader(name, value) {
    let found = false;
    for (let h of __headers) { if (h[0] === name) { h[1] = value; found = true; break; } }
    if (!found) __headers.push([name, value]);
}
${usesPostmanApi(a.script)
    ? withShim(`{ vars: __vars, setVar: ${SET_VAR}, globals: __globals, setGlobal: ${SET_GLOBAL}, assert: { ok: function () {} }, request: { get url() { return __url; }, method: __method, get headers() { return __headers; }, get body() { return __body; } }, setUrl: setUrl, setHeader: setHeader, removeHeader: __removeHeader, setBody: setBody, response: null }`, a.script)
    : a.script}
;JSON.stringify({ url: __url, body: __body, headers: __headers, ctx: __ctxUpdates, vars: __varUpdates, globals: __globalUpdates })`;
  let out;
  try {
    out = JSON.parse(vm.runInNewContext(src, {}, { timeout: 2000 }));
  } catch (e) {
    throw `pre-request script error: ${e.message}`;
  }
  return { url: typeof out.url === 'string' ? out.url : a.url, headers: out.headers, body: typeof out.body === 'string' ? out.body : a.body, context_updates: out.ctx, variables: out.vars, globals: out.globals };
}

// scripting.rs: run_post_request_script.
function postScript(a) {
  let parsed = null;
  try {
    parsed = JSON.parse(a.response_body);
  } catch {
    // Value::Null
  }
  const src = `
const response        = ${JSON.stringify(parsed)};
const responseText    = ${JSON.stringify(a.response_body)};
const responseHeaders = ${JSON.stringify(a.response_headers || {})};
const statusCode      = ${a.status_code};
const __vars = ${JSON.stringify(a.variables || {})};
const __ctx  = ${JSON.stringify(a.context || {})};
const __ctxUpdates = {};
const __varUpdates = {};
const __globals = ${JSON.stringify(a.globals ?? null)};
const __globalUpdates = {};
function getVariable(k) { return __vars[k]; }
function getContext(k)  { return __ctxUpdates[k] !== undefined ? __ctxUpdates[k] : __ctx[k]; }
function setContext(k, v) { __ctxUpdates[k] = v; }
function getResponseValue(path) {
    const keys = path.split('.');
    let val = response;
    for (const k of keys) { if (val && typeof val === 'object') val = val[k]; else return undefined; }
    return val;
}
function getResponseHeader(name) { return responseHeaders[name.toLowerCase()]; }
const __tests = [];
function __record(name, pass, detail) { __tests.push({ name: String(name), pass: !!pass, detail: pass ? "" : String(detail) }); }
const assert = {
    status(e) { __record("status is " + e, statusCode === Number(e), "got " + statusCode); },
    bodyContains(f) { __record("body contains \\"" + f + "\\"", String(responseText).includes(String(f)), "not found in the response body"); },
    header(n, e) {
        const a = responseHeaders[String(n).toLowerCase()];
        if (e === undefined) __record("has header " + n, a !== undefined, "header missing");
        else __record("header " + n + " is \\"" + e + "\\"", a !== undefined && a.includes(String(e)), a === undefined ? "header missing" : "got \\"" + a + "\\"");
    },
    responseTime(ms) { __record("responds within " + ms + " ms", ${a.duration_ms || 0} <= Number(ms), "took ${a.duration_ms || 0} ms"); },
    equal(a, e, name) { __record(name === undefined ? JSON.stringify(a) + " equals " + JSON.stringify(e) : name, JSON.stringify(a) === JSON.stringify(e), "got " + JSON.stringify(a)); },
    ok(c, name, detail) { __record(name === undefined ? "condition is true" : name, c, detail === undefined || detail === "" ? "was false" : detail); },
};
${usesPostmanApi(a.script)
    ? withShim(`{ vars: __vars, setVar: ${SET_VAR}, globals: __globals || undefined, setGlobal: ${SET_GLOBAL}, assert: assert, request: ${JSON.stringify(a.request || { url: '', method: '', headers: [], body: '' })}, response: { code: statusCode, text: responseText, headers: responseHeaders, time: ${a.duration_ms || 0} } }`, a.script)
    : a.script}
;JSON.stringify({ ctx: __ctxUpdates, tests: __tests, vars: __varUpdates, globals: __globalUpdates })`;
  let out;
  try {
    out = JSON.parse(vm.runInNewContext(src, {}, { timeout: 2000 }));
  } catch (e) {
    throw `post-request script error: ${e.message}`;
  }
  return { context_updates: out.ctx, tests: out.tests, variables: out.vars, globals: out.globals };
}

function listTree(dir, base = dir, out = {}) {
  if (!fs.existsSync(dir)) return out;
  for (const name of fs.readdirSync(dir).sort()) {
    if (name.startsWith('.')) continue;
    const p = path.join(dir, name);
    const st = fs.lstatSync(p);
    if (st.isDirectory()) listTree(p, base, out);
    else if (st.isFile()) out[path.relative(base, p).split(path.sep).join('/')] = fs.readFileSync(p, 'utf8');
  }
  return out;
}

class TauriMock {
  // `seed`: a directory of storage files to start from (copied).
  constructor({ seed } = {}) {
    this.dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hitop-tauri-'));
    this.config = path.join(this.dir, 'config');
    fs.mkdirSync(this.config);
    if (seed) for (const f of fs.readdirSync(seed)) fs.copyFileSync(path.join(seed, f), path.join(this.config, f));
    this.sent = [];
    this.calls = [];
    this.keychain = new Map();
    // secret_refs_resolve: what hitop-core's secret managers would answer, by inner text.
    this.secretManager = new Map();
    this.refCalls = [];
    this.writes = [];
    this.nextFolder = null;
    this.lastCall = 0;
    this.sockets = new Map();
    this.nextSocket = 0;
    this.wsConnects = [];
    // The gRPC server the mock pretends to reach, and every call made.
    this.grpcServer = { address: '127.0.0.1:50051', reflection: true };
    this.grpcCalls = [];
    this.grpcCancelled = [];
    this.telemetry = { enabled: false, asked: true, id: null, lastPing: null };
    // Secret manager settings (commands.rs secret_manager_settings*): the
    // variables the app's environment sets (names), as Rust reports them.
    this.smShell = [];
  }

  // hitop-core secret_managers::settings_view: settings from
  // secret-managers.json, which secrets the keychain holds, never values.
  smView() {
    const names = ['VAULT_TOKEN', 'AZURE_CLIENT_SECRET', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN'];
    return {
      settings: JSON.parse(this.read('secret-managers.json', '{}')),
      secrets: Object.fromEntries(names.map((n) => [n, Boolean(this.keychain.get(`secret-managers/${n}`))])),
      shell: [...this.smShell],
      path: this.file('secret-managers.json'),
      keychainError: null,
    };
  }

  // save_settings: secrets to the keychain (text stores, null removes),
  // the rest trimmed into the file (no file when nothing is set).
  smSave(settings, secrets) {
    const names = ['VAULT_TOKEN', 'AZURE_CLIENT_SECRET', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN'];
    for (const [name, value] of Object.entries(secrets || {})) {
      if (!names.includes(name)) throw `${name} is not a secret manager setting (one of ${names.join(', ')})`;
      if (value && value.trim()) this.keychain.set(`secret-managers/${name}`, value.trim());
      else this.keychain.delete(`secret-managers/${name}`);
    }
    const clean = Object.fromEntries(Object.entries(settings || {}).filter(([, v]) => v !== null && v !== undefined && String(v).trim() !== '').map(([k, v]) => [k, typeof v === 'string' ? v.trim() : v]));
    if (Object.keys(clean).length) this.write('secret-managers.json', clean);
    else fs.rmSync(this.file('secret-managers.json'), { force: true });
    return this.smView();
  }

  telemetryStatus() {
    if (!this.telemetry) throw 'this build has no usage ping';
    const t = this.telemetry;
    return { enabled: t.enabled, asked: t.asked, endpoint: 'https://hitop.rws8.tech/api/ping', ping: { id: t.id ?? '<a random id, made when you turn it on>', surface: 'desktop', version: '0.1.0', os: 'linux' }, lastPing: t.lastPing };
  }

  async ready() {
    this.format = await loadFormat();
    this.jar = await import(path.join(__dirname, '../src/features.js'));
    return this;
  }

  get jarFile() {
    return path.join(this.config, 'cookies', 'global.json');
  }

  loadJar() {
    return fs.existsSync(this.jarFile) ? JSON.parse(fs.readFileSync(this.jarFile, 'utf8')) : [];
  }

  // cookies.rs Jar::save: an empty jar removes the file.
  saveJar(cookies) {
    if (!cookies.length) return fs.rmSync(this.jarFile, { force: true });
    fs.mkdirSync(path.dirname(this.jarFile), { recursive: true });
    fs.writeFileSync(this.jarFile, JSON.stringify(cookies, null, 2));
  }

  file(name) {
    return path.join(this.config, name);
  }

  read(name, fallback) {
    const p = this.file(name);
    return fs.existsSync(p) ? fs.readFileSync(p, 'utf8') : fallback;
  }

  // What the desktop has stored, parsed.
  stored(name) {
    return JSON.parse(this.read(`${name}.json`, name === 'collections' || name === 'history' ? '[]' : '{}'));
  }

  write(name, value) {
    fs.writeFileSync(this.file(name), JSON.stringify(value, null, 2));
  }

  async invoke(cmd, args = {}) {
    this.calls.push(cmd);
    this.lastCall = Date.now();
    const f = this.format;
    switch (cmd) {
      case 'load_collections':
        return f.normalize('collections', this.read('collections.json', '[]'));
      case 'save_collections':
        return this.write('collections.json', f.normalize('collections', args.collections));
      case 'load_history':
        try {
          return f.normalize('history', this.read('history.json', '[]'));
        } catch {
          return []; // storage.rs: unwrap_or_default
        }
      case 'save_history':
        return this.write('history.json', f.normalize('history', args.history).slice(-50));
      case 'load_contexts':
        return JSON.parse(this.read('contexts.json', '{}'));
      case 'save_contexts':
        return this.write('contexts.json', args.contexts);
      case 'load_globals':
        return f.normalize('globals', this.read('globals.json', '{"id":"globals","name":"globals","variables":{}}'));
      case 'save_globals':
        return this.write('globals.json', f.normalize('globals', args.globals));
      case 'load_environments':
        return f.normalize('environments', this.read('environments.json', '{"active":null,"environments":[]}'));
      case 'save_environments':
        return this.write('environments.json', f.normalize('environments', args.environments));
      case 'send_request':
        return this.send(args.args);
      case 'telemetry_status':
        return this.telemetryStatus();
      case 'telemetry_set':
        if (!this.telemetry) throw 'this build has no usage ping';
        Object.assign(this.telemetry, { asked: true, enabled: args.enabled, id: args.enabled ? this.telemetry.id ?? '0123456789abcdef0123456789abcdef' : null, lastPing: null });
        return this.telemetryStatus();
      case 'cookies_load':
        return this.loadJar();
      case 'cookies_save':
        return this.saveJar(args.cookies);
      case 'run_pre_script':
        return preScript(args.args);
      case 'run_post_script':
        return postScript(args.args);
      case 'read_folder': {
        if (!fs.existsSync(path.join(args.path, 'hitop.yaml'))) throw `${args.path} is not a collection folder (no hitop.yaml; create one with \`hitop init\`)`;
        return f.readFiles(listTree(args.path), path.basename(args.path));
      }
      case 'write_folder': {
        const files = f.toFiles({ collection: args.collection, environments: args.environments, ...(args.globals ? { globals: args.globals } : {}) });
        const plan = f.planSync(Object.keys(listTree(args.path)), files);
        for (const [rel, text] of Object.entries(plan.write)) {
          const p = path.join(args.path, rel);
          fs.mkdirSync(path.dirname(p), { recursive: true });
          if (!fs.existsSync(p) || fs.readFileSync(p, 'utf8') !== text) fs.writeFileSync(p, text);
        }
        for (const rel of plan.remove) fs.rmSync(path.join(args.path, rel));
        return null;
      }
      case 'secret_refs_resolve': {
        this.refCalls.push([...args.refs]);
        const missing = args.refs.filter((r) => !this.secretManager.has(r));
        if (missing.length) throw missing.map((r) => `\${${r}} (${r.split(':')[1]}): not found`).join('\n');
        return Object.fromEntries(args.refs.map((r) => [r, this.secretManager.get(r)]));
      }
      case 'secret_manager_settings':
        return this.smView();
      case 'secret_manager_settings_set':
        return this.smSave(args.settings, args.secrets);
      case 'secret_get':
        return this.keychain.get(args.account) ?? null;
      case 'secret_set':
        this.keychain.set(args.account, args.value);
        return null;
      case 'secret_delete':
        return this.keychain.delete(args.account);
      case 'folder_forget':
        // Mac App Store bookmarks: nothing to do outside the sandbox.
        return null;
      case 'write_file':
        fs.writeFileSync(args.path, Buffer.from(args.data, 'base64'));
        this.writes.push(args.path);
        return null;
      case 'plugin:dialog|open': {
        const folder = this.nextFolder;
        this.nextFolder = null;
        return folder;
      }
      case 'plugin:dialog|save':
        return path.join(this.dir, path.basename(args.options?.defaultPath || 'export'));
      case 'ws_connect':
        return this.wsConnect(args.args, args.onEvent);
      case 'ws_send': {
        const ws = this.sockets.get(args.id);
        if (!ws) throw 'the connection is closed';
        return ws.send(args.binary ? Buffer.from(args.data, 'base64') : args.data);
      }
      case 'ws_close':
        this.sockets.get(args.id)?.close(1000);
        return null;
      case 'grpc_list':
        return { ...this.grpcOpen(args.args), services: GRPC_SERVICES };
      case 'grpc_describe':
        return this.grpcDescribe(args.args, args.symbol);
      case 'grpc_call':
        return this.grpcCall(args.args, args.onEvent);
      case 'grpc_cancel':
        this.grpcCancelled.push(args.id);
        return null;
      case 'git':
        return this.git(args.cmd, args.args);
      case 'git_token_status': {
        const host = await this.git('remoteHost', { path: args.path }).catch(() => null);
        if (!host) return { host: null, saved: false, username: null };
        return { host, saved: this.keychain.has(`git/${host}/token`), username: this.keychain.get(`git/${host}/username`) ?? null };
      }
      case 'git_token_set': {
        const host = await this.git('remoteHost', { path: args.path });
        if (!host) throw 'this remote is a local path: it needs no token';
        if (!args.token?.trim()) throw 'paste a token';
        this.keychain.set(`git/${host}/token`, args.token.trim());
        if (args.username?.trim()) this.keychain.set(`git/${host}/username`, args.username.trim());
        else this.keychain.delete(`git/${host}/username`);
        return null;
      }
      case 'git_token_delete': {
        const host = await this.git('remoteHost', { path: args.path });
        if (!host) throw 'this remote is a local path: it has no token';
        this.keychain.delete(`git/${host}/username`);
        return this.keychain.delete(`git/${host}/token`);
      }
      default:
        throw `unknown command ${cmd}`;
    }
  }

  // http::send: the proxy's envelope in, header pairs and the body out.
  async send(a) {
    this.sent.push(a);
    const started = Date.now();
    const body = a.body == null ? undefined : a.bodyEncoding === 'base64' ? Buffer.from(a.body, 'base64') : a.body;
    const jar = a.cookies ? this.loadJar() : null;
    const headers0 = a.headers.filter(([k]) => k);
    if (jar && !headers0.some(([k, v]) => k.toLowerCase() === 'cookie' && v)) {
      const cookie = this.jar.cookieHeader(jar, a.url);
      if (cookie) headers0.push(['Cookie', cookie]);
    }
    let res;
    try {
      res = await fetch(a.url, { method: a.method, headers: headers0, body: ['GET', 'HEAD'].includes(a.method) ? undefined : body, redirect: 'follow' });
    } catch (e) {
      throw `error sending request for url (${a.url}): ${e.cause?.message ?? e.message}`;
    }
    const headers = [];
    res.headers.forEach((v, k) => k !== 'set-cookie' && headers.push([k, v]));
    for (const c of res.headers.getSetCookie()) headers.push(['set-cookie', c]);
    let cookies;
    if (jar) {
      cookies = this.jar.mergeCookies(jar, res.headers.getSetCookie(), res.url || a.url);
      this.saveJar(cookies);
    }
    const bytes = Buffer.from(await res.arrayBuffer());
    let text;
    try {
      text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch {
      text = null;
    }
    return { status: res.status, statusText: res.statusText, url: res.url, headers, body: text ?? bytes.toString('base64'), ...(text == null ? { bodyEncoding: 'base64' } : {}), timeMs: Date.now() - started, ...(cookies ? { cookies } : {}) };
  }

  // commands.rs ws_connect: Node's WebSocket (undici) sends handshake
  // headers as Rust's client does; events go to the page's channel callback
  // as { message, index } (WsEvent: message, closed, error).
  async wsConnect(a, channel) {
    const cb = Number(String(channel).replace('__CHANNEL__:', ''));
    let index = 0;
    const emit = (message) => this.deliver?.(cb, { message, index: index++ });
    this.wsConnects.push({ url: a.url, headers: a.headers, protocols: a.protocols });
    const ws = new WebSocket(a.url, { protocols: a.protocols?.length ? a.protocols : undefined, headers: Object.fromEntries(a.headers || []) });
    ws.binaryType = 'arraybuffer';
    await new Promise((resolve, reject) => {
      ws.onopen = resolve;
      ws.onerror = () => reject(`couldn't connect to ${a.url}`);
    });
    const id = ++this.nextSocket;
    this.sockets.set(id, ws);
    ws.onmessage = (e) => emit(typeof e.data === 'string' ? { type: 'message', data: e.data, binary: false } : { type: 'message', data: Buffer.from(e.data).toString('base64'), binary: true });
    ws.onerror = null;
    ws.onclose = (e) => {
      this.sockets.delete(id);
      emit({ type: 'closed', code: e.code || null, reason: e.reason || '' });
    };
    return { id, status: 101, protocol: ws.protocol || null, headers: [] };
  }

  // commands.rs grpc_open: connect, then the schema from the .proto files
  // (paths relative to baseDir; the greeter needs its import path) or
  // reflection.
  grpcOpen(a) {
    const uri = /^https?:\/\//i.test(a.address) ? a.address : `${a.plaintext ? 'http' : 'https'}://${a.address}`;
    const plain = /^http:/i.test(uri);
    if (uri.replace(/^https?:\/\//i, '') !== this.grpcServer.address || !plain) throw `couldn't connect to ${uri}: transport error`;
    if (!(a.protos || []).length) {
      if (!this.grpcServer.reflection) throw 'server reflection failed: the server has no reflection service (grpc.reflection.v1 or v1alpha): give its .proto files';
      return { source: 'reflection v1' };
    }
    const at = (p) => (path.isAbsolute(p) || !a.baseDir ? p : path.join(a.baseDir, p));
    for (const p of a.protos) if (!fs.existsSync(at(p))) throw `no .proto file at ${at(p)}`;
    const dirs = [...a.protos.map((p) => path.dirname(at(p))), ...(a.importPaths || []).map(at)];
    if (!dirs.some((d) => fs.existsSync(path.join(d, 'common.proto')))) throw 'greeter.proto: import "common.proto" was not found';
    return { source: 'protos' };
  }

  grpcDescribe(a, symbol) {
    this.grpcOpen(a);
    const [service, method] = symbol.split('/');
    const m = GRPC_SERVICES.find((s) => s.name === service)?.methods.find((x) => x.name === method);
    if (!m) throw `no service, method, message or enum '${symbol}' in the schema`;
    return { description: `${m.fullName}\nrpc ${m.name}(${m.input}) returns (${m.output});`, template: GRPC_TEMPLATES[m.input] ?? {} };
  }

  async grpcCall(a, channel) {
    this.grpcOpen(a);
    const svc = GRPC_SERVICES.find((s) => s.name === a.service);
    if (!svc) throw `no service '${a.service}' (the schema has: ${GRPC_SERVICES.map((s) => s.name).join(', ')})`;
    const m = svc.methods.find((x) => x.name === a.method);
    if (!m) throw `service ${svc.name} has no method '${a.method}' (it has: ${svc.methods.map((x) => x.name).join(', ')})`;
    if (m.clientStreaming) throw `${m.fullName} is a bidirectional streaming method: hitop calls unary and server-streaming methods only`;
    let msg;
    try {
      msg = JSON.parse(a.message.trim() || '{}');
    } catch (e) {
      throw `the message isn't a valid ${m.input}: ${e.message}`;
    }
    this.grpcCalls.push(a);
    const id = this.grpcCalls.length;
    const cb = Number(String(channel).replace('__CHANNEL__:', ''));
    let index = 0;
    const emit = (message) => this.deliver?.(cb, { message, index: index++ });
    const meta = (name) => (a.metadata || []).find(([k]) => k.toLowerCase() === name)?.[1] ?? '';
    const ok = { type: 'status', code: 0, name: 'OK', message: '', trailers: [] };
    setTimeout(() => {
      if (this.grpcCancelled.includes(id)) return;
      if (m.name === 'Fail') return emit({ type: 'status', code: 5, name: 'NOT_FOUND', message: `no user named ${msg.name || ''}`, trailers: [['x-reason', 'test']] });
      emit({ type: 'headers', metadata: [['x-served-by', 'hitop-test']] });
      if (m.name === 'SayHello') emit({ type: 'message', data: { message: `Hello, ${msg.name || ''}`, tenant: meta('x-tenant'), authorization: meta('authorization'), index: 0 } });
      else for (let i = 0; i < (msg.times || 0); i++) emit({ type: 'message', data: { message: `Hello #${i}, ${msg.name || ''}`, tenant: '', authorization: '', index: i } });
      emit(ok);
    }, 20);
    return { id };
  }

  // git.rs `git`: fetch, pull and push get the remote host's token.
  async git(cmd, args) {
    if (!fs.existsSync(GIT_JSON)) throw `no ${GIT_JSON}: cargo build --manifest-path cli/Cargo.toml -p hitop-git --example git-json --no-default-features`;
    this.gitCalls = [...(this.gitCalls || []), cmd];
    let token;
    if (['fetch', 'pull', 'push'].includes(cmd)) {
      const host = await this.git('remoteHost', { path: args.path }).catch(() => null);
      token = host ? this.keychain.get(`git/${host}/token`) : undefined;
    }
    const env = { ...process.env, ...(token ? { HITOP_GIT_TOKEN: token } : {}) };
    const out = await new Promise((resolve, reject) => execFile(GIT_JSON, [cmd, JSON.stringify(args)], { env }, (e, stdout) => (e ? reject(`git-json failed: ${e.message}`) : resolve(stdout))));
    const answer = JSON.parse(out);
    if ('error' in answer) throw answer.error;
    return answer.ok;
  }

  cleanup() {
    for (const ws of this.sockets.values()) ws.close();
    fs.rmSync(this.dir, { recursive: true, force: true });
  }
}

// hitop-core's tests/fixtures/grpc/greeter.proto, as grpc::services lists it.
const GRPC_METHOD = (name, input, output, clientStreaming, serverStreaming) => ({ name, fullName: `hitop.test.Greeter.${name}`, input, output, clientStreaming, serverStreaming });
const GRPC_SERVICES = [
  {
    name: 'grpc.reflection.v1.ServerReflection',
    methods: [{ name: 'ServerReflectionInfo', fullName: 'grpc.reflection.v1.ServerReflection.ServerReflectionInfo', input: 'grpc.reflection.v1.ServerReflectionRequest', output: 'grpc.reflection.v1.ServerReflectionResponse', clientStreaming: true, serverStreaming: true }],
  },
  {
    name: 'hitop.test.Greeter',
    methods: [
      GRPC_METHOD('SayHello', 'hitop.test.HelloRequest', 'hitop.test.HelloReply', false, false),
      GRPC_METHOD('StreamHellos', 'hitop.test.HelloRequest', 'hitop.test.HelloReply', false, true),
      GRPC_METHOD('Fail', 'hitop.test.HelloRequest', 'hitop.test.HelloReply', false, false),
      GRPC_METHOD('Chat', 'hitop.test.HelloRequest', 'hitop.test.HelloReply', true, true),
    ],
  },
];
// grpc::template of the request type.
const GRPC_TEMPLATES = { 'hitop.test.HelloRequest': { name: '', times: 0, mood: 'MOOD_UNSPECIFIED', tag: { key: '' }, labels: [''], attributes: {}, email: '' } };

module.exports = { TauriMock, listTree };
