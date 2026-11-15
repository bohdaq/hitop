// "Format" for a request body: JSON re-indented character by character (so a body with
// ${variables} where values go still formats), XML one element per line. Returns null when the
// text doesn't look like either.

/** JSON (or JSON with ${variables} in it) indented by two spaces; strings are left untouched. */
export function prettyJson(text) {
  const src = String(text ?? "").trim();
  if (!/^[[{]/.test(src)) return null;
  let out = "";
  let depth = 0;
  let inString = false;
  const pad = () => "\n" + "  ".repeat(depth);
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (inString) {
      out += c;
      if (c === "\\") out += src[++i] ?? "";
      else if (c === '"') inString = false;
      continue;
    }
    if (c === "$" && src[i + 1] === "{") {
      // A ${variable} where a value goes: copied as it is.
      const end = src.indexOf("}", i);
      if (end === -1) return null;
      out += src.slice(i, end + 1);
      i = end;
    } else if (c === '"') {
      inString = true;
      out += c;
    } else if (c === "{" || c === "[") {
      // An empty object or list stays on one line.
      const next = src.slice(i + 1).match(/^\s*([}\]])/);
      if (next) {
        out += c + next[1];
        i += next[0].length;
      } else {
        depth++;
        out += c + pad();
      }
    } else if (c === "}" || c === "]") {
      depth = Math.max(0, depth - 1);
      out += pad() + c;
    } else if (c === ",") {
      out += "," + pad();
    } else if (c === ":") {
      out += ": ";
    } else if (/\s/.test(c)) {
      // Whitespace outside strings is rebuilt.
    } else {
      out += c;
    }
  }
  return inString || depth !== 0 ? null : out;
}

/** XML with one element per line, indented by two spaces; <a>text</a> stays on one line. */
export function prettyXml(text) {
  const src = String(text ?? "").trim();
  if (!src.startsWith("<")) return null;
  const tokens = src.match(/<[^>]+>|[^<]+/g)?.map((t) => (t.startsWith("<") ? t : t.trim())).filter(Boolean) ?? [];
  const opening = (t) => /^<[^/!?]/.test(t) && !t.endsWith("/>");
  const lines = [];
  let depth = 0;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (opening(t) && tokens[i + 1] && !tokens[i + 1].startsWith("<") && tokens[i + 2]?.startsWith("</")) {
      lines.push("  ".repeat(depth) + t + tokens[i + 1] + tokens[i + 2]);
      i += 2;
    } else if (t.startsWith("</")) {
      depth = Math.max(0, depth - 1);
      lines.push("  ".repeat(depth) + t);
    } else {
      lines.push("  ".repeat(depth) + t);
      if (opening(t)) depth++;
    }
  }
  return lines.join("\n");
}

/** The body formatted for its kind ("JSON", "XML", or what request-facts' bodyKind says). */
export function formatBody(text, kind) {
  if (kind === "XML" || kind === "HTML") return prettyXml(text);
  return prettyJson(text) ?? prettyXml(text);
}
