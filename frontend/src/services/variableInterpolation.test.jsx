/**
 * Tests for Variable Interpolation Service
 * 
 * Run these tests with: npm test
 */

import {
  interpolateVariables,
  safeInterpolateVariables,
  extractVariableNames,
  validateVariables
} from './variableInterpolation';

describe('interpolateVariables', () => {
  test('should interpolate single variable', () => {
    const result = interpolateVariables('Hello ${name}!', { name: 'World' });
    expect(result).toBe('Hello World!');
  });

  test('should interpolate multiple variables', () => {
    const result = interpolateVariables(
      'Hello ${name}, you are ${age} years old',
      { name: 'John', age: 30 }
    );
    expect(result).toBe('Hello John, you are 30 years old');
  });

  test('should handle variables with spaces', () => {
    const result = interpolateVariables('${ greeting } ${name}!', { greeting: 'Hi', name: 'Alice' });
    expect(result).toBe('Hi Alice!');
  });

  test('should throw error for missing variable', () => {
    expect(() => {
      interpolateVariables('Hello ${name}!', {});
    }).toThrow("Variable 'name' not found in variables object");
  });

  test('should throw error for multiple missing variables', () => {
    expect(() => {
      interpolateVariables('${greeting} ${name}!', {});
    }).toThrow("Variables 'greeting', 'name' not found in variables object");
  });

  test('should handle empty string', () => {
    const result = interpolateVariables('', {});
    expect(result).toBe('');
  });

  test('should handle string without variables', () => {
    const result = interpolateVariables('Hello World!', {});
    expect(result).toBe('Hello World!');
  });

  test('should convert numbers to strings', () => {
    const result = interpolateVariables('Value: ${value}', { value: 42 });
    expect(result).toBe('Value: 42');
  });

  test('should handle null values as empty string', () => {
    const result = interpolateVariables('Value: ${value}', { value: null });
    expect(result).toBe('Value: ');
  });

  test('should throw error if template is not a string', () => {
    expect(() => {
      interpolateVariables(123, {});
    }).toThrow('Template must be a string');
  });

  test('should throw error if variables is not an object', () => {
    expect(() => {
      interpolateVariables('Hello ${name}', 'not an object');
    }).toThrow('Variables must be an object');
  });
});

describe('safeInterpolateVariables', () => {
  test('should interpolate existing variables', () => {
    const result = safeInterpolateVariables('Hello ${name}!', { name: 'World' });
    expect(result).toBe('Hello World!');
  });

  test('should use default value for missing variables', () => {
    const result = safeInterpolateVariables('Hello ${name}!', {}, 'Unknown');
    expect(result).toBe('Hello Unknown!');
  });

  test('should use empty string as default when not specified', () => {
    const result = safeInterpolateVariables('Hello ${name}!', {});
    expect(result).toBe('Hello !');
  });

  test('should handle mixed existing and missing variables', () => {
    const result = safeInterpolateVariables(
      '${greeting} ${name}!',
      { greeting: 'Hi' },
      'Unknown'
    );
    expect(result).toBe('Hi Unknown!');
  });

  test('should return original string if template is not a string', () => {
    const result = safeInterpolateVariables(123, {});
    expect(result).toBe(123);
  });

  test('should return original string if variables is not an object', () => {
    const result = safeInterpolateVariables('Hello ${name}', 'not an object');
    expect(result).toBe('Hello ${name}');
  });
});

describe('extractVariableNames', () => {
  test('should extract single variable name', () => {
    const result = extractVariableNames('Hello ${name}!');
    expect(result).toEqual(['name']);
  });

  test('should extract multiple variable names', () => {
    const result = extractVariableNames('${greeting} ${name}, you are ${age}');
    expect(result).toEqual(['greeting', 'name', 'age']);
  });

  test('should handle duplicate variables', () => {
    const result = extractVariableNames('${name} and ${name}');
    expect(result).toEqual(['name']);
  });

  test('should trim variable names', () => {
    const result = extractVariableNames('${ name } and ${ age }');
    expect(result).toEqual(['name', 'age']);
  });

  test('should return empty array for string without variables', () => {
    const result = extractVariableNames('Hello World!');
    expect(result).toEqual([]);
  });

  test('should return empty array for non-string input', () => {
    const result = extractVariableNames(123);
    expect(result).toEqual([]);
  });
});

describe('validateVariables', () => {
  test('should return valid for all variables present', () => {
    const result = validateVariables('Hello ${name}!', { name: 'World' });
    expect(result).toEqual({ valid: true, missing: [] });
  });

  test('should return invalid for missing variables', () => {
    const result = validateVariables('${greeting} ${name}!', { greeting: 'Hi' });
    expect(result).toEqual({ valid: false, missing: ['name'] });
  });

  test('should return valid for string without variables', () => {
    const result = validateVariables('Hello World!', {});
    expect(result).toEqual({ valid: true, missing: [] });
  });

  test('should handle multiple missing variables', () => {
    const result = validateVariables('${a} ${b} ${c}', { b: 'value' });
    expect(result).toEqual({ valid: false, missing: ['a', 'c'] });
  });
});
