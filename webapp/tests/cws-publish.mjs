// scripts/cws-publish.mjs against a fake Chrome Web Store API and token endpoint (node:http):
// token refresh, upload succeeded / in progress then polled / failed, version mismatch, publish
// state, missing credentials (notice, exit 0) and --dry-run (no request at all).
//   node tests/cws-publish.mjs
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("../scripts/cws-publish.mjs", import.meta.url));
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cws-"));
const zip = path.join(dir, "hitop-chrome.zip");
fs.writeFileSync(zip, Buffer.from("PK\u0003\u0004 fake zip"));

const ITEM = "/v2/publishers/pub-1/items/mmbpagencfjfgeicmeobbmeocdpeokdk";
let requests = [];
let plan = {};
const server = http.createServer((req, res) => {
  const chunks = [];
  req.on("data", (c) => chunks.push(c));
  req.on("end", () => {
    const body = Buffer.concat(chunks);
    requests.push({ method: req.method, url: req.url, auth: req.headers.authorization, type: req.headers["content-type"], body });
    const send = (status, json) => {
      res.writeHead(status, { "Content-Type": "application/json" });
      res.end(JSON.stringify(json));
    };
    if (req.url === "/token") {
      const form = new URLSearchParams(body.toString());
      if (form.get("grant_type") !== "refresh_token" || form.get("refresh_token") !== "refresh-1" || form.get("client_secret") !== "secret-1") {
        return send(400, { error: "invalid_grant" });
      }
      return send(200, { access_token: "access-1", expires_in: 3600, token_type: "Bearer" });
    }
    if (req.headers.authorization !== "Bearer access-1") return send(401, { error: { code: 401, message: "no auth" } });
    if (req.url === `/upload${ITEM}:upload`) return send(plan.uploadStatus ?? 200, plan.upload);
    if (req.url === `${ITEM}:fetchStatus`) return send(200, plan.statuses.shift());
    if (req.url === `${ITEM}:publish`) return send(200, plan.publish);
    send(404, { error: { message: `unexpected ${req.url}` } });
  });
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;

const creds = {
  CWS_CLIENT_ID: "client-1",
  CWS_CLIENT_SECRET: "secret-1",
  CWS_REFRESH_TOKEN: "refresh-1",
  CWS_PUBLISHER_ID: "pub-1",
};
const run = (args, env = {}) =>
  new Promise((resolve) => {
    const clean = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("CWS_") && k !== "GITHUB_ACTIONS"));
    execFile(
      process.execPath,
      [script, ...args],
      { env: { ...clean, CWS_API_BASE: base, CWS_TOKEN_URL: `${base}/token`, CWS_POLL_MS: "10", CWS_POLL_TRIES: "5", ...env } },
      (error, stdout, stderr) => resolve({ code: error ? error.code : 0, out: stdout + stderr }),
    );
  });

let passed = 0;
const test = async (name, fn) => {
  requests = [];
  plan = {};
  await fn();
  passed++;
  console.log(`ok ${name}`);
};

await test("uploads, then submits for review", async () => {
  plan = { upload: { itemId: "x", crxVersion: "5.0.1", uploadState: "SUCCEEDED" }, publish: { itemId: "x", state: "PENDING_REVIEW" } };
  const r = await run([zip, "--version", "5.0.1"], creds);
  assert.equal(r.code, 0, r.out);
  assert.match(r.out, /item state PENDING_REVIEW/);
  assert.deepEqual(requests.map((q) => q.url), ["/token", `/upload${ITEM}:upload`, `${ITEM}:publish`]);
  assert.equal(requests[1].method, "POST");
  assert.equal(requests[1].type, "application/zip");
  assert.deepEqual(requests[1].body, fs.readFileSync(zip));
  assert.equal(requests[2].auth, "Bearer access-1");
  assert.doesNotMatch(r.out, /access-1|refresh-1|secret-1/);
});

// An older submission still in review (5.0.0) must not count as the uploaded package's version.
await test("polls fetchStatus while the upload is in progress", async () => {
  plan = {
    upload: { itemId: "x", uploadState: "IN_PROGRESS" },
    statuses: [{ lastAsyncUploadState: "IN_PROGRESS" }, { lastAsyncUploadState: "SUCCEEDED", submittedItemRevisionStatus: { state: "PENDING_REVIEW", distributionChannels: [{ crxVersion: "5.0.0" }] } }],
    publish: { state: "PUBLISHED", warningInfo: { warnings: [{ reason: "W1", description: "a warning" }] } },
  };
  const r = await run([zip, "--version", "5.0.1"], creds);
  assert.equal(r.code, 0, r.out);
  assert.equal(requests.filter((q) => q.url.endsWith(":fetchStatus")).length, 2);
  assert.match(r.out, /warning: W1: a warning/);
  assert.match(r.out, /item state PUBLISHED/);
});

await test("a failed upload exits 1 and never publishes", async () => {
  plan = { upload: { itemId: "x", uploadState: "FAILED" } };
  const r = await run([zip, "--version", "5.0.1"], creds);
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /state FAILED/);
  assert.ok(!requests.some((q) => q.url.endsWith(":publish")));
});

await test("an HTTP error exits 1 with the API's message", async () => {
  plan = { uploadStatus: 400, upload: { error: { code: 400, message: "version must be higher" } } };
  const r = await run([zip, "--version", "5.0.1"], creds);
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /upload: HTTP 400 .*version must be higher/);
});

await test("a version the store read differently exits 1", async () => {
  plan = { upload: { crxVersion: "5.0.0", uploadState: "SUCCEEDED" } };
  const r = await run([zip, "--version", "5.0.1"], creds);
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /read version 5\.0\.0/);
});

await test("a rejected submission exits 1", async () => {
  plan = { upload: { uploadState: "SUCCEEDED" }, publish: { state: "REJECTED" } };
  const r = await run([zip, "--version", "5.0.1"], creds);
  assert.equal(r.code, 1, r.out);
});

await test("a refused refresh token exits 1", async () => {
  const r = await run([zip, "--version", "5.0.1"], { ...creds, CWS_REFRESH_TOKEN: "wrong" });
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /access token: HTTP 400/);
  assert.doesNotMatch(r.out, /wrong|secret-1/);
});

await test("missing credentials: a notice, exit 0, no request", async () => {
  const r = await run([zip, "--version", "5.0.1"], { ...creds, CWS_PUBLISHER_ID: "", GITHUB_ACTIONS: "true" });
  assert.equal(r.code, 0, r.out);
  assert.match(r.out, /::notice title=Chrome Web Store::CWS_PUBLISHER_ID not set/);
  assert.equal(requests.length, 0);
});

await test("--dry-run sends nothing", async () => {
  const r = await run([zip, "--version", "5.0.1", "--dry-run"], creds);
  assert.equal(r.code, 0, r.out);
  assert.match(r.out, /dry run: would upload/);
  assert.equal(requests.length, 0);
});

await test("usage errors exit 2", async () => {
  assert.equal((await run([zip], creds)).code, 2);
  assert.equal((await run([path.join(dir, "missing.zip"), "--version", "1.0.0"], creds)).code, 2);
});

server.close();
fs.rmSync(dir, { recursive: true, force: true });
console.log(`cws-publish: ${passed} passed`);
