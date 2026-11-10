/**
 * Transport Service
 *
 * Sends a request either straight from the browser (fetch) or through the
 * HITOP request proxy, which removes CORS limits. The proxy is used only when
 * the build sets VITE_HITOP_PROXY (the hitop.rws8.tech web app) and the
 * target is a public host: the proxy can't reach your localhost or private
 * network, so those always go direct. Extensions build without it and keep
 * using fetch, which their host permissions already free from CORS.
 */

const PROXY_URL = import.meta.env.VITE_HITOP_PROXY || '';

const PRIVATE_HOST = [
  /^localhost$/i,
  /\.localhost$/i,
  /\.local$/i,
  /^127\./,
  /^10\./,
  /^192\.168\./,
  /^172\.(1[6-9]|2\d|3[01])\./,
  /^169\.254\./,
  /^0\.0\.0\.0$/,
  /^\[?::1\]?$/,
  /^\[?f[cd][0-9a-f]{2}:/i,
  /^\[?fe80:/i,
];

/**
 * Whether a request to this URL goes through the proxy
 *
 * @param {string} url - Request URL
 * @returns {boolean}
 */
export const usesProxy = (url) => {
  if (!PROXY_URL) return false;
  try {
    const { hostname, protocol } = new URL(url);
    if (protocol !== 'http:' && protocol !== 'https:') return false;
    return !PRIVATE_HOST.some(re => re.test(hostname));
  } catch {
    return false;
  }
};

/** Read only Headers lookalike that keeps repeated headers (Set-Cookie). */
class ProxiedHeaders {
  constructor(pairs) {
    // Every header as sent, repeats included (Set-Cookie needs them apart).
    this.pairs = pairs;
    this.map = new Map();
    for (const [name, value] of pairs) {
      const key = name.toLowerCase();
      const joined = this.map.has(key) ? `${this.map.get(key)}${key === 'set-cookie' ? '\n' : ', '}${value}` : value;
      this.map.set(key, joined);
    }
  }

  get(name) {
    return this.map.has(name.toLowerCase()) ? this.map.get(name.toLowerCase()) : null;
  }

  has(name) {
    return this.map.has(name.toLowerCase());
  }

  forEach(callback) {
    this.map.forEach((value, key) => callback(value, key, this));
  }
}

const decodeBody = (body, encoding) => {
  if (encoding !== 'base64') return body;
  const bytes = Uint8Array.from(atob(body), c => c.charCodeAt(0));
  return new TextDecoder().decode(bytes);
};

/** The parts of a fetch Response the app reads, built from a proxy answer. */
class ProxiedResponse {
  constructor(envelope) {
    this.status = envelope.status;
    this.statusText = envelope.statusText;
    this.ok = envelope.status >= 200 && envelope.status < 300;
    this.url = envelope.url;
    this.headers = new ProxiedHeaders(envelope.headers);
    this.timeMs = envelope.timeMs;
    // DNS, connect, waiting, download (hitop-server, HITOP Next track B); undefined from older servers.
    this.timings = envelope.timings;
    // "HTTP/1.1", "HTTP/2", "HTTP/3" as the proxy received it; undefined from older servers.
    this.httpVersion = envelope.httpVersion;
    this.truncated = envelope.truncated;
    this.bodyText = decodeBody(envelope.body, envelope.bodyEncoding);
  }

  async text() {
    return this.bodyText;
  }

  async json() {
    return JSON.parse(this.bodyText);
  }
}

const toBase64 = (bytes) => {
  let text = '';
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text);
};

const headerPairs = (headers) => {
  if (!headers) return [];
  if (Array.isArray(headers)) return headers.map(h => [h.name, h.value]);
  return Object.entries(headers);
};

/**
 * fetch(url, options), through the proxy when usesProxy(url)
 *
 * @param {string} url - Request URL
 * @param {object} options - fetch options: method, headers (object), body (string)
 * @returns {Promise<Response|ProxiedResponse>}
 */
export const sendRequest = async (url, options = {}) => {
  if (!usesProxy(url)) {
    return fetch(url, options);
  }
  // Binary bodies (multipart with files) travel base64 encoded.
  const binary = options.body instanceof Uint8Array;
  const answer = await fetch(PROXY_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      method: options.method || 'GET',
      url,
      headers: headerPairs(options.headers),
      body: binary ? toBase64(options.body) : options.body ?? null,
      ...(binary ? { bodyEncoding: 'base64' } : {}),
    }),
  });
  let envelope;
  try {
    envelope = await answer.json();
  } catch {
    throw new Error(`HITOP proxy failed (HTTP ${answer.status}). Try again in a minute.`);
  }
  if (!answer.ok || envelope.error) {
    throw new Error(envelope.error || `HITOP proxy failed (HTTP ${answer.status})`);
  }
  return new ProxiedResponse(envelope);
};

export default { sendRequest, usesProxy };
