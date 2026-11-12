// UX2 (2): a pasted curl command as a request, deterministically (no AI). Pure: unit tests in
// tests/unit-ux2.mjs, including every curl.sh hitop-core's snippet goldens write.
//   isCurl(text)     → whether the text is a curl command ("curl …", "$ curl …")
//   parseCurl(text)  → { method, url, headers: [{name, value}], body, bodyMode, formFields, auth,
//                        warnings: [text] } or throws an Error saying what is wrong
// Shell words as bash reads them: '…', "…" (\ escapes \ " $ ` and newline), $'…' (C escapes),
// \ outside quotes, line continuations (\, and ^ / ` at a line end for cmd and PowerShell).
// Postman-style {{name}} (hitop's own snippets write variables that way) becomes ${name}.

const CURL = /^\s*(?:\$\s+)?curl(?:\.exe)?(?=\s|$)/;

export const isCurl = (text) => CURL.test(String(text ?? ""));

// Options that take a value and only change how curl runs: skipped with their value.
const SKIP_WITH_VALUE = new Set([
  "-o", "--output", "-m", "--max-time", "--connect-timeout", "-w", "--write-out", "--retry", "--retry-delay", "--retry-max-time",
  "-x", "--proxy", "--cacert", "--capath", "--cert", "-E", "--key", "--cert-type", "--key-type", "-c", "--cookie-jar", "-K", "--config",
  "--resolve", "--connect-to", "--limit-rate", "-y", "--speed-time", "-Y", "--speed-limit", "--max-redirs", "--interface", "-r", "--range",
  "-z", "--time-cond", "--dns-servers", "--proxy-user", "-U", "--trace", "--trace-ascii", "--stderr", "--max-filesize", "--expect100-timeout",
  "-D", "--dump-header", "--unix-socket", "--abstract-unix-socket", "--happy-eyeballs-timeout-ms", "--keepalive-time", "--tls-max", "--ciphers",
]);
// Flags with no value that change nothing a request keeps (or that HITOP does anyway).
const SKIP_FLAGS = new Set([
  "-s", "--silent", "-S", "--show-error", "-L", "--location", "-k", "--insecure", "-i", "--include", "-v", "--verbose", "--compressed",
  "-N", "--no-buffer", "-f", "--fail", "--fail-with-body", "-#", "--progress-bar", "-q", "--disable", "-g", "--globoff", "--http1.0",
  "--http1.1", "--http2", "--http2-prior-knowledge", "--http3", "-4", "--ipv4", "-6", "--ipv6", "--no-keepalive", "--tr-encoding",
  "--path-as-is", "--location-trusted", "--raw", "-n", "--netrc", "--ssl", "--ssl-reqd", "--tlsv1.2", "--tlsv1.3", "-l", "--list-only",
  "--no-progress-meter", "-O", "--remote-name", "-J", "--remote-header-name", "--create-dirs",
]);
// Short flags with no value, for clusters like -sSL.
const SHORT_FLAGS = new Set(["s", "S", "L", "k", "i", "v", "N", "f", "q", "g", "4", "6", "n", "l", "O", "J", "G", "I", "#"]);
// Short options whose value may be glued on (-XPOST, -H'Accept: x').
const SHORT_WITH_VALUE = new Set(["X", "H", "d", "u", "A", "e", "b", "F", "o", "m", "w", "x", "E", "c", "K", "r", "z", "y", "Y", "U", "D"]);

