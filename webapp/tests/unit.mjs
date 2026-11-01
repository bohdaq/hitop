// Unit checks for features.js (no browser): node tests/unit.mjs
import assert from "node:assert/strict";
import { cookieHeader, exportableEnvironments, maskSecrets, mergeCookies, multipartBody, parseEnvironments, parseQuery, secretNames, secretValues, urlEncodedBody, withQuery } from "../src/features.js";

const now = Date.parse("2026-10-31T12:00:00Z");
let jar = mergeCookies([], ["sid=abc; Path=/; HttpOnly", "pref=dark; Domain=example.com; Path=/; Max-Age=3600", "evil=1; Domain=other.com", "old=x; Expires=Thu, 01 Jan 2026 00:00:00 GMT"], "https://api.example.com/v1/login", now);
assert.deepEqual(jar.map((c) => c.name).sort(), ["pref", "sid"]);
assert.equal(cookieHeader(jar, "https://api.example.com/v1/me", now), "sid=abc; pref=dark");
assert.equal(mergeCookies([], ["a=1"], "https://x.dev/v1/login", now)[0].path, "/v1", "default path is the request directory");
assert.equal(cookieHeader(jar, "https://www.example.com/", now), "pref=dark", "domain cookie reaches subdomains, host-only doesn't");
assert.equal(cookieHeader(jar, "https://example.org/", now), "");
jar = mergeCookies(jar, ["sid=; Path=/; Max-Age=0"], "https://api.example.com/v1/logout", now);
assert.equal(cookieHeader(jar, "https://api.example.com/", now), "pref=dark", "Max-Age=0 deletes");
assert.equal(cookieHeader(jar, "https://www.example.com/", now + 3601_000), "", "expired cookies aren't sent");

const files = new Map([["f1", new Blob(["hello"], { type: "text/plain" })]]);
files.get("f1").name = "hi.txt";
const { body, contentType } = await multipartBody([{ id: "t", key: "title", value: "A", type: "text" }, { id: "f1", key: "upload", type: "file" }, { id: "x", key: "off", value: "1", type: "text", enabled: false }], files);
const text = new TextDecoder().decode(body);
const boundary = contentType.split("boundary=")[1];
assert.ok(text.startsWith(`--${boundary}\r\nContent-Disposition: form-data; name="title"\r\n\r\nA\r\n`));
assert.ok(text.includes('name="upload"; filename="hi.txt"\r\nContent-Type: text/plain\r\n\r\nhello\r\n'));
assert.ok(!text.includes("off"));
assert.ok(text.endsWith(`--${boundary}--\r\n`));

assert.deepEqual(urlEncodedBody([{ key: "q", value: "a b&c" }, { key: "n", value: "1", enabled: false }]), { body: "q=a%20b%26c", contentType: "application/x-www-form-urlencoded" });
assert.equal(withQuery("https://x/?a=1", parseQuery("https://x/?a=1")), "https://x/?a=1");
// Secrets: values stay in localStorage, never in an export or a masked text.
const legacy = { id: "e1", name: "Prod", variables: { base: "https://x" } };
assert.deepEqual(secretNames(legacy), [], "environments stored before secrets have none");
assert.deepEqual(exportableEnvironments([legacy]), [{ name: "Prod", variables: { base: "https://x" } }], "and export as before");
const prod = { id: "e2", name: "Prod", variables: { base: "https://x", token: "s3cr3t t0ken" }, secrets: ["token"] };
assert.deepEqual(secretValues(prod), ["s3cr3t t0ken"]);
const exported = JSON.stringify(exportableEnvironments([prod]));
assert.ok(!exported.includes("s3cr3t"), exported);
assert.deepEqual(JSON.parse(exported), [{ name: "Prod", variables: { base: "https://x" }, secrets: ["token"] }]);
const back = parseEnvironments(exported)[0];
assert.deepEqual([back.secrets, back.variables], [["token"], { base: "https://x", token: "" }], "import keeps the name, with no value");
const masked = maskSecrets({ url: "https://x/?t=s3cr3t%20t0ken", headers: [{ name: "Authorization", value: "Bearer s3cr3t t0ken" }], statusCode: 200 }, secretValues(prod));
assert.deepEqual(masked, { url: "https://x/?t=••••", headers: [{ name: "Authorization", value: "Bearer ••••" }], statusCode: 200 });
assert.equal(maskSecrets("as is", []), "as is");
console.log("features.js: all unit checks passed");

// --- T5: hitop-core's format in WASM, zip files, the desktop's storage shapes ---
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { formatFromBytes } from "../src/wasm/format.js";
import { historyFromRust, historyToRust } from "../src/platform/history.js";

