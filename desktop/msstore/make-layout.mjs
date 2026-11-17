// Stages the MSIX package layout for the Microsoft Store build (release-msstore.yml): the store exe,
// AppxManifest.xml rendered from this folder's template, and the Assets images (brand/export.sh renders
// them from brand/logo.svg). Everything except makepri/makeappx/signtool, so it runs and is tested on Linux.
//
//   node desktop/msstore/make-layout.mjs --exe <hitop exe> --out <layout dir> [--arch x64]
//   node desktop/msstore/make-layout.mjs --version          the 4-part package version, nothing written
//
// Identity: the Partner Center name reservation, from env MSSTORE_IDENTITY_NAME, MSSTORE_PUBLISHER
// (CN=...) and MSSTORE_PUBLISHER_DISPLAY_NAME (repository variables). All three or none: without them
// the package gets a placeholder identity for a self-signed, install-for-testing-only package.
// Prints a JSON summary; on GitHub Actions also writes it to $GITHUB_OUTPUT.
import { appendFileSync, copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const conf = JSON.parse(readFileSync(join(here, "../src-tauri/tauri.conf.json"), "utf8"));

// The test package's identity. Its publisher is also the subject of the throwaway certificate the
// workflow makes, which must equal the manifest's Publisher for the signature to be accepted.
export const TEST_IDENTITY = {
  name: "tech.rws8.hitop.test",
  publisher: "CN=HITOP Test Package",
  publisherDisplayName: "HITOP test package",
};

// tauri.conf.json "1.3.0" → "1.3.0.0". The Store reserves the fourth part (it must be 0), and each part
// is a 16-bit number. A pre-release suffix (1.4.0-rc.1) is dropped: the package version is the release's.
export function packageVersion(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$/.exec(version);
  if (!m) throw new Error(`tauri.conf.json version "${version}" is not major.minor.patch`);
  const parts = m.slice(1, 4).map(Number);
  if (parts.some((n) => n > 65535)) throw new Error(`version "${version}": each part must be at most 65535`);
  if (parts[0] === 0) throw new Error(`version "${version}": the Store needs a major version above 0`);
  return `${parts.join(".")}.0`;
}

export function xmlEscape(text) {
  return String(text).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;" })[c]);
}

// The store identity from the environment, or the test identity when none of it is set.
export function identity(env) {
  const name = (env.MSSTORE_IDENTITY_NAME || "").trim();
  const publisher = (env.MSSTORE_PUBLISHER || "").trim();
  const publisherDisplayName = (env.MSSTORE_PUBLISHER_DISPLAY_NAME || "").trim();
  if (/[\r\n]/.test(name + publisher + publisherDisplayName)) throw new Error("MSSTORE_* values must be one line each");
  const set = [name, publisher, publisherDisplayName].filter(Boolean).length;
  if (set === 0) return { ...TEST_IDENTITY, test: true };
  if (set !== 3) throw new Error("set all three of MSSTORE_IDENTITY_NAME, MSSTORE_PUBLISHER and MSSTORE_PUBLISHER_DISPLAY_NAME, or none (test package)");
  // Package/Identity/@Name: 3-50 characters, letters, digits, dots and dashes (appx manifest schema).
  if (!/^[A-Za-z0-9.-]{3,50}$/.test(name)) throw new Error(`MSSTORE_IDENTITY_NAME "${name}" is not a package name (3-50 of A-Z a-z 0-9 . -)`);
  if (!/^CN=/.test(publisher)) throw new Error(`MSSTORE_PUBLISHER must be the full "CN=..." value from Partner Center, got "${publisher}"`);
  if (publisher === TEST_IDENTITY.publisher) throw new Error("MSSTORE_PUBLISHER is the test publisher");
  return { name, publisher, publisherDisplayName, test: false };
}

// The Visual C++ runtime DLLs an exe imports (their names sit in its import table as ASCII).
export function vcRuntimeImports(bytes) {
  const text = bytes.toString("latin1").toLowerCase();
  return ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"].filter((dll) => text.includes(dll));
}

const VCLIBS = `
    <PackageDependency Name="Microsoft.VCLibs.140.00.UWPDesktop" MinVersion="14.0.30704.0" Publisher="CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US" />`;

export function renderManifest(template, values) {
  const out = template.replace(/\{\{([A-Z_]+)\}\}/g, (_, key) => {
    if (!(key in values)) throw new Error(`AppxManifest.xml: no value for {{${key}}}`);
    return values[key];
  });
  if (out.includes("{{")) throw new Error("AppxManifest.xml: a placeholder is left");
  return out;
}

export function manifestValues({ id, version, arch, vclibs }) {
  const description = conf.bundle.shortDescription;
  return {
    IDENTITY_NAME: xmlEscape(id.name),
    PUBLISHER: xmlEscape(id.publisher),
    PUBLISHER_DISPLAY_NAME: xmlEscape(id.publisherDisplayName),
    VERSION: version,
    ARCH: arch,
    // The store package's DisplayName must be the name reserved in Partner Center: HITOP.
    DISPLAY_NAME: xmlEscape(id.test ? "HITOP (test package, not for the Store)" : conf.productName),
    APP_DISPLAY_NAME: xmlEscape(id.test ? "HITOP (test)" : conf.productName),
    DESCRIPTION: xmlEscape(description),
    // Only when the exe needs the Visual C++ runtime (Tauri links it statically, so normally not): the Store
    // installs the framework package with the app.
    PACKAGE_DEPENDENCIES: vclibs ? VCLIBS : "",
  };
}

function args(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) throw new Error(`unexpected argument ${a}`);
    const key = a.slice(2);
    if (key === "version") out.version = true;
    else out[key] = argv[++i];
  }
  return out;
}