/** Splits a command line into words as a POSIX shell would (no expansion). */
export function shellWords(text) {
  const src = String(text ?? "")
    // Line continuations: bash's backslash, cmd's caret, PowerShell's backtick.
    .replace(/\\\r?\n/g, " ")
    .replace(/\^\r?\n/g, " ")
    .replace(/`\r?\n/g, " ");
  const words = [];
  let word = null;
  const push = (s) => (word = (word ?? "") + s);
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (c === "'") {
      const end = src.indexOf("'", i + 1);
      if (end === -1) throw new Error("A ' quote isn't closed.");
      push(src.slice(i + 1, end));
      i = end;
    } else if (c === "$" && src[i + 1] === "'") {
      let out = "";
      let j = i + 2;
      for (; j < src.length && src[j] !== "'"; j++) {
        if (src[j] !== "\\") {
          out += src[j];
          continue;
        }
        const n = src[++j];
        const map = { n: "\n", t: "\t", r: "\r", "\\": "\\", "'": "'", '"': '"', a: "\x07", b: "\b", e: "\x1b", f: "\f", v: "\v", 0: "\0" };
        if (n === "x" || n === "u") {
          const hex = /^[0-9a-fA-F]+/.exec(src.slice(j + 1, j + 1 + (n === "x" ? 2 : 4)))?.[0] ?? "";
          out += hex ? String.fromCodePoint(parseInt(hex, 16)) : `\\${n}`;
          j += hex.length;
        } else out += map[n] ?? `\\${n}`;
      }
      if (j >= src.length) throw new Error("A $' quote isn't closed.");
      push(out);
      i = j;
    } else if (c === '"') {
      let out = "";
      let j = i + 1;
      for (; j < src.length && src[j] !== '"'; j++) {
        if (src[j] === "\\" && '\\"$`\n'.includes(src[j + 1] ?? "")) out += src[++j];
        else out += src[j];
      }
      if (j >= src.length) throw new Error('A " quote isn\'t closed.');
      push(out);
      i = j;
    } else if (c === "\\") {
      if (i + 1 < src.length) push(src[++i]);
    } else if (/\s/.test(c)) {
      if (word !== null) words.push(word);
      word = null;
    } else push(c);
  }
  if (word !== null) words.push(word);
  return words;
}

const vars = (s) => s.replace(/\{\{\s*([A-Za-z_$][\w.$-]*)\s*\}\}/g, (_, n) => `\${${n}}`);

// --data-urlencode's forms: content, =content, name=content, @file, name@file.
function urlencodeArg(v) {
  const eq = v.indexOf("=");
  if (eq === -1) return v.startsWith("@") ? null : encodeURIComponent(v);
  const name = v.slice(0, eq);
  return name ? `${name}=${encodeURIComponent(v.slice(eq + 1))}` : encodeURIComponent(v.slice(1));
}

