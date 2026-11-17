// Uploads a Chrome Web Store package and submits it for review (Chrome Web Store API v2,
// developer.chrome.com/docs/webstore/using-api). Used by .github/workflows/release-extensions.yml.
//   node scripts/cws-publish.mjs <zip> --version <x.y.z> [--dry-run]
// Credentials come from the environment, never from arguments:
//   CWS_CLIENT_ID, CWS_CLIENT_SECRET, CWS_REFRESH_TOKEN (OAuth client + refresh token, scope chromewebstore)
//   CWS_PUBLISHER_ID (Developer Dashboard > Publisher > Settings)
//   CWS_EXTENSION_ID (default: HITOP's item id)
// Any of them missing: a notice and exit 0 (the store is skipped, the release goes on).
// --dry-run: checks the file and prints what it would do, without a single request.
// Tests point CWS_API_BASE and CWS_TOKEN_URL at a fake server (tests/cws-publish.mjs).
import fs from "node:fs";

const HITOP_ITEM = "mmbpagencfjfgeicmeobbmeocdpeokdk";
const env = process.env;
const API = (env.CWS_API_BASE || "https://chromewebstore.googleapis.com").replace(/\/$/, "");
const TOKEN_URL = env.CWS_TOKEN_URL || "https://oauth2.googleapis.com/token";
const POLL_MS = Number(env.CWS_POLL_MS || 5000);
const POLL_TRIES = Number(env.CWS_POLL_TRIES || 60);
const inActions = env.GITHUB_ACTIONS === "true";

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const vi = args.indexOf("--version");
const version = vi >= 0 ? args[vi + 1] : undefined;
const zip = args.find((a, i) => !a.startsWith("--") && args[i - 1] !== "--version");
if (!zip || !version) {
  console.error("usage: cws-publish.mjs <zip> --version <x.y.z> [--dry-run]");
  process.exit(2);
}
if (!fs.existsSync(zip) || !fs.statSync(zip).isFile()) {
  console.error(`${zip}: no such file`);
  process.exit(2);
}

const notice = (msg) => console.log(inActions ? `::notice title=Chrome Web Store::${msg}` : `notice: ${msg}`);
const fail = (msg) => {
  console.error(inActions ? `::error title=Chrome Web Store::${msg}` : `error: ${msg}`);
  process.exit(1);
};

const missing = ["CWS_CLIENT_ID", "CWS_CLIENT_SECRET", "CWS_REFRESH_TOKEN", "CWS_PUBLISHER_ID"].filter((k) => !env[k]);
const itemId = env.CWS_EXTENSION_ID || HITOP_ITEM;
const item = `publishers/${env.CWS_PUBLISHER_ID || "<publisher>"}/items/${itemId}`;
const size = fs.statSync(zip).size;

if (dryRun) {
  console.log(`dry run: would upload ${zip} (${size} bytes, version ${version}) to ${API}/upload/v2/${item}:upload, then ${API}/v2/${item}:publish`);
  if (missing.length) notice(`not configured (${missing.join(", ")} missing): a real run would skip the Chrome Web Store`);
  process.exit(0);
}
if (missing.length) {
  notice(`${missing.join(", ")} not set: skipping the Chrome Web Store upload. Upload ${zip.split("/").pop()} in the Developer Dashboard instead.`);
  process.exit(0);
}

async function call(url, init, what) {
  let res;
  try {
    res = await fetch(url, init);
  } catch (e) {
    fail(`${what}: ${e.message}`);
  }
  const text = await res.text();
  let body;
  try {
    body = text ? JSON.parse(text) : {};
  } catch {
    body = { raw: text };
  }
  // Google's error bodies carry a message and status, never the credentials.
  if (!res.ok) fail(`${what}: HTTP ${res.status} ${JSON.stringify(body.error ?? body)}`);
  return body;
}

const tokenBody = await call(
  TOKEN_URL,
  {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      client_id: env.CWS_CLIENT_ID,
      client_secret: env.CWS_CLIENT_SECRET,
      refresh_token: env.CWS_REFRESH_TOKEN,
      grant_type: "refresh_token",
    }),
  },
  "access token",
);
const token = tokenBody.access_token;
if (!token) fail("access token: no access_token in the response");
if (inActions) console.log(`::add-mask::${token}`);
const auth = { Authorization: `Bearer ${token}` };

console.log(`Uploading ${zip} (${size} bytes, version ${version}) to item ${itemId}`);
const upload = await call(
  `${API}/upload/v2/${item}:upload`,
  { method: "POST", headers: { ...auth, "Content-Type": "application/zip" }, body: fs.readFileSync(zip) },
  "upload",
);
let state = upload.uploadState;
const crxVersion = upload.crxVersion;
for (let i = 0; (state === "IN_PROGRESS" || state === "UPLOAD_IN_PROGRESS") && i < POLL_TRIES; i++) {
  await new Promise((r) => setTimeout(r, POLL_MS));
  const status = await call(`${API}/v2/${item}:fetchStatus`, { headers: auth }, "fetchStatus");
  // Only the upload state: the revision statuses describe what was submitted or published before.
  state = status.lastAsyncUploadState;
}
if (state !== "SUCCEEDED") fail(`upload ended in state ${state ?? "(none)"}: ${JSON.stringify(upload)}`);
if (crxVersion && crxVersion !== version) fail(`the store read version ${crxVersion} from the package, expected ${version}`);
console.log(`Upload succeeded (version ${crxVersion ?? version})`);

// Submits for review; it goes live when approved (the dashboard's publish and rollout settings apply).
const published = await call(
  `${API}/v2/${item}:publish`,
  { method: "POST", headers: { ...auth, "Content-Type": "application/json" }, body: "{}" },
  "publish",
);
for (const w of published.warningInfo?.warnings ?? []) console.log(`warning: ${w.reason}: ${w.description}`);
if (published.state === "REJECTED" || published.state === "CANCELLED") fail(`publish: item state ${published.state}`);
console.log(`Submitted: item state ${published.state ?? "(not given)"}`);
if (inActions && env.GITHUB_STEP_SUMMARY) {
  fs.appendFileSync(env.GITHUB_STEP_SUMMARY, `Chrome Web Store: version ${version} uploaded and submitted, state \`${published.state ?? "unknown"}\`.\n`);
}
