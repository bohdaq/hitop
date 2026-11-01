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
//   cookies   hitop-core's jar (cookies.rs, same rules as features.js) in
//             <config>/cookies/global.json
const fs = require('fs');
const os = require('os');
const path = require('path');
const vm = require('vm');

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
    this.writes = [];
    this.nextFolder = null;
    this.lastCall = 0;
    this.telemetry = { enabled: false, asked: true, id: null, lastPing: null };
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
      case 'secret_get':
        return this.keychain.get(args.account) ?? null;
      case 'secret_set':
        this.keychain.set(args.account, args.value);
        return null;
      case 'secret_delete':
        return this.keychain.delete(args.account);
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

  cleanup() {
    fs.rmSync(this.dir, { recursive: true, force: true });
  }
}

module.exports = { TauriMock, listTree };
