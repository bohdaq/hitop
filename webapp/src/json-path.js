// A small JSONPath for the response filter: $, .name, ['name'], [n] (negative from the end),
// [start:end], [*], .*, ..name (any depth) and [a,b] (names or indexes). Returns the matches.
// No filter expressions (?()) and no scripts: what people type to narrow a response.

function tokens(expr) {
  const out = [];
  let i = 0;
  const s = expr.trim();
  if (s.startsWith("$")) i = 1;
  while (i < s.length) {
    if (s.startsWith("..", i)) {
      i += 2;
      if (s[i] === "*") {
        out.push({ deep: true, all: true });
        i += 1;
        continue;
      }
      const m = /^[A-Za-z_$][\w$-]*/.exec(s.slice(i));
      if (!m) throw new Error(`a name after .. at ${i + 1}`);
      out.push({ deep: true, names: [m[0]] });
      i += m[0].length;
    } else if (s[i] === ".") {
      i += 1;
      if (s[i] === "*") {
        out.push({ all: true });
        i += 1;
        continue;
      }
      const m = /^[A-Za-z_$][\w$-]*/.exec(s.slice(i));
      if (!m) throw new Error(`a name after . at ${i + 1}`);
      out.push({ names: [m[0]] });
      i += m[0].length;
    } else if (s[i] === "[") {
      const end = s.indexOf("]", i);
      if (end === -1) throw new Error("a [ without ]");
      const inner = s.slice(i + 1, end).trim();
      i = end + 1;
      if (inner === "*") {
        out.push({ all: true });
        continue;
      }
      const slice = /^(-?\d*):(-?\d*)$/.exec(inner);
      if (slice) {
        out.push({ slice: [slice[1] === "" ? null : Number(slice[1]), slice[2] === "" ? null : Number(slice[2])] });
        continue;
      }
      const parts = inner.split(",").map((p) => p.trim());
      const keys = parts.map((p) => {
        const q = /^(['"])(.*)\1$/.exec(p);
        if (q) return q[2];
        if (/^-?\d+$/.test(p)) return Number(p);
        throw new Error(`[${inner}] isn't a name, an index or a slice`);
      });
      out.push({ names: keys });
    } else if (/[A-Za-z_]/.test(s[i]) && out.length === 0 && i === 0) {
      // "data.items" without the leading $.
      const m = /^[A-Za-z_$][\w$-]*/.exec(s.slice(i));
      out.push({ names: [m[0]] });
      i += m[0].length;
    } else {
      throw new Error(`unexpected "${s[i]}" at ${i + 1}`);
    }
  }
  return out;
}

const children = (v) => (Array.isArray(v) ? v : v && typeof v === "object" ? Object.values(v) : []);

function pick(v, key) {
  if (Array.isArray(v) && typeof key === "number") {
    const at = key < 0 ? v.length + key : key;
    return at >= 0 && at < v.length ? [v[at]] : [];
  }
  if (v && typeof v === "object" && Object.prototype.hasOwnProperty.call(v, String(key))) return [v[String(key)]];
  return [];
}

function descendants(v, out = []) {
  out.push(v);
  for (const c of children(v)) descendants(c, out);
  return out;
}

/** The values `expr` selects in `value`; throws with a short reason on a bad expression. */
export function jsonPath(value, expr) {
  let current = [value];
  for (const t of tokens(expr)) {
    const from = t.deep ? current.flatMap((v) => descendants(v)) : current;
    if (t.all) current = from.flatMap(children);
    else if (t.slice) {
      current = from.flatMap((v) => (Array.isArray(v) ? v.slice(t.slice[0] ?? 0, t.slice[1] ?? v.length) : []));
    } else current = from.flatMap((v) => t.names.flatMap((k) => pick(v, k)));
  }
  return current;
}
