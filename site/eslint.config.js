// ESLint for the site (npm run lint; CI job "lint"): src/ runs in the browser and, prerendered,
// in Node (Lit SSR), so both sets of globals; scripts/ and the config files are Node ESM.
import js from "@eslint/js";
import lit from "eslint-plugin-lit";
import globals from "globals";

export default [
  { ignores: ["dist/", "node_modules/", "public-assets/"] },
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
    languageOptions: { ecmaVersion: 2024, sourceType: "module", globals: { ...globals.browser, ...globals.node } },
  },
  { files: ["scripts/**/*.{js,mjs}", "*.config.js", "eslint.config.js"], languageOptions: { ecmaVersion: 2024, sourceType: "module", globals: { ...globals.node } } },
];
