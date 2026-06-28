import { invoke } from '@tauri-apps/api/core';

/**
 * Executes a pre-request script via the Rust/boa_engine backend.
 * Returns { url, headers, body } with any modifications applied.
 */
export const executePreRequestScript = async (
  script,
  requestData,
  context,
  variables,
  updateContext
) => {
  if (!script || !script.trim()) {
    return { url: requestData.url, headers: requestData.headers, body: requestData.body };
  }

  const headerPairs = requestData.headers
    .filter(h => h.name)
    .map(h => [h.name, h.value]);

  const result = await invoke('run_pre_script', {
    args: {
      script,
      url: requestData.url,
      method: requestData.method,
      headers: headerPairs,
      body: requestData.body || '',
      variables: variables || {},
      context: context || {},
    }
  });

  // Apply context updates
  Object.entries(result.context_updates || {}).forEach(([k, v]) => updateContext(k, v));

  // Convert [[name, value]] back to [{name, value}]
  const headers = (result.headers || []).map(([name, value]) => ({ name, value }));

  return { url: result.url, headers, body: result.body };
};

/**
 * Executes a post-request script via the Rust/boa_engine backend.
 */
export const executePostRequestScript = async (
  script,
  response,
  responseText,
  responseHeaders,
  statusCode,
  context,
  variables,
  updateContext
) => {
  if (!script || !script.trim()) return;

  const result = await invoke('run_post_script', {
    args: {
      script,
      response_body: typeof responseText === 'string' ? responseText : JSON.stringify(response),
      response_headers: responseHeaders || {},
      status_code: statusCode,
      variables: variables || {},
      context: context || {},
    }
  });

  Object.entries(result.context_updates || {}).forEach(([k, v]) => updateContext(k, v));
};

export default { executePreRequestScript, executePostRequestScript };
