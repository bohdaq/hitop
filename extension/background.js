/**
 * Background script for HITOP Extension
 * Handles extension lifecycle and message passing
 * Compatible with both Firefox (browser) and Chrome (chrome)
 */

// Use chrome API if available, otherwise fall back to browser (Firefox)
const extensionAPI = typeof chrome !== 'undefined' ? chrome : browser;

// Listen for extension installation
extensionAPI.runtime.onInstalled.addListener((details) => {
  if (details.reason === 'install') {
    console.log('HITOP extension installed');
    // Storage is handled by localStorage in the popup
  }
});

// Handle toolbar icon click - open app in new tab
extensionAPI.action.onClicked.addListener(() => {
  const appUrl = extensionAPI.runtime.getURL('app/index.html');
  extensionAPI.tabs.create({ url: appUrl });
});

// Listen for messages from popup
extensionAPI.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'MAKE_REQUEST') {
    makeHttpRequest(message.data)
      .then(response => sendResponse({ success: true, data: response }))
      .catch(error => sendResponse({ success: false, error: error.message }));
    return true; // Keep channel open for async response
  }
});

/**
 * Makes an HTTP request with the given parameters
 */
async function makeHttpRequest({ url, method, headers, body }) {
  try {
    const fetchOptions = {
      method: method,
      headers: headers || {}
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

    // Get response text
    const responseText = await response.text();
    
    // Determine response type
    let data = responseText;
    let type = 'text';
    const contentType = response.headers.get('content-type');
    
    if (contentType && contentType.includes('application/json')) {
      try {
        const jsonData = JSON.parse(responseText);
        data = JSON.stringify(jsonData, null, 2);
        type = 'json';
      } catch (e) {
        // Not valid JSON, keep as text
      }
    } else if (contentType && contentType.includes('text/html')) {
      type = 'html';
    } else if (contentType && (contentType.includes('application/xml') || contentType.includes('text/xml'))) {
      type = 'xml';
    }

    return {
      data,
      type,
      status: response.status,
      headers: responseHeaders,
      ok: response.ok
    };
  } catch (error) {
    throw new Error(`Request failed: ${error.message}`);
  }
}

console.log('HITOP background script loaded');
