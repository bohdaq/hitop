// Unit checks for UX2 (no browser): node tests/unit-ux2.mjs
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { explainError } from "../src/errors.js";

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

console.log("unit-ux2: all checks passed");
