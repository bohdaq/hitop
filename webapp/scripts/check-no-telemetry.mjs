// The web app never contains the usage ping (ROADMAP section 2): its build
// (dist/) has no ping endpoint and no telemetry code, and the web adapter has
// no telemetry path. As a control, the desktop build (dist-tauri/) does have
// the Settings page that drives the Rust side. Run after `npm run build` and
// `npm run build:tauri`:  node scripts/check-no-telemetry.mjs
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const files = (dir) =>
  fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? files(path.join(dir, e.name)) : [path.join(dir, e.name)]));
// The Anthropic SDK chunk (the AI assistant, src/ai.js, loaded only when someone asks Claude
// with their own key) bundles the OpenTelemetry API and the Admin API's telemetry endpoints by
// name; it sends nothing but the person's own requests to api.anthropic.com. It isn't HITOP's
// usage ping, so that one chunk (told by a header only the SDK sets) is left out of the search.
const isAnthropicSdk = (text) => text.includes("anthropic-dangerous-direct-browser-access");
const has = (dir, re) =>
  files(dir).filter((f) => {
    const text = fs.readFileSync(f, "latin1");
    return re.test(text) && !isAnthropicSdk(text);
  });
const fail = (msg) => {
  console.error(`check-no-telemetry: ${msg}`);
  process.exit(1);
};

const web = path.join(root, "dist");
const tauri = path.join(root, "dist-tauri");
if (!fs.existsSync(web) || !fs.existsSync(tauri)) fail("build dist/ and dist-tauri/ first (npm run build && npm run build:tauri)");

const PING = /api\/ping/;
const CODE = /telemetry|usage ping|hitop-settings/i;
for (const [what, re] of [["the ping endpoint", PING], ["telemetry code", CODE]]) {
  const found = has(web, re);
  if (found.length) fail(`the web build has ${what}: ${found.map((f) => path.relative(root, f)).join(", ")}`);
}
const adapter = fs.readFileSync(path.join(root, "src/platform/web.js"), "utf8");
if (CODE.test(adapter) || PING.test(adapter) || /tauri-settings/.test(adapter)) fail("src/platform/web.js has a telemetry path");
// The desktop's JavaScript doesn't know the endpoint either: Rust sends the ping.
if (has(tauri, PING).length) fail("dist-tauri has the ping endpoint; only Rust may send pings");
if (!has(tauri, /telemetry_status/).length) fail("control failed: dist-tauri has no Settings page (telemetry_status)");
console.log("check-no-telemetry: the web build and web adapter have no usage ping; dist-tauri has the Settings page");
