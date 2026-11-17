// node --test desktop/msstore/make-layout.test.mjs (CI job desktop): version, identity, escaping and the
// rendered manifest of make-layout.mjs, run on a fake exe in both identity modes. HITOP_MSIX_KEEP=<prefix>
// also writes the two rendered manifests (<prefix>.test.xml, <prefix>.store.xml) for xmllint.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { identity, packageVersion, TEST_IDENTITY, vcRuntimeImports, xmlEscape } from "./make-layout.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, "make-layout.mjs");
const STORE = {
  MSSTORE_IDENTITY_NAME: "12345Example.HITOP",
  MSSTORE_PUBLISHER: "CN=1A2B3C4D-5E6F-7A8B-9C0D-1E2F3A4B5C6D",
  MSSTORE_PUBLISHER_DISPLAY_NAME: 'Example "B" & Co <dev>',
};

test("version: three parts become four, the fourth is 0", () => {
  assert.equal(packageVersion("1.3.0"), "1.3.0.0");
  assert.equal(packageVersion("1.4.0-rc.1"), "1.4.0.0");
  assert.equal(packageVersion("12.0.65535"), "12.0.65535.0");
  assert.throws(() => packageVersion("1.3"));
  assert.throws(() => packageVersion("1.65536.0"));
  assert.throws(() => packageVersion("0.9.0"));
});

test("identity: all three variables or none", () => {
  assert.deepEqual(identity({}), { ...TEST_IDENTITY, test: true });
  assert.equal(identity(STORE).test, false);
  assert.throws(() => identity({ MSSTORE_IDENTITY_NAME: "12345Example.HITOP" }), /all three/);
  assert.throws(() => identity({ ...STORE, MSSTORE_PUBLISHER: "1A2B" }), /CN=/);
  assert.throws(() => identity({ ...STORE, MSSTORE_IDENTITY_NAME: "has space" }), /package name/);
  assert.throws(() => identity({ ...STORE, MSSTORE_PUBLISHER_DISPLAY_NAME: "a\nb" }), /one line/);
});

test("escaping and the VC runtime check", () => {
  assert.equal(xmlEscape(`a&b<c>"d'`), "a&amp;b&lt;c&gt;&quot;d&apos;");
  assert.deepEqual(vcRuntimeImports(Buffer.from("MZ...KERNEL32.dll...")), []);
  assert.deepEqual(vcRuntimeImports(Buffer.from("MZ...VCRUNTIME140.dll...")), ["vcruntime140.dll"]);
});

function layout(env, exeBytes) {
  const dir = mkdtempSync(join(tmpdir(), "hitop-msix-"));
  const exe = join(dir, "hitop-desktop.exe");
  writeFileSync(exe, exeBytes);
  const out = join(dir, "layout");
  try {
    const stdout = execFileSync(process.execPath, [script, "--exe", exe, "--out", out], {
      env: { PATH: process.env.PATH, ...env },
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    return {
      summary: JSON.parse(stdout),
      manifest: readFileSync(join(out, "AppxManifest.xml"), "utf8"),
      files: readdirSync(out),
      assets: readdirSync(join(out, "Assets")),
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("test package layout", () => {
  const r = layout({}, Buffer.from("MZ fake exe"));
  assert.equal(r.summary.mode, "test");
  assert.match(r.summary.file, /^hitop-\d+\.\d+\.\d+\.0-x64-TEST-ONLY\.msix$/);
  assert.deepEqual(r.files.sort(), ["AppxManifest.xml", "Assets", "hitop.exe"]);
  assert.ok(r.assets.includes("Square44x44Logo.targetsize-256_altform-unplated.png"));
  assert.match(r.manifest, /Publisher="CN=HITOP Test Package"/);
  assert.match(r.manifest, /<DisplayName>HITOP \(test package, not for the Store\)<\/DisplayName>/);
  assert.doesNotMatch(r.manifest, /\{\{|VCLibs/);
  if (process.env.HITOP_MSIX_KEEP) writeFileSync(`${process.env.HITOP_MSIX_KEEP}.test.xml`, r.manifest);
});

test("store package layout: identity escaped, VCLibs only when imported", () => {
  const r = layout(STORE, Buffer.from("MZ fake exe VCRUNTIME140.dll"));
  assert.equal(r.summary.mode, "store");
  assert.equal(r.summary.vclibs, true);
  assert.match(r.summary.file, /-x64-store\.msix$/);
  assert.match(r.manifest, /Name="12345Example\.HITOP"/);
  assert.match(r.manifest, /<PublisherDisplayName>Example &quot;B&quot; &amp; Co &lt;dev&gt;<\/PublisherDisplayName>/);
  assert.match(r.manifest, /<DisplayName>HITOP<\/DisplayName>/);
  assert.match(r.manifest, /Microsoft\.VCLibs\.140\.00\.UWPDesktop/);
  if (process.env.HITOP_MSIX_KEEP) writeFileSync(`${process.env.HITOP_MSIX_KEEP}.store.xml`, r.manifest);
});

test("refuses a file that isn't an exe", () => {
  assert.throws(() => layout({}, Buffer.from("not an exe")));
});
