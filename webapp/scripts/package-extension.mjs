// The browser extensions from the web app (HITOP Next A8): copies dist-extension/ (npm run
// build:extension) into an extension folder with its manifest, background script and icons.
//   node scripts/package-extension.mjs chrome|firefox|store <out dir>
// chrome   = chrome-extension/ (unpacked, app/ subfolder, as the React build had it)
// store    = chrome-store-package/ (Chrome Web Store: the app at the package root)
// firefox  = extension/ (Firefox: app/ subfolder, its own manifest with the gecko id)
// The React UI stays buildable: the repo's build-*.sh scripts take HITOP_EXTENSION_UI=react.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const [kind, out] = process.argv.slice(2);
if (!["chrome", "firefox", "store"].includes(kind) || !out) {
  console.error("usage: package-extension.mjs chrome|firefox|store <out dir>");
  process.exit(2);
}
const dist = path.join(root, "webapp/dist-extension");
if (!fs.existsSync(path.join(dist, "sandbox.html"))) {
  console.error("webapp/dist-extension is missing: run npm run build:extension in webapp/ first");
  process.exit(2);
}

const DESCRIPTION = "The API client your terminal, your CI and your AI agent can all drive.";
const VERSION = "5.0.0";
// Extension pages: own files only, plus WebAssembly (hitop-wasm). Scripts run in the sandbox.
const CSP_PAGES = "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'";
// The sandboxed script runner may evaluate scripts; it has no extension APIs and its own origin.
const CSP_SANDBOX = "sandbox allow-scripts; script-src 'self' 'unsafe-eval'; object-src 'none'";

const outDir = path.resolve(out);
const appDir = kind === "store" ? outDir : path.join(outDir, "app");
fs.rmSync(appDir, { recursive: true, force: true });
fs.mkdirSync(appDir, { recursive: true });
fs.cpSync(dist, appDir, { recursive: true });
const prefix = kind === "store" ? "" : "app/";

fs.mkdirSync(path.join(outDir, "icons"), { recursive: true });
const icon = (size, name = `icon-${size}.png`) => fs.copyFileSync(path.join(root, `brand/png/logo-${size}.png`), path.join(outDir, "icons", name));

let manifest;
if (kind === "firefox") {
  icon(48);
  // icon-96.png has always been the 192 px render (brand/BRAND.md).
  icon(192, "icon-96.png");
  manifest = {
    manifest_version: 3,
    name: "HITOP",
    version: VERSION,
    description: DESCRIPTION,
    icons: { 48: "icons/icon-48.png", 96: "icons/icon-96.png" },
    action: { default_icon: { 48: "icons/icon-48.png" }, default_title: "HITOP" },
    host_permissions: ["<all_urls>"],
    background: { scripts: ["background.js"] },
    browser_specific_settings: { gecko: { id: "hitop@example.com", strict_min_version: "109.0" } },
    content_security_policy: { extension_pages: CSP_PAGES },
  };
} else {
  for (const s of [16, 48, 128]) icon(s);
  manifest = {
    manifest_version: 3,
    name: "HITOP",
    version: VERSION,
    description: DESCRIPTION,
    action: { default_title: "HITOP" },
    icons: { 16: "icons/icon-16.png", 48: "icons/icon-48.png", 128: "icons/icon-128.png" },
    background: { service_worker: "background.js" },
    host_permissions: ["<all_urls>"],
    sandbox: { pages: [`${prefix}sandbox.html`] },
    content_security_policy: { extension_pages: CSP_PAGES, sandbox: CSP_SANDBOX },
  };
}
fs.writeFileSync(path.join(outDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);

const background =
  kind === "firefox"
    ? `// Opens HITOP in a tab when the toolbar icon is clicked.\nbrowser.action.onClicked.addListener(() => browser.tabs.create({ url: browser.runtime.getURL("${prefix}index.html") }));\n`
    : `// Opens HITOP in a tab when the toolbar icon is clicked.\nchrome.action.onClicked.addListener(() => chrome.tabs.create({ url: chrome.runtime.getURL("${prefix}index.html") }));\n`;
fs.writeFileSync(path.join(outDir, "background.js"), background);
console.log(`${kind} extension in ${outDir} (the web app, manifest ${VERSION})`);
