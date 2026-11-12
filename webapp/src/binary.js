// UX2 (6): binary response bodies. Pure helpers (unit tests in tests/unit-ux2.mjs):
//   isBinaryType(contentType)  images, PDF, audio, video, fonts, archives, octet streams…
//   bodyView(contentType)      "image" | "pdf" | "binary" | null (text)
//   toBase64(bytes) / fromBase64(text)
//   hexDump(bytes, max)        "00000000  89 50 4e 47 …  |.PNG....|" lines, the first `max` bytes
//   digest(bytes)              FNV-1a 32 as 8 hex digits (Diff compares binary bodies by size + this)
// A binary body is kept on the tab as base64 (`response`, `responseType: "binary"`, `responseSize`).

const BINARY_APP = /^application\/(pdf|octet-stream|zip|gzip|x-gzip|x-tar|x-7z-compressed|x-bzip2|x-xz|wasm|x-protobuf|protobuf|vnd\.google\.protobuf|msgpack|x-msgpack|cbor|vnd\.ms-excel|vnd\.ms-powerpoint|msword|vnd\.openxmlformats-officedocument\.[\w.-]+|vnd\.oasis\.opendocument\.[\w.-]+|java-archive|x-sqlite3|x-iso9660-image|x-apple-diskimage|vnd\.android\.package-archive)$/;

const mime = (ct) => String(ct ?? "").split(";")[0].trim().toLowerCase();

export function isBinaryType(contentType) {
  const t = mime(contentType);
  if (!t) return false;
  if (t === "image/svg+xml") return false; // text: shown as XML
  return /^(image|audio|video|font)\//.test(t) || BINARY_APP.test(t);
}

export function bodyView(contentType) {
  if (!isBinaryType(contentType)) return null;
  const t = mime(contentType);
  if (t.startsWith("image/")) return "image";
  if (t === "application/pdf") return "pdf";
  return "binary";
}

export function toBase64(bytes) {
  let text = "";
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text);
}

export function fromBase64(text) {
  try {
    return Uint8Array.from(atob(String(text ?? "")), (c) => c.charCodeAt(0));
  } catch {
    return new Uint8Array(0);
  }
}

export function hexDump(bytes, max = 4096) {
  const n = Math.min(bytes.length, max);
  const lines = [];
  for (let at = 0; at < n; at += 16) {
    const row = bytes.subarray(at, Math.min(at + 16, n));
    const hex = [...row].map((b) => b.toString(16).padStart(2, "0"));
    const left = hex.slice(0, 8).join(" ");
    const right = hex.slice(8).join(" ");
    const text = [...row].map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("");
    lines.push(`${at.toString(16).padStart(8, "0")}  ${left.padEnd(23)}  ${right.padEnd(23)}  |${text}|`);
  }
  return lines.join("\n");
}

export function digest(bytes) {
  let h = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    h ^= bytes[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}
