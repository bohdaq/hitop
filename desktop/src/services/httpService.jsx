import { invoke } from '@tauri-apps/api/core';

export const makeHttpRequest = async (url, method, headers, body) => {
  // Convert [{name, value}] → [[name, value]] for the Rust command
  const headerPairs = headers
    .filter(h => h.name && h.value)
    .map(h => [h.name, h.value]);

  const result = await invoke('execute_request', {
    args: { method, url, headers: headerPairs, body: body || '' }
  });

  // Detect response type from content-type header
  const contentType = result.headers['content-type'] || '';
  let type = 'text';
  let data = result.body;

  if (contentType.includes('application/json')) {
    try {
      data = JSON.stringify(JSON.parse(result.body), null, 2);
    } catch (_) {}
    type = 'json';
  } else if (contentType.includes('text/html')) {
    type = 'html';
  } else if (contentType.includes('xml')) {
    type = 'xml';
  }

  return {
    data,
    type,
    status: result.status,
    headers: result.headers,
    duration_ms: result.duration_ms,
    ok: result.status < 400,
  };
};

export const getStatusText = (statusCode) => {
  const statusTexts = {
    200: 'OK', 201: 'Created', 202: 'Accepted', 204: 'No Content',
    301: 'Moved Permanently', 302: 'Found', 304: 'Not Modified',
    400: 'Bad Request', 401: 'Unauthorized', 403: 'Forbidden',
    404: 'Not Found', 405: 'Method Not Allowed', 409: 'Conflict',
    422: 'Unprocessable Entity', 429: 'Too Many Requests',
    500: 'Internal Server Error', 502: 'Bad Gateway',
    503: 'Service Unavailable', 504: 'Gateway Timeout',
  };
  return statusTexts[statusCode] || 'Unknown Status';
};

export const isValidUrl = (url) => {
  try { new URL(url); return true; } catch (_) { return false; }
};

export const buildHeadersObject = (headers) => {
  const obj = {};
  headers.forEach(h => { if (h.name && h.value) obj[h.name] = h.value; });
  return obj;
};

export default { makeHttpRequest, getStatusText, isValidUrl, buildHeadersObject };
