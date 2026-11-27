// Plugin calls (`${plugin.name(args)}`, spec section 5c) in what a request sends, and their answers
// filled in. Pure: the store asks the plugins runner (platform().plugins) for the answers first, then
// adds them to the variables a request sees, keyed by the call's text, as resolved secret manager
// references are. A call with no answer there is still refused by the interpolators (part 1), so a
// path that forgot to ask never sends the call as text.

// Keys that aren't filled in when a request is sent (features.js NOT_FILLED).
const NOT_FILLED = new Set(["preRequestScript", "postRequestScript", "examples", "scripts"]);
const SLOT = /\$\{([^}]+)\}/g;
const LOOKS_LIKE_CALL = /^plugin\.[^(]*\(/;

/** The trimmed inner texts of `${plugin.…(…)}` in a value (strings, arrays, objects). */
export function pluginCallTexts(value, out = new Set()) {
  if (typeof value === "string") {
    for (const m of value.matchAll(SLOT)) {
      const inner = m[1].trim();
      if (LOOKS_LIKE_CALL.test(inner)) out.add(inner);
    }
  } else if (Array.isArray(value)) value.forEach((v) => pluginCallTexts(v, out));
  else if (value && typeof value === "object") for (const [k, v] of Object.entries(value)) if (!NOT_FILLED.has(k)) pluginCallTexts(v, out);
  return out;
}

/**
 * The argument values of a parsed call (hitop-wasm pluginCall: {plugin, args: [{variable} | {text}]}),
 * or throws naming a variable with no value (as `${variable}` does).
 */
export function callArguments(call, variables) {
  return call.args.map((a) => {
    if ("text" in a) return a.text;
    const v = variables[a.variable];
    if (v === undefined || v === null) throw new Error(`Variable '${a.variable}' not found in variables object`);
    return String(v);
  });
}

/** True when an answer was computed from a secret (threat T11): an argument holds a secret value. */
export const fromSecret = (args, secrets) => secrets.some((s) => typeof s === "string" && s !== "" && args.some((a) => a.includes(s)));

/** Variables with plugin answers added (by call text) and filled in inside other variables' values. */
export function withPluginValues(variables, answers) {
  if (!answers || !Object.keys(answers).length) return variables;
  const out = { ...answers, ...variables };
  for (const [k, v] of Object.entries(out)) {
    if (typeof v === "string" && v.includes("${plugin.")) out[k] = v.replace(SLOT, (whole, inner) => (inner.trim() in answers ? answers[inner.trim()] : whole));
  }
  return out;
}

/**
 * The calls that pass a plugin a secret (threat T1: the editor warns before sending), by the same tests
 * as masking: an argument that is a secret variable by name, or whose value holds a secret value (T11).
 * `calls` = [{text, call}] (call parsed by hitop-wasm pluginCall); answers [{text, plugin}].
 */
export function secretPluginCalls(calls, variables, secretNames, secretValues) {
  const names = new Set(secretNames);
  return calls
    .filter(({ call }) => {
      if (call.args.some((a) => "variable" in a && names.has(a.variable))) return true;
      const values = call.args.map((a) => ("text" in a ? a.text : variables[a.variable])).filter((v) => typeof v === "string");
      return fromSecret(values, secretValues);
    })
    .map(({ text, call }) => ({ text, plugin: call.plugin }));
}
