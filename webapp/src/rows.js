// Rows as text (bulk edit) and a response's Set-Cookie lines as rows: pure, unit-checked.

// Bulk edit: rows as "name: value" lines; a line starting with // is a row switched off.
export const rowsToText = (rows) => rows.map((r) => `${r.enabled === false ? "// " : ""}${r.name}: ${r.value ?? ""}`).join("\n");
export function textToRows(text) {
  return String(text)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const off = line.startsWith("//");
      const body = off ? line.slice(2).trim() : line;
      const at = body.search(/[:=]/);
      const name = (at === -1 ? body : body.slice(0, at)).trim();
      const value = at === -1 ? "" : body.slice(at + 1).trim();
      return { name, value, enabled: !off };
    })
    .filter((r) => r.name);
}

// The response's Set-Cookie lines (joined with newlines in the headers object) as rows.
export function responseCookies(headers) {
  const raw = Object.entries(headers || {}).find(([k]) => k.toLowerCase() === "set-cookie")?.[1];
  if (!raw) return [];
  return String(raw)
    .split("\n")
    .filter((l) => l.trim())
    .map((line) => {
      const [pair, ...attrs] = line.split(";").map((x) => x.trim());
      const at = pair.indexOf("=");
      const attr = (name) => attrs.find((a) => a.toLowerCase().startsWith(`${name}=`))?.slice(name.length + 1) ?? "";
      const flag = (name) => attrs.some((a) => a.toLowerCase() === name);
      return {
        name: at === -1 ? pair : pair.slice(0, at),
        value: at === -1 ? "" : pair.slice(at + 1),
        domain: attr("domain"),
        path: attr("path"),
        expires: attr("expires") || (attr("max-age") ? `in ${attr("max-age")} s` : "Session"),
        flags: [flag("secure") && "Secure", flag("httponly") && "HttpOnly", attr("samesite") && `SameSite=${attr("samesite")}`].filter(Boolean).join(" · "),
      };
    });
}

