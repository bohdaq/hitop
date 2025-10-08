/**
 * Script Execution Service
 * 
 * Handles execution of pre-request and post-request scripts
 */

/**
 * Executes a pre-request script
 * 
 * @param {string} script - JavaScript code to execute
 * @param {object} requestData - Current request data (url, method, headers, body)
 * @param {object} context - Runtime context
 * @param {object} variables - Collection variables
 * @param {function} updateContext - Function to update context
 * @returns {object|null} - Modified request data or null if script failed
 */
export const executePreRequestScript = (script, requestData, context, variables, updateContext) => {
  if (!script || !script.trim()) {
    return {
      url: requestData.url,
      headers: requestData.headers,
      body: requestData.body
    };
  }

  try {
    // Create a safe execution context
    const scriptContext = {
      url: requestData.url,
      method: requestData.method,
      headers: [...requestData.headers],
      body: requestData.body,
      variables: { ...variables },
      context: { ...context },
      setContext: (key, value) => {
        updateContext(key, value);
      },
      getContext: (key) => {
        return context[key];
      },
      getVariable: (key) => {
        return variables[key];
      },
      setHeader: (name, value) => {
        const existingIndex = scriptContext.headers.findIndex(h => h.name === name);
        if (existingIndex >= 0) {
          scriptContext.headers[existingIndex].value = value;
        } else {
          scriptContext.headers.push({ name, value });
        }
      },
      setUrl: (newUrl) => {
        scriptContext.url = newUrl;
      },
      setBody: (newBody) => {
        scriptContext.body = newBody;
      }
    };

    // Execute the script
    const func = new Function('ctx', `with(ctx) { ${script} }`);
    func(scriptContext);

    return {
      url: scriptContext.url,
      headers: scriptContext.headers,
      body: scriptContext.body
    };
  } catch (error) {
    console.error('Pre-request script error:', error);
    throw new Error(`Pre-request script error: ${error.message}`);
  }
};

/**
 * Executes a post-request script
 * 
 * @param {string} script - JavaScript code to execute
 * @param {object} response - Response data
 * @param {string} responseText - Raw response text
 * @param {object} responseHeaders - Response headers
 * @param {number} statusCode - HTTP status code
 * @param {object} context - Runtime context
 * @param {object} variables - Collection variables
 * @param {function} updateContext - Function to update context
 * @returns {void}
 */
export const executePostRequestScript = (script, response, responseText, responseHeaders, statusCode, context, variables, updateContext) => {
  if (!script || !script.trim()) {
    return;
  }

  try {
    let parsedResponse = response;
    if (typeof response === 'string') {
      try {
        parsedResponse = JSON.parse(response);
      } catch (e) {
        parsedResponse = response;
      }
    }

    const scriptContext = {
      response: parsedResponse,
      responseText: responseText,
      responseHeaders: responseHeaders,
      statusCode: statusCode,
      variables: { ...variables },
      context: { ...context },
      setContext: (key, value) => {
        updateContext(key, value);
      },
      getContext: (key) => {
        return context[key];
      },
      getVariable: (key) => {
        return variables[key];
      },
      getResponseValue: (path) => {
        const keys = path.split('.');
        let value = parsedResponse;
        for (const key of keys) {
          if (value && typeof value === 'object') {
            value = value[key];
          } else {
            return undefined;
          }
        }
        return value;
      },
      getResponseHeader: (name) => {
        return responseHeaders[name.toLowerCase()];
      }
    };

    const func = new Function('ctx', `with(ctx) { ${script} }`);
    func(scriptContext);
  } catch (error) {
    console.error('Post-request script error:', error);
    throw new Error(`Post-request script error: ${error.message}`);
  }
};

export default {
  executePreRequestScript,
  executePostRequestScript
};
