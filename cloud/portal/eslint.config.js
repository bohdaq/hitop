// ESLint for the HITOP Cloud portal (npm run lint; CI job "lint"): browser ES modules in src/,
// the e2e suite in Node CommonJS (the functions it hands to page.evaluate() run in the page).
import js from "@eslint/js";
import lit from "eslint-plugin-lit";
import globals from "globals";

export default [
  { ignores: ["dist/", "node_modules/", "vendor/", "public/"] },
  js.configs.recommended,
  {
    linterOptions: { reportUnusedDisableDirectives: "error" },
    rules: {
      "no-unused-vars": ["error", { args: "after-used", argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrors: "none", ignoreRestSiblings: true }],
      "no-empty": ["error", { allowEmptyCatch: true }],
      eqeqeq: ["error", "always", { null: "ignore" }],
      "prefer-const": "error",
      "no-var": "error",
    },
  },
  {
    files: ["src/**/*.js"],
    ...lit.configs["flat/recommended"],
    // __HITOP_COMMIT__: replaced at build time (vite.config.js `define`).
    languageOptions: { ecmaVersion: 2024, sourceType: "module", globals: { ...globals.browser, __HITOP_COMMIT__: "readonly" } },
  },
  { files: ["tests/**/*.cjs"], languageOptions: { ecmaVersion: 2024, sourceType: "commonjs", globals: { ...globals.node, ...globals.browser } } },
  { files: ["*.config.js", "eslint.config.js"], languageOptions: { ecmaVersion: 2024, sourceType: "module", globals: { ...globals.node } } },
];
