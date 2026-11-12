import { digest, fromBase64, isBinaryType } from "./binary.js";

// Response diff: status, headers and body of two responses (history items
// or saved examples). JSON bodies compare as structures (key order doesn't
// matter, arrays by position), anything else line by line.

// What a history item keeps of a response: enough to compare, capped so 50
// items stay well inside localStorage.
export const SNAPSHOT_LIMIT = 100_000;

export function snapshotResponse({ status, headers, body, durationMs }) {
  const text = typeof body === "string" ? body : body == null ? "" : String(body);
  // UX2 (6): a binary body (base64 on the tab) is kept as its size and digest, compared by those.
  const type = Object.entries(headers || {}).find(([k]) => k.toLowerCase() === "content-type")?.[1];
  if (isBinaryType(type)) {
    const bytes = fromBase64(text);
    return {
      status: Number.isInteger(status) ? status : null,
      headers: { ...(headers || {}) },
      body: "",
      binary: { size: bytes.length, digest: digest(bytes) },
      ...(durationMs != null ? { durationMs } : {}),
    };
  }
  return {
    status: Number.isInteger(status) ? status : null,
    headers: { ...(headers || {}) },
    body: text.length > SNAPSHOT_LIMIT ? text.slice(0, SNAPSHOT_LIMIT) : text,
    ...(text.length > SNAPSHOT_LIMIT ? { truncated: true } : {}),
    ...(durationMs != null ? { durationMs } : {}),
  };
}

// An example as saved in a request (`examples` in the folder format: free-form
// maps with name, status, headers, body) as a response.
export function exampleResponse(example) {
  const headers = {};
  const h = example?.headers;
  if (Array.isArray(h)) for (const row of h) row?.name != null && (headers[String(row.name).toLowerCase()] = String(row.value ?? ""));
  else if (h && typeof h === "object") for (const [k, v] of Object.entries(h)) headers[k.toLowerCase()] = String(v);
  const body = typeof example?.body === "string" ? example.body : example?.body == null ? "" : JSON.stringify(example.body, null, 2);
  return { status: Number.isInteger(example?.status) ? example.status : Number(example?.status) || null, headers, body };
}

function parseJson(text) {
  if (typeof text !== "string" || !/^\s*[[{"\d\-tfn]/.test(text)) return undefined;
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

// `$.a.b[0]`, with brackets for keys that aren't plain names.
function child(path, key) {
  if (typeof key === "number") return `${path}[${key}]`;
  return /^[A-Za-z_$][\w$]*$/.test(key) ? `${path}.${key}` : `${path}[${JSON.stringify(key)}]`;
}

const kindOf = (v) => (v === null ? "null" : Array.isArray(v) ? "array" : typeof v);

// [{ path, kind: "added" | "removed" | "changed", before?, after? }], in path order.
export function diffJson(a, b, path = "$", out = []) {
  const [ka, kb] = [kindOf(a), kindOf(b)];
  if (ka === "object" && kb === "object") {
    const keys = [...new Set([...Object.keys(a), ...Object.keys(b)])].sort();
    for (const k of keys) {
      if (!(k in b)) out.push({ path: child(path, k), kind: "removed", before: a[k] });
      else if (!(k in a)) out.push({ path: child(path, k), kind: "added", after: b[k] });
      else diffJson(a[k], b[k], child(path, k), out);
    }
  } else if (ka === "array" && kb === "array") {
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      if (i >= b.length) out.push({ path: child(path, i), kind: "removed", before: a[i] });
      else if (i >= a.length) out.push({ path: child(path, i), kind: "added", after: b[i] });
      else diffJson(a[i], b[i], child(path, i), out);
    }
  } else if (ka !== kb || a !== b) {
    out.push({ path, kind: "changed", before: a, after: b });
  }
  return out;
}

// JSON with object keys sorted, so two bodies that differ only in key order print the same.
export function canonicalJson(value) {
  const sort = (v) => (Array.isArray(v) ? v.map(sort) : v && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sort(v[k])])) : v);
  return JSON.stringify(sort(value), null, 2);
}

// Line diff (longest common subsequence): [{ op: "same" | "added" | "removed", text }].
// Past `limit` cells the middle is reported as removed then added.
export function diffLines(a, b, limit = 4_000_000) {
  const x = a === "" ? [] : a.split(/\r?\n/);
  const y = b === "" ? [] : b.split(/\r?\n/);
  let start = 0;
  while (start < x.length && start < y.length && x[start] === y[start]) start++;
  let endX = x.length;
  let endY = y.length;
  while (endX > start && endY > start && x[endX - 1] === y[endY - 1]) endX--, endY--;
  const head = x.slice(0, start).map((text) => ({ op: "same", text }));
  const tail = x.slice(endX).map((text) => ({ op: "same", text }));
  const mx = x.slice(start, endX);
  const my = y.slice(start, endY);
  const n = mx.length;
  const m = my.length;
  if (n * m > limit) return [...head, ...mx.map((text) => ({ op: "removed", text })), ...my.map((text) => ({ op: "added", text })), ...tail];
  const table = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) table[i][j] = mx[i] === my[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
  const mid = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (mx[i] === my[j]) mid.push({ op: "same", text: mx[i++] }), j++;
    else if (table[i + 1][j] >= table[i][j + 1]) mid.push({ op: "removed", text: mx[i++] });
    else mid.push({ op: "added", text: my[j++] });
  }
  while (i < n) mid.push({ op: "removed", text: mx[i++] });
  while (j < m) mid.push({ op: "added", text: my[j++] });
  return [...head, ...mid, ...tail];
}

