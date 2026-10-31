/**
 * Tauri Transport
 *
 * Sends requests through the Rust `execute_request` command (reqwest, in
 * hitop-core) instead of the webview's fetch, so CORS never applies and
 * every header can be set and read. Returns the parts of a fetch Response
 * the app reads.
 */
import { invoke } from '@tauri-apps/api/core';

class NativeHeaders {
  constructor(map) {
    this.map = new Map(Object.entries(map || {}).map(([k, v]) => [k.toLowerCase(), v]));
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

/**
 * fetch(url, options) through Rust
 *
 * @param {string} url - Request URL
 * @param {object} options - method, headers (object), body (string)
 * @returns {Promise<object>} - { status, ok, headers, text(), json(), timeMs }
 */
export const sendRequest = async (url, options = {}) => {
  const headers = Object.entries(options.headers || {});
  const result = await invoke('execute_request', {
    args: { method: options.method || 'GET', url, headers, body: options.body || '' }
  });
  return {
    status: result.status,
    ok: result.status >= 200 && result.status < 300,
    headers: new NativeHeaders(result.headers),
    timeMs: result.duration_ms,
    text: async () => result.body,
    json: async () => JSON.parse(result.body),
  };
};

export default { sendRequest };
