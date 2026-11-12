// Unit checks for UX2 (no browser): node tests/unit-ux2.mjs
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { explainError } from "../src/errors.js";
import { isCurl, parseCurl, shellWords } from "../src/curl.js";
import { bodyView, digest, fromBase64, hexDump, isBinaryType, toBase64 } from "../src/binary.js";
import { compareResponses, snapshotResponse } from "../src/diff.js";

const here = path.dirname(fileURLToPath(import.meta.url));

// --- (7) engine and proxy errors: a headline and a next step, the raw text kept ---
{
  const proxy = (cause) => `Error: couldn't reach the server: ${cause}`;
  const dns = explainError(proxy("dns error: failed to lookup address information: Name or service not known"));
  assert.equal(dns.headline, "Host not found");
  assert.equal(dns.known, true);
  assert.match(dns.next, /host name/);
  assert.equal(dns.raw, "couldn't reach the server: dns error: failed to lookup address information: Name or service not known", "the Error: prefix goes, the rest stays");
  assert.equal(explainError("error sending request for url (http://localhost:9/): tcp connect error: Connection refused (os error 111)").headline, "Connection refused");
  assert.equal(explainError("connect ECONNREFUSED 127.0.0.1:9").headline, "Connection refused");
  assert.equal(explainError(proxy("invalid peer certificate: UnknownIssuer")).headline, "The TLS (HTTPS) handshake failed");
  assert.equal(explainError(proxy("the server didn't answer within 20 seconds")).headline, "No answer in time");
  assert.equal(explainError("Error: Connection reset by peer (os error 104)").headline, "The server closed the connection");
  assert.equal(explainError("example.internal resolves only to private or reserved addresses").headline, "The proxy won't call that address");
  // gRPC: TLS spoken to a plaintext server.
  const grpc = explainError("status: Unknown, message: \"transport error: received corrupt message of type InvalidContentType\"", { kind: "grpc" });
  assert.equal(grpc.headline, "The server isn't speaking gRPC over TLS");
  assert.match(grpc.next, /http:\/\/host:port/);
  assert.equal(explainError("received corrupt message of type InvalidContentType").headline, "The server didn't answer over HTTPS", "HTTP words for an HTTP request");
  // The app's own texts already say what to do: they are the next step.
  const cors = explainError("Error: Couldn't reach the server. Requests to localhost and private networks go straight from your browser, so the server must allow CORS from this site; the desktop and mobile apps don't need it.");
  assert.equal(cors.headline, "The browser couldn't reach the server");
  assert.match(cors.next, /^Requests to localhost/);
  const limit = explainError("Error: No response within 1 s. The time limit is in Settings, under Requests.");
  assert.equal(limit.headline, "No response in time");
  assert.match(limit.next, /No response within 1 s/);
  // Anything else: said as it is.
  const other = explainError("Error: something odd");
  assert.deepEqual(other, { headline: "The request failed", next: "", raw: "something odd", known: false });
  assert.equal(explainError(null).raw, "");
}

// --- (10) lit-material-empty-state's slot is `actions` (alert's is `action`): no `slot="action"`
// inside an empty state anywhere in the app's views ---
{
  const dir = path.join(here, "../src");
  const files = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      if (e.isDirectory() && e.name !== "wasm") walk(path.join(d, e.name));
      else if (e.name.endsWith(".js")) files.push(path.join(d, e.name));
    }
  };
  walk(dir);
  const bad = [];
  for (const f of files) {
    const text = fs.readFileSync(f, "utf8");
    for (const m of text.matchAll(/<lit-material-empty-state\b[\s\S]*?<\/lit-material-empty-state>/g)) {
      if (/slot="action"/.test(m[0])) bad.push(`${path.relative(dir, f)}: ${m[0].slice(0, 80).replace(/\s+/g, " ")}…`);
    }
  }
  assert.deepEqual(bad, [], `empty states with slot="action" (use "actions"):\n${bad.join("\n")}`);
}

