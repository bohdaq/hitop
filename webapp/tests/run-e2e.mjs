// The end-to-end suites, one after another: node tests/run-e2e.mjs [--shard i/n] (HITOP_MODE=tauri
// for the desktop build). Every suite is in exactly one shard, and every .cjs file here is a suite
// or named below: a new one can't be forgotten.
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const SUITES = [
  "e2e", "editing", "features", "extras", "folders", "postman",
  "settings", "globals", "graphql", "diff-code", "websocket", "grpc",
  "openapi", "git", "secret-refs", "ux", "ux12", "onboarding",
  "sse", "ai", "sync", "library", "devices", "account-delete",
  "limits", "github", "next", "collections-ux", "stories", "first-visit",
  "ux2", "tree-dnd", "tabs-dnd", "ux3", "updater", "store-mode", "run-cancel",
];
// Helpers, screenshots (visual.cjs too), and the extension and Firefox checks (their own CI jobs).
const OTHERS = ["harness", "fake-github", "tauri-mock", "shots-fv1", "shots-ux2", "store-shots", "extension", "firefox", "visual"];

const files = readdirSync(here).filter((f) => f.endsWith(".cjs")).map((f) => f.slice(0, -4));
const unknown = files.filter((f) => !SUITES.includes(f) && !OTHERS.includes(f));
const missing = SUITES.filter((s) => !files.includes(s));
if (unknown.length || missing.length || new Set(SUITES).size !== SUITES.length) {
  console.error(`tests/run-e2e.mjs: not listed: ${unknown.join(", ") || "none"}; listed but not there: ${missing.join(", ") || "none"}`);
  process.exit(2);
}

const arg = process.argv.indexOf("--shard");
let [i, n] = [1, 1];
if (arg > 0) {
  [i, n] = (process.argv[arg + 1] ?? "").split("/").map(Number);
  if (!(n >= 1 && i >= 1 && i <= n)) {
    console.error("--shard i/n");
    process.exit(2);
  }
}
const mine = SUITES.filter((_, at) => at % n === i - 1);
console.log(`shard ${i}/${n}: ${mine.join(" ")}`);
for (const s of mine) {
  const started = Date.now();
  const r = spawnSync(process.execPath, [join(here, `${s}.cjs`)], { stdio: "inherit", cwd: join(here, "..") });
  console.log(`[${s}] ${Math.round((Date.now() - started) / 1000)} s`);
  if (r.status !== 0) process.exit(r.status ?? 1);
}
