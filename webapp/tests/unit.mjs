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
