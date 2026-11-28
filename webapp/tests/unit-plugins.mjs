// Plugin calls in the web app (src/plugin-calls.js) and hitop-core's grammar through hitop-wasm:
// node tests/unit-plugins.mjs (after npm run wasm)
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { callArguments, fromSecret, pluginCallTexts, secretPluginCalls, withPluginValues } from "../src/plugin-calls.js";
import { formatFromBytes } from "../src/wasm/format.js";
import { interpolateVariables } from "../../shared/services/variableInterpolation.js";

const here = dirname(fileURLToPath(import.meta.url));
const format = await formatFromBytes(readFileSync(join(here, "../src/wasm/pkg/hitop_wasm_bg.wasm")));

test("calls are found where a request is filled in, not in its scripts or examples", () => {
  const source = { url: "${base}/p?sig=${ plugin.b64(cred) }", headers: [{ name: "X", value: '${plugin.b64("hi")}' }], preRequestScript: "${plugin.no()}", examples: [{ body: "${plugin.no()}" }], body: "${plugin.name}" };
  assert.deepEqual([...pluginCallTexts(source)], ["plugin.b64(cred)", 'plugin.b64("hi")']);
});

test("hitop-wasm parses calls with hitop-core's grammar", () => {
  assert.deepEqual(format.pluginCall('plugin.sig(key, "a \\"b\\" \\\\", x.y-z)'), { plugin: "sig", args: [{ variable: "key" }, { text: 'a "b" \\' }, { variable: "x.y-z" }] });
  assert.deepEqual(format.pluginCall("plugin.now()"), { plugin: "now", args: [] });
  for (const bad of ["plugin.Sig()", "plugin.sig", "plugin.sig(a b)", 'plugin.sig("x)', "plugin.(x)", "other.sig()"]) assert.equal(format.pluginCall(bad), null, bad);
  assert.equal(JSON.parse(format.pluginManifest('/* hitop-plugin {"name": "b64", "version": "1", "hooks": ["value"]} */\nfunction value() {}')).name, "b64");
  assert.throws(() => format.pluginManifest("function value() {}"), /starts with its manifest/);
  assert.throws(() => format.pluginManifest('/* hitop-plugin {"name": "B", "version": "1", "hooks": ["value"]} */'), /isn't valid/);
});

test("arguments: variable values and texts; a variable with no value is named", () => {
  const call = format.pluginCall('plugin.sig(key, "t")');
  assert.deepEqual(callArguments(call, { key: "k" }), ["k", "t"]);
  assert.throws(() => callArguments(call, {}), /Variable 'key' not found/);
});

test("answers fill calls in, also inside variable values; a variable named like a call wins", () => {
  const answers = { "plugin.b64(cred)": "Y3JlZA==", "plugin.n()": "7" };
  const vars = withPluginValues({ cred: "cred", nonce: "n-${plugin.n()}", "plugin.n()": "mine" }, answers);
  assert.equal(interpolateVariables("${ plugin.b64(cred) }&${nonce}&${plugin.n()}", vars), "Y3JlZA==&n-7&mine");
  // Without an answer the call is still refused, never sent as text.
  assert.throws(() => interpolateVariables("${plugin.other()}", vars), /plugin 'other'/);
});

test("an answer computed from a secret is a secret (T11)", () => {
  assert.equal(fromSecret(["Bearer s3cr3t"], ["s3cr3t"]), true);
  assert.equal(fromSecret(["hi"], ["s3cr3t", ""]), false);
});

test("the editor names calls passed a secret by name or by value (T1)", () => {
  const calls = ["plugin.b64(cred)", 'plugin.b64("s3cr3t")', "plugin.b64(user)", "plugin.nonce()", "plugin.b64(token)"].map((text) => ({ text, call: format.pluginCall(text) }));
  const variables = { cred: "", user: "bob", token: "Bearer s3cr3t" };
  assert.deepEqual(
    secretPluginCalls(calls, variables, ["cred"], ["s3cr3t"]).map((c) => c.text),
    ["plugin.b64(cred)", 'plugin.b64("s3cr3t")', "plugin.b64(token)"],
  );
});

// Part 3c: the hook answers every screen gets, checked by hitop-core through hitop-wasm.
test("hook answers: one validator, a viewer's answer shown masked and printable", () => {
  assert.equal(format.pluginAnswer("p", "value", "x"), "x");
  assert.throws(() => format.pluginAnswer("p", "value", 42), /plugin 'p' answered something other than text/);
  const table = format.pluginAnswer("p", "viewer", { columns: ["k", 2], rows: [["<b>", null], [true]] });
  assert.deepEqual(table, { columns: ["k", "2"], rows: [["<b>", ""], ["true", ""]] });
  assert.throws(() => format.pluginAnswer("p", "viewer", { columns: Array.from({ length: 101 }, (_, i) => `c${i}`), rows: [] }), /101 columns/);
  assert.deepEqual(format.pluginViewerShown("a\u001bs3cret\n", ["s3cret"]), "a�••••\n");
  assert.deepEqual(format.pluginViewerShown(table, ["<b>"]), { columns: ["k", "2"], rows: [["••••", ""], ["true", ""]] });
  const input = format.pluginViewerInput(200, [["X-B", "2"], ["a", "1"]], "body");
  assert.deepEqual(input, { status: 200, headers: [{ name: "a", value: "1" }, { name: "x-b", value: "2" }], body: "body", truncated: false });
});

test("an importer's collections lose workspace and org; a file outside the folder is refused", () => {
  const c = { id: 1, name: "c", workspace: "w", org: "o", requests: [{ id: 2, name: "r", method: "GET", url: "/r", headers: [], body: "" }] };
  const [got] = format.pluginAnswer("imp", "importer", c);
  assert.equal(got.name, "c");
  assert.ok(!("workspace" in got) && !("org" in got), JSON.stringify(got));
  assert.throws(() => format.pluginAnswer("imp", "importer", { ...c, requests: [{ ...c.requests[0], file: "../x" }] }));
});

test("a snippet plugin's input has placeholders for secrets, never their values", () => {
  const input = format.snippetPluginInput({ request: { id: 0, name: "r", method: "GET", url: "https://api.example/x", headers: [{ name: "Authorization", value: "Bearer ${cred}" }], body: "" }, variables: { cred: "s3cret" }, secrets: ["cred"] });
  assert.ok(JSON.stringify(input).includes("{{cred}}") && !JSON.stringify(input).includes("s3cret"), JSON.stringify(input));
});