// --- (2) curl commands as requests ---
{
  assert.equal(isCurl("curl https://x"), true);
  assert.equal(isCurl("  $ curl -sS https://x"), true);
  assert.equal(isCurl("curl.exe https://x"), true);
  assert.equal(isCurl("curling.example.com"), false);
  assert.equal(isCurl("https://x"), false);
  assert.deepEqual(shellWords(`a 'b c' "d \\"e\\" \\$f" g\\ h $'i\\nj' ''`), ["a", "b c", 'd "e" $f', "g h", "i\nj", ""]);
  assert.deepEqual(shellWords("a \\\n  b ^\n c `\n d"), ["a", "b", "c", "d"], "bash, cmd and PowerShell line continuations");
  assert.throws(() => shellWords("a 'b"), /isn't closed/);

  const simple = parseCurl("curl https://httpbin.org/get");
  assert.deepEqual(simple, { method: "GET", url: "https://httpbin.org/get", headers: [], body: "", bodyMode: "raw", formFields: [], auth: null, warnings: [] });
  // The audit's paste: -X, -H and -d, quotes, a $ prompt and continuations.
  const post = parseCurl(`$ curl -X POST 'https://httpbin.org/post' \\\n  -H 'Content-Type: application/json' \\\n  -H "Authorization: Bearer abc" \\\n  -d '{"name": "Ada"}'`);
  assert.equal(post.method, "POST");
  assert.equal(post.url, "https://httpbin.org/post");
  assert.deepEqual(post.headers, [{ name: "Content-Type", value: "application/json" }, { name: "Authorization", value: "Bearer abc" }]);
  assert.equal(post.body, '{"name": "Ada"}');
  // -d without -X is a POST; without a Content-Type a non-JSON body is said to be a form, as curl sends it.
  const form = parseCurl("curl https://x.test/login -d user=ada -d 'pass=a b'");
  assert.equal(form.method, "POST");
  assert.equal(form.body, "user=ada&pass=a b");
  assert.deepEqual(form.headers, [{ name: "Content-Type", value: "application/x-www-form-urlencoded" }]);
  assert.deepEqual(parseCurl("curl https://x.test -d '{\"a\":1}'").headers, [], "JSON: the app's own Content-Type rule decides");
  assert.equal(parseCurl("curl -G https://x.test/s -d q=1 --data-urlencode 'name=a b'").url, "https://x.test/s?q=1&name=a%20b", "-G puts the data in the query");
  assert.equal(parseCurl("curl -G https://x.test/s -d q=1").method, "GET");
  assert.equal(parseCurl("curl -I https://x.test").method, "HEAD");
  assert.equal(parseCurl("curl -XDELETE https://x.test/1").method, "DELETE", "a value glued to -X");
  assert.equal(parseCurl("curl -sSL --request=patch https://x.test/1").method, "PATCH", "--opt=value and a flag cluster");
  assert.equal(parseCurl("curl -sSX PUT https://x.test/1").method, "PUT", "a cluster whose last flag takes the next word");
  assert.deepEqual(parseCurl("curl --url https://x.test/a -u ada:p:w").auth, { type: "basic", username: "ada", password: "p:w" });
  assert.deepEqual(parseCurl("curl https://x.test -A hitop/1 -e https://ref -b 'a=1; b=2'").headers, [
    { name: "User-Agent", value: "hitop/1" },
    { name: "Referer", value: "https://ref" },
    { name: "Cookie", value: "a=1; b=2" },
  ]);
  const j = parseCurl(`curl https://x.test/j --json '{"a": 1}'`);
  assert.equal(j.method, "POST");
  assert.deepEqual(j.headers, [{ name: "Content-Type", value: "application/json" }, { name: "Accept", value: "application/json" }]);
  const multi = parseCurl(`curl https://x.test/up -F 'title=Hello' -F 'avatar=@"me.png";type=image/png' --form-string 'raw=@x'`);
  assert.equal(multi.method, "POST");
  assert.equal(multi.bodyMode, "multipart");
  assert.deepEqual(multi.formFields.map(({ key, value, type, fileName }) => [key, value, type, fileName]), [["title", "Hello", "text", undefined], ["avatar", "", "file", "me.png"], ["raw", "@x", "text", undefined]]);
  assert.equal(parseCurl("curl https://x.test/{{id}} -H 'X-Key: {{ apiKey }}'").url, "https://x.test/${id}", "{{name}} becomes ${name}");
  assert.equal(parseCurl("curl https://x.test -H 'X-Key: {{ apiKey }}'").headers[0].value, "${apiKey}");
  assert.deepEqual(parseCurl("curl -o out.json --max-time 5 -k https://x.test --frobnicate").warnings, ["Left out: --frobnicate."], "known options skipped quietly, unknown ones named");
  assert.equal(parseCurl("curl https://a.test && echo done").url, "https://a.test", "the first command only");
  assert.throws(() => parseCurl("curl -X POST"), /no URL/);
  assert.throws(() => parseCurl("wget https://x"), /isn't a curl command/);
  assert.throws(() => parseCurl("curl -H"), /needs a value/);
  assert.equal(parseCurl("curl -d @body.json https://x.test").warnings[0], "The body comes from the file body.json: paste its contents into Body.");

  // Every curl.sh hitop-core's snippet goldens write reads back as the request it came from.
  const goldens = path.join(here, "../../cli/crates/hitop-core/tests/golden/snippets");
  const cases = JSON.parse(fs.readFileSync(path.join(goldens, "cases.json"), "utf8"));
  const by = Object.fromEntries(cases.map((c) => [c.name, c]));
  const read = (name) => parseCurl(fs.readFileSync(path.join(goldens, name, "curl.sh"), "utf8"));
  const getq = read("get-query-bearer");
  assert.equal(getq.method, "GET");
  assert.equal(getq.url, "https://api.example.test/users?page=2&q=a%20b");
  assert.deepEqual(getq.headers, [{ name: "Accept", value: "application/json" }, { name: "Authorization", value: "Bearer tok-123" }]);
  const pj = read("post-json-basic");
  assert.equal(pj.body, by["post-json-basic"].request.body, "quotes, backslashes, $ and ` survive the round trip");
  assert.deepEqual(pj.auth, { type: "basic", username: "ada", password: `p'w"d` });
  const gql = read("graphql-query");
  assert.equal(JSON.parse(gql.body).operationName, "User");
  assert.ok(gql.headers.some((h) => h.value === "Bearer ${token}"), "{{token}} placeholders come back as ${token}");
  const up = read("multipart-apikey-header");
  assert.deepEqual(up.formFields.map(({ key, value, type, fileName }) => [key, value, type, fileName]), [["title", "@not-a-file", "text", undefined], ["avatar", "", "file", "photo.png"]]);
  assert.equal(read("custom-method").method, "PURGE");
  assert.deepEqual(read("custom-method").auth, { type: "basic", username: "svc", password: "" });
  assert.equal(read("urlencoded-apikey-query").body, "name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9");
  assert.equal(read("urlencoded-apikey-query").method, "PUT");
  for (const name of fs.readdirSync(goldens).filter((n) => fs.existsSync(path.join(goldens, n, "curl.sh")))) assert.doesNotThrow(() => read(name), name);
}

// --- (6) binary bodies: kind by Content-Type, base64 both ways, hex, digest; Diff by size + digest ---
{
  assert.equal(isBinaryType("image/png"), true);
  assert.equal(isBinaryType("application/pdf; charset=binary"), true);
  assert.equal(isBinaryType("application/octet-stream"), true);
  assert.equal(isBinaryType("image/svg+xml"), false, "SVG is text");
  assert.equal(isBinaryType("application/json"), false);
  assert.equal(isBinaryType("text/plain"), false);
  assert.equal(isBinaryType(""), false);
  assert.equal(bodyView("image/webp"), "image");
  assert.equal(bodyView("application/pdf"), "pdf");
  assert.equal(bodyView("application/zip"), "binary");
  assert.equal(bodyView("text/html"), null);
  const bytes = Uint8Array.from([0, 1, 2, 255, 65, 66, 67]);
  assert.deepEqual([...fromBase64(toBase64(bytes))], [...bytes]);
  assert.equal(fromBase64("%%%").length, 0, "not base64: no bytes");
  assert.equal(hexDump(bytes), `00000000  ${"00 01 02 ff 41 42 43".padEnd(23)}  ${"".padEnd(23)}  |....ABC|`);
  assert.equal(hexDump(new Uint8Array(40), 20).split("\n").length, 2, "only the first `max` bytes");
  assert.equal(digest(bytes), digest(Uint8Array.from(bytes)));
  assert.notEqual(digest(bytes), digest(Uint8Array.from([0, 1, 2, 255, 65, 66, 68])));
  const png = { "content-type": "image/png" };
  const a = snapshotResponse({ status: 200, headers: png, body: toBase64(bytes) });
  assert.deepEqual(a.binary, { size: 7, digest: digest(bytes) });
  assert.equal(a.body, "");
  const same = compareResponses(a, snapshotResponse({ status: 200, headers: png, body: toBase64(bytes) }));
  assert.equal(same.same, true);
  const other = compareResponses(a, snapshotResponse({ status: 200, headers: png, body: toBase64(Uint8Array.from([9])) }));
  assert.equal(other.body.kind, "binary");
  assert.equal(other.body.same, false);
  assert.deepEqual([other.body.before.size, other.body.after.size], [7, 1]);
}

console.log("unit-ux2: all checks passed");
