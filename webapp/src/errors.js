// UX2 (7): engine and proxy errors as a headline and a next step (the raw text stays under
// Details). Pure: the same text gives the same answer (unit tests in tests/unit-ux.mjs).
//   explainError(raw, { kind: "http" | "grpc" | "websocket" }) → { headline, next, raw, known }
// The CORS and time-limit texts the app writes itself are the model: they already say what to do,
// so their own text is the next step.

const RULES = [
  {
    test: /no response within .* time limit is in settings/i,
    headline: "No response in time",
    next: ({ raw }) => raw,
  },
  {
    test: /requests to localhost and private networks go straight from your browser/i,
    headline: "The browser couldn't reach the server",
    next: ({ raw }) => raw.replace(/^Couldn't reach the server\.\s*/, ""),
  },
  {
    // rustls reading a plaintext answer as TLS: a plaintext server behind an https:// or bare address.
    test: /received corrupt message|InvalidContentType|wrong version number|http2 error: .*frame with invalid size|invalid http version parsed/i,
    headline: ({ kind }) => (kind === "grpc" ? "The server isn't speaking gRPC over TLS" : "The server didn't answer over HTTPS"),
    next: ({ kind }) =>
      kind === "grpc"
        ? "If it is a plaintext server, try http://host:port (or turn on Plaintext)."
        : "If it only speaks plain HTTP, try the address with http:// instead of https://.",
  },
  {
    test: /failed to lookup address|dns error|name or service not known|nodename nor servname|no such host|ENOTFOUND|getaddrinfo|could not resolve host|couldn.t resolve|temporary failure in name resolution/i,
    headline: "Host not found",
    next: "Check the host name for typos, and the value of any variable in it (an environment may be missing). Is the host reachable from here?",
  },
  {
    test: /connection refused|ECONNREFUSED|os error 111|os error 61\b|os error 10061/i,
    headline: "Connection refused",
    next: "Nothing answered on that port: is the server running, and is the port right?",
  },
  {
    test: /certificate|UnknownIssuer|self[- ]signed|NotValidForName|invalid peer certificate|tls handshake|handshake failure|alert received/i,
    headline: "The TLS (HTTPS) handshake failed",
    next: "The server's certificate isn't trusted or doesn't match the host name. Check the address, or try http:// if the server doesn't speak HTTPS.",
  },
  {
    test: /didn't answer within|timed out|deadline has elapsed|operation timed out|ETIMEDOUT|os error 110\b/i,
    headline: "No answer in time",
    next: "The server didn't answer. It may be down, slow or behind a firewall that drops the connection.",
  },
  {
    test: /connection reset|ECONNRESET|broken pipe|os error 104\b|connection closed before message completed|unexpected eof/i,
    headline: "The server closed the connection",
    next: "It hung up before answering. Check the scheme (http or https) and the port, then try again.",
  },
  {
    test: /network is unreachable|no route to host|ENETUNREACH|EHOSTUNREACH|os error 101\b|os error 113\b/i,
    headline: "Network unreachable",
    next: "This device can't route to that address. Check the network connection or VPN.",
  },
  {
    test: /private or reserved address|port \d+ is blocked/i,
    headline: "The proxy won't call that address",
    next: "The HITOP proxy only calls public hosts. Requests to localhost and private networks go straight from your browser; the desktop app calls any address.",
  },
];

const pick = (v, ctx) => (typeof v === "function" ? v(ctx) : v);

/** The error `raw` (an engine, proxy or app text) as { headline, next, raw, known }. */
export function explainError(raw, { kind = "http" } = {}) {
  const text = String(raw ?? "").replace(/^Error:\s*/, "").trim();
  const ctx = { kind, raw: text };
  const rule = RULES.find((r) => r.test.test(text));
  if (!rule) return { headline: "The request failed", next: "", raw: text, known: false };
  return { headline: pick(rule.headline, ctx), next: pick(rule.next, ctx), raw: text, known: true };
}
