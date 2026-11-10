// Small facts the request page shows about a request and its last response (HITOP Next, Main
// board): the response's HTTP version and the raw body's kind and JSON validity. Only what the
// app knows: no TLS version (the engines don't report it) and no schema checks (no schemas kept).

// "HTTP/2" → "h2" (the ALPN name), "HTTP/3" → "h3"; HTTP/1.x stays as is.
export function shortVersion(version) {
  if (version === "HTTP/2") return "h2";
  if (version === "HTTP/3") return "h3";
  return version || "";
}

// The status bar's "HTTPS · h2": the final URL's scheme and the version; "" without a version
// (a direct browser fetch can't see it).
export function protocolLabel(version, url) {
  if (!version) return "";
  const m = /^([a-z][a-z0-9+.-]*):/i.exec(String(url ?? ""));
  const scheme = m ? m[1].toUpperCase() : "";
  return scheme ? `${scheme} · ${shortVersion(version)}` : shortVersion(version);
}

// The raw body's kind: from the Content-Type when it says, else from the text.
export function bodyKind(body, contentType) {
  const ct = String(contentType || "").toLowerCase();
  if (/json/.test(ct)) return "JSON";
  if (/xml/.test(ct)) return "XML";
  if (ct) return "Text";
  const text = String(body || "").trimStart();
  if (/^[[{]/.test(text)) return "JSON";
  if (text.startsWith("<")) return "XML";
  return "Text";
}

// 1-based line and column of an offset in a text.
export function lineCol(text, offset) {
  let line = 1;
  let col = 1;
  for (let i = 0; i < offset && i < text.length; i++) {
    if (text[i] === "\n") {
      line++;
      col = 1;
    } else col++;
  }
  return { line, col };
}

// Checks a JSON text (RFC 8259) with ${variables} standing for values, as hitop fills them in
// before sending. Returns { ok: true } or { ok: false, line, col } of the first offending character.
export function checkJson(body) {
  // A ${name} becomes a number padded with spaces to the same length, so positions stay put.
  const text = String(body ?? "").replace(/\$\{[^}]*\}/g, (m) => `0${" ".repeat(m.length - 1)}`);
  let i = 0;
  const fail = () => {
    throw i;
  };
  const ws = () => {
    while (i < text.length && " \t\n\r".includes(text[i])) i++;
  };
  const literal = (word) => {
    if (text.startsWith(word, i)) i += word.length;
    else fail();
  };
  const string = () => {
    i++; // the opening quote
    for (;;) {
      if (i >= text.length) fail();
      const c = text[i];
      if (c === '"') return void i++;
      if (c < " ") fail();
      if (c === "\\") {
        i++;
        const e = text[i];
        if (e === "u") {
          for (let k = 1; k <= 4; k++) {
            if (!/[0-9a-fA-F]/.test(text[i + k] ?? "")) {
              i += k;
              fail();
            }
          }
          i += 5;
        } else if (e !== undefined && '"\\/bfnrt'.includes(e)) i++;
        else fail();
      } else i++;
    }
  };
  const number = () => {
    const m = /^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/.exec(text.slice(i));
    if (!m) fail();
    i += m[0].length;
  };
  const value = () => {
    ws();
    const c = text[i];
    if (c === "{") {
      i++;
      ws();
      if (text[i] === "}") return void i++;
      for (;;) {
        ws();
        if (text[i] !== '"') fail();
        string();
        ws();
        if (text[i] !== ":") fail();
        i++;
        value();
        ws();
        if (text[i] === ",") i++;
        else if (text[i] === "}") return void i++;
        else fail();
      }
    }
    if (c === "[") {
      i++;
      ws();
      if (text[i] === "]") return void i++;
      for (;;) {
        value();
        ws();
        if (text[i] === ",") i++;
        else if (text[i] === "]") return void i++;
        else fail();
      }
    }
    if (c === '"') return string();
    if (c === "t") return literal("true");
    if (c === "f") return literal("false");
    if (c === "n") return literal("null");
    if (c === "-" || (c >= "0" && c <= "9")) return number();
    fail();
  };
  try {
    value();
    ws();
    if (i < text.length) fail();
    return { ok: true };
  } catch (at) {
    if (typeof at !== "number") throw at;
    return { ok: false, ...lineCol(text, at) };
  }
}
