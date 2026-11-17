// Builds cli/crates/hitop-wasm (hitop-core's collection format) into
// src/wasm/pkg/ for the web app: cargo for wasm32-unknown-unknown, then the
// wasm-bindgen CLI, whose version must equal the wasm-bindgen crate's.
//   node scripts/build-wasm.mjs
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WASM_BINDGEN = "0.2.126"; // = the crate pin in cli/crates/hitop-wasm/Cargo.toml
const here = dirname(fileURLToPath(import.meta.url));
const crate = join(here, "../../cli/crates/hitop-wasm");
const out = join(here, "../src/wasm/pkg");

const pin = readFileSync(join(crate, "Cargo.toml"), "utf8").match(/wasm-bindgen = "=([^"]+)"/)?.[1];
if (pin !== WASM_BINDGEN) throw new Error(`hitop-wasm pins wasm-bindgen ${pin}, this script expects ${WASM_BINDGEN}: change both`);

const tool = (name) => {
  if (process.env[name.toUpperCase().replace(/-/g, "_")]) return process.env[name.toUpperCase().replace(/-/g, "_")];
  const local = join(homedir(), ".cargo/bin", name);
  return existsSync(local) ? local : name;
};
const run = (cmd, args, opts = {}) => execFileSync(cmd, args, { stdio: ["ignore", "pipe", "inherit"], encoding: "utf8", ...opts });

const bindgen = tool("wasm-bindgen");
let version = "";
try {
  version = run(bindgen, ["--version"]).trim();
} catch {
  // reported below
}
if (version !== `wasm-bindgen ${WASM_BINDGEN}`) {
  console.error(`The web app needs the wasm-bindgen CLI ${WASM_BINDGEN} (found: ${version || "none"}).
Install it with: cargo install wasm-bindgen-cli --version ${WASM_BINDGEN} --locked
and the target with: rustup target add wasm32-unknown-unknown`);
  process.exit(1);
}

// Panic locations carry source paths: map the checkout and the cargo home to fixed names, so the
// .wasm (and the hashed file names after it) don't depend on where the build ran. AMO reviewers
// rebuild the Firefox extension and compare it file by file (scripts/extension-source.mjs).
const repo = join(here, "../..");
const cargoHome = process.env.CARGO_HOME || join(homedir(), ".cargo");
// CARGO_ENCODED_RUSTFLAGS (0x1f-separated) keeps paths with spaces whole; it replaces RUSTFLAGS, so those come along.
const flags = process.env.CARGO_ENCODED_RUSTFLAGS
  ? process.env.CARGO_ENCODED_RUSTFLAGS.split("\x1f")
  : (process.env.RUSTFLAGS ?? "").split(/\s+/).filter(Boolean);
flags.push(`--remap-path-prefix=${repo}=/hitop`, `--remap-path-prefix=${cargoHome}=/cargo`);
const env = { ...process.env, CARGO_PROFILE_RELEASE_DEBUG: "0", CARGO_ENCODED_RUSTFLAGS: flags.join("\x1f") };
delete env.RUSTFLAGS;
const cargoPath = tool("cargo");
const build = (dir, file, dest, label) => {
  // cwd inside cli/ so cli/rust-toolchain.toml picks the toolchain.
  execFileSync(cargoPath, ["build", "--release", "--locked", "--target", "wasm32-unknown-unknown"], { cwd: dir, stdio: "inherit", env });
  const wasm = join(dir, `target/wasm32-unknown-unknown/release/${file}`);
  execFileSync(bindgen, ["--target", "web", "--no-typescript", "--out-dir", dest, wasm], { stdio: "inherit" });
  console.log(`${label} → ${dest}`);
};
build(crate, "hitop_wasm.wasm", out, "hitop-wasm");
// --scripts: also hitop-scripts-wasm (boa + the pm.* shim) → src/wasm/pkg-scripts/, the Firefox
// extension's script runner (npm run build:extension).
if (process.argv.includes("--scripts")) {
  const scripts = join(here, "../../cli/crates/hitop-scripts-wasm");
  const spin = readFileSync(join(scripts, "Cargo.toml"), "utf8").match(/wasm-bindgen = "=([^"]+)"/)?.[1];
  if (spin !== WASM_BINDGEN) throw new Error(`hitop-scripts-wasm pins wasm-bindgen ${spin}, this script expects ${WASM_BINDGEN}: change both`);
  build(scripts, "hitop_scripts_wasm.wasm", join(here, "../src/wasm/pkg-scripts"), "hitop-scripts-wasm");
}