/** The request a curl command makes. Throws when it isn't one or has no URL. */
export function parseCurl(text) {
  if (!isCurl(text)) throw new Error("That isn't a curl command: it should start with curl.");
  const words = shellWords(String(text).replace(/^\s*\$\s+/, ""));
  // Several commands joined with && or ; or |: the first one.
  const stop = words.findIndex((w, i) => i > 0 && ["&&", "||", ";", "|"].includes(w));
  const args = (stop === -1 ? words : words.slice(0, stop)).slice(1);
  let method = null;
  let url = null;
  const headers = [];
  const data = [];
  const form = [];
  let auth = null;
  let get = false;
  let head = false;
  let json = false;
  const warnings = [];
  const unknown = [];
  const take = (i, name) => {
    if (i + 1 >= args.length) throw new Error(`${name} needs a value.`);
    return args[i + 1];
  };
  for (let i = 0; i < args.length; i++) {
    let a = args[i];
    let value;
    // --opt=value
    if (a.startsWith("--") && a.includes("=")) {
      value = a.slice(a.indexOf("=") + 1);
      a = a.slice(0, a.indexOf("="));
    }
    // -XPOST, -H'Accept: x' (a value glued to a short option)
    if (/^-[A-Za-z#]./.test(a) && !a.startsWith("--")) {
      if (SHORT_WITH_VALUE.has(a[1])) {
        value = a.slice(2);
        a = a.slice(0, 2);
      } else if ([...a.slice(1)].every((f) => SHORT_FLAGS.has(f) || SHORT_WITH_VALUE.has(f))) {
        // A cluster like -sSL or -sSX POST: flags, the last may take the next word.
        const flags = [...a.slice(1)];
        const last = flags[flags.length - 1];
        for (const f of flags.slice(0, -1)) {
          if (f === "G") get = true;
          else if (f === "I") head = true;
          else if (SHORT_WITH_VALUE.has(f)) throw new Error(`-${f} needs a value.`);
        }
        a = `-${last}`;
      }
    }
    const val = () => {
      if (value !== undefined) return value;
      const v = take(i, a);
      i++;
      return v;
    };
    switch (a) {
      case "-X":
      case "--request":
        method = val().toUpperCase();
        break;
      case "-H":
      case "--header": {
        const h = val();
        const colon = h.indexOf(":");
        if (colon === -1) {
          // "-H 'X-Thing;'" sends an empty header; "-H 'X-Thing'" removes one: both kept as empty.
          const name = h.replace(/;$/, "").trim();
          if (name) headers.push({ name, value: "" });
        } else headers.push({ name: h.slice(0, colon).trim(), value: vars(h.slice(colon + 1).trim()) });
        break;
      }
      case "-d":
      case "--data":
      case "--data-ascii":
      case "--data-binary":
      case "--data-raw": {
        const v = val();
        if (v.startsWith("@") && a !== "--data-raw") warnings.push(`The body comes from the file ${v.slice(1)}: paste its contents into Body.`);
        else data.push(a === "--data-binary" || a === "--data-raw" ? v : v.replace(/[\r\n]/g, ""));
        break;
      }
      case "--data-urlencode": {
        const v = urlencodeArg(val());
        if (v === null) warnings.push("A --data-urlencode value comes from a file: add it to Body.");
        else data.push(v);
        break;
      }
      case "--json":
        data.push(val());
        json = true;
        break;
      case "-F":
      case "--form":
      case "--form-string": {
        const v = val();
        const eq = v.indexOf("=");
        if (eq === -1) throw new Error(`${a} '${v}' should be name=value.`);
        const key = v.slice(0, eq);
        const rest = v.slice(eq + 1);
        if (a !== "--form-string" && (rest.startsWith("@") || rest.startsWith("<"))) {
          const fileName = (/^"([^"]*)"/.exec(rest.slice(1))?.[1] ?? rest.slice(1).split(";")[0]).trim();
          form.push({ key, value: "", type: "file", fileName });
        } else form.push({ key, value: vars(a === "--form-string" ? rest : rest.replace(/^"(.*)"$/, "$1").split(";type=")[0]), type: "text" });
        break;
      }
      case "-u":
      case "--user": {
        const v = val();
        const colon = v.indexOf(":");
        auth = { type: "basic", username: vars(colon === -1 ? v : v.slice(0, colon)), password: vars(colon === -1 ? "" : v.slice(colon + 1)) };
        break;
      }
      case "-A":
      case "--user-agent":
        headers.push({ name: "User-Agent", value: vars(val()) });
        break;
      case "-e":
      case "--referer":
        headers.push({ name: "Referer", value: vars(val()) });
        break;
      case "-b":
      case "--cookie": {
        const v = val();
        if (v.includes("=")) headers.push({ name: "Cookie", value: vars(v) });
        else warnings.push(`Cookies from the file ${v} were left out.`);
        break;
      }
      case "--oauth2-bearer":
        auth = { type: "bearer", token: vars(val()) };
        break;
      case "--url":
        url = val();
        break;
      case "-G":
      case "--get":
        get = true;
        break;
      case "-I":
      case "--head":
        head = true;
        break;
      default:
        if (SKIP_WITH_VALUE.has(a)) {
          if (value === undefined) i++;
        } else if (SKIP_FLAGS.has(a)) {
          // nothing a request keeps
        } else if (a.startsWith("-") && a.length > 1) {
          unknown.push(a);
        } else if (url === null) url = a;
        else warnings.push(`Only the first URL is used (${a} left out).`);
    }
  }
  if (!url) throw new Error("The curl command has no URL.");
  if (unknown.length) warnings.push(`Left out: ${[...new Set(unknown)].join(", ")}.`);
  url = vars(url);
  // -G: the data goes into the query.
  if (get && data.length) {
    url += (url.includes("?") ? "&" : "?") + data.join("&");
    data.length = 0;
  }
  const has = (n) => headers.some((h) => h.name.toLowerCase() === n);
  if (json) {
    if (!has("content-type")) headers.push({ name: "Content-Type", value: "application/json" });
    if (!has("accept")) headers.push({ name: "Accept", value: "application/json" });
  }
  let body = "";
  let bodyMode = "raw";
  let formFields = [];
  if (form.length) {
    bodyMode = "multipart";
    formFields = form.map((f, n) => ({ id: `curl-${n}`, enabled: true, ...f }));
  } else if (data.length) {
    body = vars(data.join("&"));
    // curl sends -d as a form unless told otherwise: said so when it isn't JSON the app would see.
    if (!has("content-type") && !/^\s*[[{]/.test(body)) headers.push({ name: "Content-Type", value: "application/x-www-form-urlencoded" });
  }
  method = method ?? (head ? "HEAD" : form.length || data.length ? "POST" : "GET");
  return { method, url, headers, body, bodyMode, formFields, auth, warnings };
}
