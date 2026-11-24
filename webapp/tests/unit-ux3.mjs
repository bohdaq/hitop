// Unit checks for the 2026-11-15 UX review fixes (no browser): node tests/unit-ux3.mjs
import assert from "node:assert/strict";
import { applyPathVariables, cleanSettings, inheritDefaults, inheritedAuth, pathVariableNames, requestTimeout } from "../src/defaults.js";
import { jsonPath } from "../src/json-path.js";
import { formatBody, prettyJson, prettyXml } from "../src/format-body.js";
import { findDamaged } from "../src/platform/local-storage.js";
import { responseCookies, rowsToText, textToRows } from "../src/rows.js";

// Inherited auth and headers: the nearest folder, then the collection; a request's none wins.
{
  const c = {
    auth: { type: "bearer", token: "col" },
    headers: [{ name: "X-App", value: "1" }, { name: "Accept", value: "*/*" }],
    folderDefaults: { orders: { auth: { type: "basic", username: "u", password: "p" }, headers: [{ name: "X-App", value: "2" }] } },
  };
  const top = inheritDefaults({ url: "u", headers: [], auth: { type: "inherit" } }, c);
  assert.equal(top.auth.type, "bearer");
  assert.deepEqual(top.headers.map((h) => [h.name, h.value]), [["X-App", "1"], ["Accept", "*/*"]]);
  const deep = inheritDefaults({ url: "u", file: "orders/new/b", headers: [{ name: "accept", value: "json" }] }, c);
  assert.equal(deep.auth.type, "basic", "a parent folder's auth");
  assert.deepEqual(deep.headers.map((h) => [h.name, h.value]), [["X-App", "2"], ["accept", "json"]], "own headers replace inherited ones by name");
  assert.equal(inheritDefaults({ url: "u", headers: [], auth: { type: "none" } }, c).auth.type, "none", "No auth wins");
  assert.deepEqual(inheritedAuth({ file: "orders/x" }, c), { auth: c.folderDefaults.orders.auth, from: "folder orders" });
  assert.equal(inheritedAuth({}, {}), null);
  const again = inheritDefaults(deep, c);
  assert.deepEqual(again.headers, deep.headers, "applying it twice changes nothing");
}

// Path variables and per-request settings.
{
  assert.deepEqual(pathVariableNames("https://x.dev/users/:id/posts/:postId?x=:no"), ["id", "postId"]);
  assert.equal(applyPathVariables("https://x.dev/users/:id/posts/:pid?q=:id", [{ key: "id", value: "a b" }]), "https://x.dev/users/a%20b/posts/:pid?q=:id");
  assert.equal(applyPathVariables("https://x.dev/u/:id", [{ key: "id", value: "${n}" }], (t) => t.replace("${n}", "7")), "https://x.dev/u/7");
  assert.equal(requestTimeout({ settings: { timeoutMs: 500 } }, { timeoutMs: 30000 }), 500);
  assert.equal(requestTimeout({}, { timeoutMs: 30000 }), 30000);
  assert.deepEqual(cleanSettings({ followRedirects: true, timeoutMs: 0 }), undefined, "defaults aren't saved");
  assert.deepEqual(cleanSettings({ followRedirects: false, timeoutMs: 1500.4 }), { followRedirects: false, timeoutMs: 1500 });
}

// JSONPath filter.
{
  const d = { data: { items: [{ id: 1, name: "a" }, { id: 2, name: "b", tags: ["x"] }] }, id: 9 };
  assert.deepEqual(jsonPath(d, "$.data.items[*].id"), [1, 2]);
  assert.deepEqual(jsonPath(d, "$..id"), [9, 1, 2]);
  assert.deepEqual(jsonPath(d, "data.items[-1].name"), ["b"]);
  assert.deepEqual(jsonPath(d, "$.data.items[0:1]"), [{ id: 1, name: "a" }]);
  assert.deepEqual(jsonPath(d, "$['id']"), [9]);
  assert.throws(() => jsonPath(d, "$.["), /a name after/);
}

// Format: JSON with variables where values go, XML one element per line.
{
  assert.equal(prettyJson('{"a":1,"d":${n},"e":[],"s":"x, {y}"}'), '{\n  "a": 1,\n  "d": ${n},\n  "e": [],\n  "s": "x, {y}"\n}');
  assert.equal(prettyJson('{"a":'), null, "unbalanced: not formatted");
  assert.equal(prettyXml("<a><b>t</b><c/></a>"), "<a>\n  <b>t</b>\n  <c/>\n</a>");
  assert.equal(formatBody("plain text", "Text"), null);
}

// Bulk edit and response cookies.
{
  const rows = textToRows("Accept: application/json\n// X-Off: 1\nq=two words\n\n: nothing");
  assert.deepEqual(rows, [{ name: "Accept", value: "application/json", enabled: true }, { name: "X-Off", value: "1", enabled: false }, { name: "q", value: "two words", enabled: true }]);
  assert.equal(rowsToText(rows), "Accept: application/json\n// X-Off: 1\nq: two words");
  const cookies = responseCookies({ "set-cookie": "sid=abc; Path=/; HttpOnly; Secure; SameSite=Lax\ntheme=dark; Max-Age=60" });
  assert.deepEqual(cookies.map((c) => [c.name, c.value, c.path, c.expires, c.flags]), [["sid", "abc", "/", "Session", "Secure · HttpOnly · SameSite=Lax"], ["theme", "dark", "", "in 60 s", ""]]);
}

console.log("unit-ux3: all checks passed");

// Stored data that can't be read: named, and copied before anything replaces it.
{
  const map = new Map([["hitop_collections", '[{"id": 1, "na'], ["hitop_environments", "[]"], ["hitop_globals", "[]"]]);
  const storage = { getItem: (k) => map.get(k) ?? null, setItem: (k, v) => void map.set(k, v) };
  assert.deepEqual([...findDamaged(storage)], ["collections", "globals"]);
  assert.equal(map.get("hitop_collections_damaged"), '[{"id": 1, "na');
  assert.equal(findDamaged({ getItem: () => null, setItem() {} }).size, 0);
  assert.equal(findDamaged({ getItem() { throw new Error("blocked"); } }).size, 0);
  const full = { getItem: (k) => (k === "hitop_collections" ? "{" : null), setItem() { throw new Error("full"); } };
  assert.deepEqual([...findDamaged(full)], ["collections"]);
}
console.log("damaged storage ok");
