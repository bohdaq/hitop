/**
 * Variable Interpolation Service
 * 
 * Provides functionality to interpolate variables in strings using ${variable} syntax.
 * Throws errors if variables are not found in the provided variables object.
 */

/**
 * The plugin a `${...}`'s inner text calls (`plugin.name(args)`), or null.
 * @param {string} inner
 * @returns {string|null}
 */
export const pluginOf = (inner) => {
  const m = /^\s*plugin\.([a-z][a-z0-9_-]{0,63})\(.*\)\s*$/s.exec(String(inner));
  return m ? m[1] : null;
};

/**
 * The first plugin a text calls (a variable's value may hold a call), or null.
 * @param {string} text
 * @returns {string|null}
 */
export const pluginIn = (text) => {
  for (const m of String(text).matchAll(/\$\{([^}]+)\}/g)) {
    const p = pluginOf(m[1]);
    if (p) return p;
  }
  return null;
};

/** The refusal for a request that calls a plugin, which this app can't run. */
export const pluginUnavailable = (plugin) =>
  `This request uses plugin '${plugin}', which can't run here: plugins run only where they are installed (hitop CLI: hitop plugin install <file>)`;

/**
 * Interpolates variables in a string template
 * 
 * @param {string} template - String containing ${variable} placeholders
 * @param {object} variables - Object containing variable values
 * @returns {string} - Interpolated string with variables replaced
 * @throws {Error} - If a variable is not found in the variables object
 * 
 * @example
 * const result = interpolateVariables('Hello ${name}!', { name: 'World' });
 * // Returns: 'Hello World!'
 * 
 * @example
 * interpolateVariables('${greeting} ${name}', { greeting: 'Hi' });
 * // Throws: Error: Variable 'name' not found in variables object
 */
export const interpolateVariables = (template, variables = {}) => {
  if (typeof template !== 'string') {
    throw new Error('Template must be a string');
  }

  if (typeof variables !== 'object' || variables === null) {
    throw new Error('Variables must be an object');
  }

  // Regular expression to match ${variableName}
  const variablePattern = /\$\{([^}]+)\}/g;
  
  // Track missing variables
  const missingVariables = [];
  
  // First pass: check for missing variables
  let match;
  const regex = new RegExp(variablePattern);
  while ((match = regex.exec(template)) !== null) {
    const variableName = match[1].trim();
    if (!(variableName in variables)) {
      missingVariables.push(variableName);
    }
  }

  // A plugin call (`${plugin.name(args)}`, spec section 5c) can't run here:
  // the request is refused and the plugin named, never sent with the call as text.
  const call = missingVariables.map(pluginOf).find(Boolean);
  if (call) throw new Error(pluginUnavailable(call));

  // If there are missing variables, throw an error
  if (missingVariables.length > 0) {
    const uniqueMissing = [...new Set(missingVariables)];
    throw new Error(
      `Variable${uniqueMissing.length > 1 ? 's' : ''} '${uniqueMissing.join("', '")}' not found in variables object`
    );
  }

  // Second pass: replace variables
  return template.replace(variablePattern, (match, variableName) => {
    const trimmedName = variableName.trim();
    const value = variables[trimmedName];
    
    // Convert value to string
    if (value === null || value === undefined) {
      return '';
    }

    const text = String(value);
    const plugin = pluginIn(text);
    if (plugin) throw new Error(pluginUnavailable(plugin));
    return text;
  });
};

/**
 * Safely interpolates variables, returning original string if variable not found
 * 
 * @param {string} template - String containing ${variable} placeholders
 * @param {object} variables - Object containing variable values
 * @param {string} defaultValue - Default value for missing variables (default: '')
 * @returns {string} - Interpolated string with variables replaced
 * 
 * @example
 * const result = safeInterpolateVariables('Hello ${name}!', { name: 'World' });
 * // Returns: 'Hello World!'
 * 
 * @example
 * const result = safeInterpolateVariables('Hello ${name}!', {}, 'Unknown');
 * // Returns: 'Hello Unknown!'
 */
export const safeInterpolateVariables = (template, variables = {}, defaultValue = '') => {
  if (typeof template !== 'string') {
    return template;
  }

  if (typeof variables !== 'object' || variables === null) {
    return template;
  }

  const variablePattern = /\$\{([^}]+)\}/g;

  return template.replace(variablePattern, (match, variableName) => {
    const trimmedName = variableName.trim();
    
    if (trimmedName in variables) {
      const value = variables[trimmedName];
      if (value === null || value === undefined) {
        return defaultValue;
      }
      return String(value);
    }
    
    return defaultValue;
  });
};

/**
 * Extracts all variable names from a template string
 * 
 * @param {string} template - String containing ${variable} placeholders
 * @returns {string[]} - Array of unique variable names found in template
 * 
 * @example
 * const vars = extractVariableNames('Hello ${name}, you are ${age} years old');
 * // Returns: ['name', 'age']
 */
export const extractVariableNames = (template) => {
  if (typeof template !== 'string') {
    return [];
  }

  const variablePattern = /\$\{([^}]+)\}/g;
  const variables = [];
  let match;

  while ((match = variablePattern.exec(template)) !== null) {
    variables.push(match[1].trim());
  }

  // Return unique variable names
  return [...new Set(variables)];
};

/**
 * Validates if all required variables are present in the variables object
 * 
 * @param {string} template - String containing ${variable} placeholders
 * @param {object} variables - Object containing variable values
 * @returns {object} - Validation result { valid: boolean, missing: string[] }
 * 
 * @example
 * const result = validateVariables('Hello ${name}!', { name: 'World' });
 * // Returns: { valid: true, missing: [] }
 * 
 * @example
 * const result = validateVariables('${greeting} ${name}', { greeting: 'Hi' });
 * // Returns: { valid: false, missing: ['name'] }
 */
export const validateVariables = (template, variables = {}) => {
  const requiredVariables = extractVariableNames(template);
  const missing = requiredVariables.filter(varName => !(varName in variables));

  return {
    valid: missing.length === 0,
    missing: missing
  };
};

export default {
  interpolateVariables,
  safeInterpolateVariables,
  pluginOf,
  pluginIn,
  pluginUnavailable,
  extractVariableNames,
  validateVariables
};