// Header names compare without case: [{ name, kind: "same" | "added" | "removed" | "changed", before?, after? }].
export function diffHeaders(a, b) {
  const lower = (h) => Object.fromEntries(Object.entries(h || {}).map(([k, v]) => [k.toLowerCase(), String(v)]));
  const [la, lb] = [lower(a), lower(b)];
  return [...new Set([...Object.keys(la), ...Object.keys(lb)])].sort().map((name) => {
    if (!(name in lb)) return { name, kind: "removed", before: la[name] };
    if (!(name in la)) return { name, kind: "added", after: lb[name] };
    return la[name] === lb[name] ? { name, kind: "same", before: la[name], after: lb[name] } : { name, kind: "changed", before: la[name], after: lb[name] };
  });
}

// Everything the compare view shows. `body.kind` is "json" (both bodies
// parse: `changes` by path, `left`/`right` canonical text, `lines` their
// line diff) or "text" (`lines`).
export function compareResponses(left, right) {
  const status = { before: left?.status ?? null, after: right?.status ?? null };
  status.same = status.before === status.after;
  const headers = diffHeaders(left?.headers, right?.headers);
  if (left?.binary || right?.binary) {
    const [a, b] = [left?.binary ?? null, right?.binary ?? null];
    const body = { kind: "binary", before: a, after: b, lines: [], same: Boolean(a && b && a.size === b.size && a.digest === b.digest) };
    return { status, headers, body, same: status.same && body.same && headers.every((h) => h.kind === "same") };
  }
  const [ja, jb] = [parseJson(left?.body ?? ""), parseJson(right?.body ?? "")];
  let body;
  if (ja !== undefined && jb !== undefined) {
    const [l, r] = [canonicalJson(ja), canonicalJson(jb)];
    body = { kind: "json", changes: diffJson(ja, jb), left: l, right: r, lines: diffLines(l, r) };
  } else {
    body = { kind: "text", lines: diffLines(left?.body ?? "", right?.body ?? "") };
  }
  body.same = body.kind === "json" ? body.changes.length === 0 : body.lines.every((l) => l.op === "same");
  return { status, headers, body, same: status.same && body.same && headers.every((h) => h.kind === "same") };
}

// Rows for a side-by-side view: [{ left?, right?, op }] where a removed line
// pairs with the added line after it.
export function sideBySide(lines) {
  const rows = [];
  for (let k = 0; k < lines.length; k++) {
    const l = lines[k];
    if (l.op === "same") rows.push({ op: "same", left: l.text, right: l.text });
    else if (l.op === "removed") {
      let run = k;
      while (run < lines.length && lines[run].op === "removed") run++;
      let add = run;
      while (add < lines.length && lines[add].op === "added") add++;
      const removed = lines.slice(k, run);
      const added = lines.slice(run, add);
      for (let n = 0; n < Math.max(removed.length, added.length); n++) {
        rows.push({ op: removed[n] && added[n] ? "changed" : removed[n] ? "removed" : "added", left: removed[n]?.text, right: added[n]?.text });
      }
      k = add - 1;
    } else rows.push({ op: "added", right: l.text });
  }
  return rows;
}
