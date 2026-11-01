// Postman scripts and import in the web app (no browser): node tests/pm.mjs
// The shim is shared/pm-shim.js, the same file hitop-core runs; the import is
// hitop-core's (hitop-wasm) with the page's script checks.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { makeAssert, parseEnvironments } from "../src/features.js";
import { fillVariables, forWebApp, issueText, isPostmanCollection, loadShim, postmanScripts, runScript, scriptScans, usesPostmanApi } from "../src/pm.js";
import { formatFromBytes } from "../src/wasm/format.js";

const here = dirname(fileURLToPath(import.meta.url));
const shim = loadShim(readFileSync(join(here, "../../shared/pm-shim.js"), "utf8"));
const fixtures = join(here, "../../cli/fixtures/postman");
const format = await formatFromBytes(readFileSync(join(here, "../src/wasm/pkg/hitop_wasm_bg.wasm")));

// A post-request script as the store runs it.
function post(script, { status = 200, text = "{}", headers = { "content-type": "application/json" } } = {}) {
  const { assert: a, results } = makeAssert({ statusCode: status, text, headers, durationMs: 12 });
  const vars = {};
  runScript(shim, script, { assert: a, responseText: text }, {
    vars: { base: "http://x" },
    setVar: (k, v) => (vars[k] = v),
    assert: a,
    request: { url: "http://x/get", method: "GET", headers: [["Accept", "*/*"]], body: "" },
    response: { code: status, text, headers, time: 12 },
  });
  return { results: results.map((r) => [r.name, r.pass, r.detail]), vars };
}

// newman's sample checks, passing and failing with chai's message.
const sample = JSON.parse(readFileSync(join(fixtures, "newman-sample/collection.json"), "utf8"));
const sampleTest = sample.item[0].event[0].script.exec.join("\n");
assert.deepEqual(post(sampleTest, { text: '{"args":{"source":"newman-sample-github-collection"}}' }).results, [
  ["expect response be 200", true, ""],
  ["expect response json contain args", true, ""],
]);
assert.deepEqual(post(sampleTest, { status: 500, text: '{"args":{"source":"x"}}' }).results, [
  ["expect response be 200", false, "expected response to have status code 2xx but got 500"],
  ["expect response json contain args", false, "expected 'x' to equal 'newman-sample-github-collection'"],
]);

// Variables, the legacy sandbox, pm.request.
const vars = post("pm.environment.set('token', pm.response.json().t); pm.globals.unset('base'); tests['legacy'] = responseCode.code === 200; pm.test('req', () => pm.expect(pm.request.url.toString()).to.equal('http://x/get'));", { text: '{"t":"abc"}' });
assert.deepEqual(vars.vars, { token: "abc", base: null });
assert.deepEqual(vars.results, [["req", true, ""], ["legacy", true, ""]]);

// Pre-request: pm.request edits go through the host functions.
const ctx = { url: "http://x/a", headers: [{ name: "X-Old", value: "1" }], body: "" };
ctx.setUrl = (u) => (ctx.url = u);
ctx.setHeader = (n, v) => ctx.headers.push({ name: n, value: v });
ctx.setBody = (b) => (ctx.body = b);
const preVars = {};
runScript(shim, "pm.request.headers.add({key: 'X-New', value: 'v'}); pm.request.headers.remove('x-old'); pm.request.url = pm.request.url + '?q=1'; pm.variables.set('ts', 7);", ctx, {
  vars: {},
  setVar: (k, v) => (preVars[k] = v),
  assert: { ok() {} },
  request: { get url() { return ctx.url; }, method: "GET", get headers() { return ctx.headers.map((h) => [h.name, h.value]); }, get body() { return ctx.body; } },
  setUrl: ctx.setUrl,
  setHeader: ctx.setHeader,
  removeHeader: (n) => (ctx.headers = ctx.headers.filter((h) => h.name.toLowerCase() !== n.toLowerCase())),
  setBody: ctx.setBody,
  response: null,
});
assert.equal(ctx.url, "http://x/a?q=1");
assert.deepEqual(ctx.headers, [{ name: "X-New", value: "v" }]);
assert.deepEqual(preVars, { ts: "7" });
assert.equal(fillVariables("${a}/${b}", { a: 1 }), "1/${b}");
assert.ok(!usesPostmanApi("assert.status(200); setContext('pmx', 1)"), "hitop scripts run without the shim");

// Import: hitop-core's importer, the shim's checks, the same report as `hitop import`.
const report = {
  info: { name: "Report", schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
  auth: { type: "bearer", bearer: [{ key: "token", value: "{{token}}" }] },
  item: [
    {
      name: "Folder",
      event: [{ listen: "prerequest", script: { exec: ["pm.sendRequest('x', () => {});"] } }],
      item: [{ name: "Get", request: { method: "GET", url: "{{base}}/x", auth: { type: "oauth2" } }, event: [{ listen: "test", script: { exec: ["pm.test('ok', () => {});", "postman.setNextRequest(null);"] } }] }],
    },
    { name: "Open", request: { method: "GET", url: "{{base}}/open" } },
  ],
};
assert.ok(isPostmanCollection(report));
assert.equal(postmanScripts(report).length, 2);
const out = format.importPostman(JSON.stringify(report), scriptScans(shim, report));
assert.deepEqual(out.report.issues.map(issueText), [
  "folder Folder (script before), line 1: pm.sendRequest",
  "Folder / Get (script after), line 2: postman.setNextRequest",
  "Folder / Get (auth): oauth2 auth",
]);
const web = forWebApp(out.collection);
assert.equal(web.auth, undefined, "the web app has no collection auth");
assert.deepEqual(web.requests.map((r) => [r.file, r.url, r.auth?.type]), [["folder/get", "${base}/x", "none"], ["open", "${base}/open", "bearer"]]);

// The three real collections import without script issues the shim can't run.
for (const name of ["conduit", "newman-sample", "adyen-binlookup"]) {
  const json = readFileSync(join(fixtures, name, "collection.json"), "utf8");
  const r = format.importPostman(json, scriptScans(shim, JSON.parse(json)));
  assert.deepEqual(r.report.issues, [], name);
  assert.ok(r.collection.requests.length > 0, name);
}

// Postman environments: the secret type makes a secret (value kept in storage, masked).
const [env] = parseEnvironments(readFileSync(join(fixtures, "conduit/conduit.postman_environment.json"), "utf8"));
assert.deepEqual(env.secrets, ["PASSWORD"]);
assert.equal(env.variables.PASSWORD, "dummy-password-1");

console.log("pm checks passed");