const here = dirname(fileURLToPath(import.meta.url));
const format = await formatFromBytes(readFileSync(join(here, "../src/wasm/pkg/hitop_wasm_bg.wasm")));
const tree = (dir) => {
  const out = {};
  const walk = (d) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      if (statSync(p).isDirectory()) walk(p);
      else out[relative(dir, p).split("\\").join("/")] = readFileSync(p, "utf8");
    }
  };
  walk(dir);
  return out;
};
// The shared fixtures: hitop-core's golden folder must come out byte for byte.
const golden = join(here, "../../cli/crates/hitop-core/tests/golden/every-field");
const model = {
  collection: JSON.parse(readFileSync(join(golden, "collection.json"), "utf8")),
  environments: JSON.parse(readFileSync(join(golden, "environments.json"), "utf8")),
  globals: JSON.parse(readFileSync(join(golden, "globals.json"), "utf8")),
};
const goldenFiles = tree(join(golden, "folder"));
assert.deepEqual(format.toFiles(model), goldenFiles, "the browser writes the golden folder exactly as the CLI does");
const readBack = format.readFiles(goldenFiles, "folder");
assert.deepEqual(format.toFiles(readBack), goldenFiles, "and reads it back to the same files");
assert.throws(() => format.readFiles({ "hitop.yaml": "format: 2\n" }, "x"), /newer hitop/);

// Old web app data with string ids converts (the T1 open item).
const legacyCols = JSON.parse(JSON.parse(readFileSync(join(here, "legacy-storage.json"), "utf8")).hitop_collections);
const legacyFiles = format.toFiles({ collection: legacyCols[1], environments: [] });
assert.ok(legacyFiles["requests/create-post.yaml"].includes("url: ${base-url}/posts"), Object.keys(legacyFiles).join());

// Zip: export, import, and a zip of the folder's contents only.
const { zipToFiles } = await import("../src/zip.js");
const { zipSync, strToU8 } = await import("fflate");
{
  const { files, name } = zipToFiles(zipSync({ "hitop.yaml": strToU8("format: 1\n"), "requests/a.yaml": strToU8("url: https://a\n"), "__MACOSX/._a.yaml": strToU8("x") }), "My API.zip");
  assert.equal(name, "My API");
  assert.deepEqual(Object.keys(files).sort(), ["hitop.yaml", "requests/a.yaml"]);
  const nested = zipToFiles(zipSync({ "outer/my-api/hitop.yaml": strToU8("format: 1\n"), "outer/my-api/requests/b.yml": strToU8("url: x\n"), "outer/other.txt": strToU8("x") }));
  assert.equal(nested.name, "my-api");
  assert.deepEqual(Object.keys(nested.files).sort(), ["hitop.yaml", "requests/b.yml"]);
  assert.throws(() => zipToFiles(zipSync({ "a.txt": strToU8("x") })), /no hitop.yaml/);
  assert.throws(() => zipToFiles(new Uint8Array([1, 2, 3])), /isn't a zip/);
}

// The desktop's history.json: Rust's HistoryItem ⇄ the web app's items.
const webItems = JSON.parse(JSON.parse(readFileSync(join(here, "legacy-storage.json"), "utf8")).hitop_history);
const rust = historyToRust([...webItems, { id: 5, timestamp: "2026-11-01T00:00:00.000Z", url: "u", method: "POST", statusCode: 201, success: true, collectionName: "C", requestName: "R", isCollectionRun: true, headers: [], body: "b" }]);
assert.deepEqual(format.normalize("history", rust), rust, "Rust reads and keeps every field");
const backItems = historyFromRust(rust);
assert.deepEqual(backItems[0], { ...webItems[0] }, "a web history item comes back as it was");
assert.equal(backItems[1].collectionName, "C");
const cli = historyFromRust(JSON.parse(readFileSync(join(here, "desktop-storage/history.json"), "utf8")));
assert.equal(cli[0].statusCode, 200);
assert.equal(cli[0].timestamp, new Date(1793470570537).toISOString(), "CLI history (ms timestamps) shows");

// The old desktop's files read through Rust's model unchanged.
for (const [kind, file] of [["collections", "collections.json"], ["history", "history.json"], ["environments", "environments.json"]]) {
  const json = JSON.parse(readFileSync(join(here, "desktop-storage", file), "utf8"));
  assert.deepEqual(format.normalize(kind, json), json, `${file} reads and writes unchanged`);
}
console.log("unit checks passed");
