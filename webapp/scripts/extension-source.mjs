// The source package addons.mozilla.org asks for with every version of a bundled extension
// (extensionworkshop.com/documentation/publish/source-code-submission/): the committed files the
// Firefox build reads (git archive of HEAD, lockfiles included, build output never), plus
// BUILD.md with the tool versions this build used and the commands that rebuild the package.
//   node scripts/extension-source.mjs <out.zip>
// release-extensions.yml uploads it with web-ext sign --upload-source-code and attaches it to the
// GitHub Release. Locally it archives HEAD: commit first.
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const out = process.argv[2];
if (!out) {
  console.error("usage: extension-source.mjs <out.zip>");
  process.exit(2);
}

// What `npm run build:extension` + package-extension.mjs read: the web app, the React app's shared
// services, the pm.* shim, the site's theme and mark, the Rust crates built to WebAssembly
// (hitop-wasm, hitop-scripts-wasm and their path dependencies) and the icons.
const PATHS = ["webapp", "frontend/src", "shared", "site/src", "cli", "brand/png", "LICENSE", ":(exclude)cli/fuzz"];

const tool = (name) => {
  const local = join(homedir(), ".cargo/bin", name);
  return existsSync(local) ? local : name;
};
const version = (cmd, args, cwd = root) => {
  try {
    return execFileSync(cmd, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return "(not found)";
  }
};
const extVersion = JSON.parse(readFileSync(join(root, "webapp/package.json"), "utf8")).extensionVersion;
const commit = version("git", ["rev-parse", "HEAD"]);
// cli/ for cli/rust-toolchain.toml, as build-wasm.mjs runs cargo.
const rustc = version(tool("rustc"), ["--version"], join(root, "cli"));
const cargo = version(tool("cargo"), ["--version"], join(root, "cli"));
const rustVersion = rustc.split(" ")[1] ?? "stable";
const bindgen = version(tool("wasm-bindgen"), ["--version"]);
const npm = version("npm", ["--version"]);

const readme = `# HITOP ${extVersion} for Firefox: building from source

The add-on is HITOP's web app (Lit, bundled by Vite) plus two Rust crates compiled to
WebAssembly. This archive is commit ${commit} of https://github.com/bohdaq/hitop
(only the parts the build reads). All tools are open source.

## Tools this version was built with

- OS: ${process.platform} ${process.arch} (GitHub Actions ubuntu-24.04)
- Node.js ${process.version}, npm ${npm}
- ${rustc} (${cargo}), target wasm32-unknown-unknown
- ${bindgen} (wasm-bindgen-cli; the version must match the crates' pin exactly)

## Install the tools (Ubuntu)

\`\`\`sh
# Node.js 22 or later, with npm: https://nodejs.org/en/download
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
rustup toolchain install ${rustVersion} --target wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126 --locked
\`\`\`

## Build

The build must run at the same paths as the release build: rustc writes source paths (the
checkout and the cargo home) into the WebAssembly, and Vite names files by content hash. The
script below unpacks this archive into /tmp/hitop-ext-src, uses /tmp/hitop-ext-cargo as the
cargo home, and builds there. It is the same script the release workflow ran.

\`\`\`sh
# cli/rust-toolchain.toml says "stable"; this pins the exact compiler this version was built with.
export RUSTUP_TOOLCHAIN=${rustVersion}
unzip -q hitop-firefox-${extVersion}-source.zip -d /tmp/hitop-unpack     # or wherever this README is
bash /tmp/hitop-unpack/webapp/scripts/build-extension-release.sh hitop-firefox-${extVersion}-source.zip /tmp/hitop-out
\`\`\`

\`/tmp/hitop-out/firefox/\` now holds the add-on: manifest.json, background.js, icons/ and app/ (the
bundled web app). It should match the submitted package file for file
(\`diff -r /tmp/hitop-out/firefox <unzipped submission>\`). The script runs \`npm ci\`,
\`npm run build:extension\` (Rust → wasm with \`cargo --locked\`, then \`vite build\` → dist-extension/)
and \`node scripts/package-extension.mjs firefox\`.

Where the bundled code comes from: \`webapp/src/\` (the app), \`frontend/src/services/\` and
\`frontend/src/utils/\` (request logic shared with the older React app), \`shared/pm-shim.js\`,
\`site/src/\` (theme and logo), npm packages pinned in \`webapp/package-lock.json\`, and the Rust
crates \`cli/crates/hitop-wasm\` and \`cli/crates/hitop-scripts-wasm\` (pinned in their Cargo.lock)
built by \`webapp/scripts/build-wasm.mjs\` into \`webapp/src/wasm/pkg*/\`. The Vite settings are in
\`webapp/vite.config.js\` (mode \`extension\`) and \`webapp/.env.extension\`.
`;

const tmp = mkdtempSync(join(tmpdir(), "hitop-ext-source-"));
try {
  writeFileSync(join(tmp, "BUILD.md"), readme);
  rmSync(resolve(out), { force: true });
  execFileSync("git", ["archive", "--format=zip", `--add-file=${join(tmp, "BUILD.md")}`, "-o", resolve(out), "HEAD", "--", ...PATHS], {
    cwd: root,
    stdio: "inherit",
  });
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
console.log(`source package ${out} (commit ${commit}, extension ${extVersion})`);
