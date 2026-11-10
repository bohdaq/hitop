// ESLint for the web app (npm run lint; CI job "lint"): the recommended rules, Lit's template
// rules, browser ES modules for src/, Node CommonJS for the e2e suites and Node ESM for scripts.
import js from "@eslint/js";
import lit from "eslint-plugin-lit";
import globals from "globals";

export default [
  { ignores: ["dist/", "dist-tauri/", "dist-extension/", "src/wasm/pkg/", "src/wasm/pkg-scripts/", "node_modules/"] },
  js.configs.recommended,
  {
    linterOptions: { reportUnusedDisableDirectives: "error" },
    rules: {
      // `_name` marks an argument or binding kept on purpose (a callback's position, a rest split).
      "no-unused-vars": ["error", { args: "after-used", argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrors: "none", ignoreRestSiblings: true }],
      "no-empty": ["error", { allowEmptyCatch: true }],
      eqeqeq: ["error", "always", { null: "ignore" }],
      "prefer-const": "error",
      "no-var": "error",
    },
  },
  {
    files: ["src/**/*.js", "public/**/*.js"],
    ...lit.configs["flat/recommended"],
    languageOptions: { ecmaVersion: 2024, sourceType: "module", globals: { ...globals.browser } },
  },
  { files: ["src/scripts-worker.js"], languageOptions: { globals: { ...globals.worker } } },
  // The e2e suites run in Node, and the functions they hand to page.evaluate() run in the page.
  { files: ["tests/**/*.cjs"], languageOptions: { ecmaVersion: 2024, sourceType: "commonjs", globals: { ...globals.node, ...globals.browser } } },
  { files: ["tests/**/*.mjs", "scripts/**/*.mjs", "*.config.js", "eslint.config.js"], languageOptions: { ecmaVersion: 2024, sourceType: "module", globals: { ...globals.node } } },
];
