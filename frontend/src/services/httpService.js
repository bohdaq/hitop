/**
 * HTTP Service
 * 
 * Handles HTTP request execution and response processing
 */

/**
 * Makes an HTTP request
 * 
 * @param {string} url - Request URL
 * @param {string} method - HTTP method
 * @param {array} headers - Headers array [{ name, value }]
 * @param {string} body - Request body
 * @returns {Promise<object>} - Response object { data, type, status, headers }
 */
export const makeHttpRequest = async (url, method, headers, body) => {
  // Build headers object
  const requestHeaders = {};
  headers.forEach(header => {
    if (header.name && header.value) {
      requestHeaders[header.name] = header.value;
    }
  });

  const fetchOptions = {
    method: method,
    headers: requestHeaders
  };

  // Add body if method supports it
  if (body && (method === 'POST' || method === 'PUT' || method === 'PATCH')) {
    fetchOptions.body = body;
  }

  const response = await fetch(url, fetchOptions);

  // Extract response headers
  const responseHeaders = {};
  response.headers.forEach((value, key) => {
    responseHeaders[key] = value;
  });

  // Determine response type and parse accordingly
  let data;
  let type = 'text';

  const contentType = response.headers.get('content-type');
  
  if (contentType && contentType.includes('application/json')) {
    const jsonData = await response.json();
    data = JSON.stringify(jsonData, null, 2);
    type = 'json';
  } else if (contentType && contentType.includes('text/html')) {
    data = await response.text();
    type = 'html';
  } else if (contentType && contentType.includes('application/xml') || contentType && contentType.includes('text/xml')) {
    data = await response.text();
    type = 'xml';
  } else {
    data = await response.text();
    type = 'text';
  }

  return {
    data,
    type,
    status: response.status,
    headers: responseHeaders,
    ok: response.ok
  };
};

/**
 * Gets HTTP status text
 * 
 * @param {number} statusCode - HTTP status code
 * @returns {string} - Status text
 */
export const getStatusText = (statusCode) => {
  const statusTexts = {
    // 2xx Success
    200: 'OK',
    201: 'Created',
    202: 'Accepted',
    203: 'Non-Authoritative Information',
    204: 'No Content',
    205: 'Reset Content',
    206: 'Partial Content',
    
    // 3xx Redirection
    300: 'Multiple Choices',
    301: 'Moved Permanently',
    302: 'Found',
    303: 'See Other',
    304: 'Not Modified',
    307: 'Temporary Redirect',
    308: 'Permanent Redirect',
    
    // 4xx Client Errors
    400: 'Bad Request',
    401: 'Unauthorized',
    402: 'Payment Required',
    403: 'Forbidden',
    404: 'Not Found',
    405: 'Method Not Allowed',
    406: 'Not Acceptable',
    407: 'Proxy Authentication Required',
    408: 'Request Timeout',
    409: 'Conflict',
    410: 'Gone',
    411: 'Length Required',
    412: 'Precondition Failed',
    413: 'Payload Too Large',
    414: 'URI Too Long',
    415: 'Unsupported Media Type',
    416: 'Range Not Satisfiable',
    417: 'Expectation Failed',
    418: 'I\'m a teapot',
    422: 'Unprocessable Entity',
    429: 'Too Many Requests',
    
    // 5xx Server Errors
    500: 'Internal Server Error',
    501: 'Not Implemented',
    502: 'Bad Gateway',
    503: 'Service Unavailable',
    504: 'Gateway Timeout',
    505: 'HTTP Version Not Supported',
    511: 'Network Authentication Required'
  };

  return statusTexts[statusCode] || 'Unknown Status';
};

/**
 * Validates a URL
 * 
 * @param {string} url - URL to validate
 * @returns {boolean} - True if valid
 */
export const isValidUrl = (url) => {
  try {
    new URL(url);
    return true;
  } catch (error) {
    return false;
  }
};

/**
 * Builds request headers object from headers array
 * 
 * @param {array} headers - Headers array [{ name, value }]
 * @returns {object} - Headers object
 */
export const buildHeadersObject = (headers) => {
  const headersObj = {};
  headers.forEach(header => {
    if (header.name && header.value) {
      headersObj[header.name] = header.value;
    }
  });
  return headersObj;
};

export default {
  makeHttpRequest,
  getStatusText,
  isValidUrl,
  buildHeadersObject
};