function main() {
  const opts = args(process.argv.slice(2));
  const version = packageVersion(conf.version);
  if (opts.version) {
    console.log(version);
    return;
  }
  if (!opts.exe || !opts.out) throw new Error("usage: make-layout.mjs --exe <hitop exe> --out <layout dir> [--arch x64]");
  const arch = opts.arch || "x64";
  if (!["x64", "arm64"].includes(arch)) throw new Error(`--arch ${arch}: x64 or arm64`);
  const id = identity(process.env);
  const exe = readFileSync(opts.exe);
  if (exe.subarray(0, 2).toString("latin1") !== "MZ") throw new Error(`${opts.exe} is not a Windows executable`);
  const vc = vcRuntimeImports(exe);
  if (vc.length) console.warn(`::warning::${basename(opts.exe)} imports ${vc.join(", ")}: the package depends on Microsoft.VCLibs.140.00.UWPDesktop`);

  const out = opts.out;
  if (existsSync(out) && readdirSync(out).length) throw new Error(`${out} is not empty`);
  mkdirSync(join(out, "Assets"), { recursive: true });
  copyFileSync(opts.exe, join(out, "hitop.exe"));
  // Tauri links the WebView2 loader statically; copy a loader DLL if this build left one beside the exe.
  const loader = join(dirname(opts.exe), "WebView2Loader.dll");
  if (existsSync(loader)) copyFileSync(loader, join(out, "WebView2Loader.dll"));
  const assets = join(here, "Assets");
  for (const f of readdirSync(assets)) copyFileSync(join(assets, f), join(out, "Assets", f));
  const template = readFileSync(join(here, "AppxManifest.xml"), "utf8");
  writeFileSync(join(out, "AppxManifest.xml"), renderManifest(template, manifestValues({ id, version, arch, vclibs: vc.length > 0 })));

  // File names say what the package is: the store one is for Partner Center only (unsigned; the Store
  // signs it), the test one is self-signed and must never be uploaded or offered to people.
  const file = id.test ? `hitop-${version}-${arch}-TEST-ONLY.msix` : `hitop-${version}-${arch}-store.msix`;
  const summary = { version, arch, mode: id.test ? "test" : "store", name: id.name, publisher: id.publisher, file, vclibs: vc.length > 0 };
  console.log(JSON.stringify(summary, null, 2));
  if (process.env.GITHUB_OUTPUT) {
    // Values are single-line by construction (checked above); the publisher may hold commas and quotes.
    for (const [k, v] of Object.entries(summary)) {
      if (String(v).includes("\n")) throw new Error(`${k} has a line break`);
      appendFileSync(process.env.GITHUB_OUTPUT, `${k}=${v}\n`);
    }
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  try {
    main();
  } catch (e) {
    console.error(`::error::${e.message}`);
    process.exit(1);
  }
}
