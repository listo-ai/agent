import unicorn from "eslint-plugin-unicorn";

/** @type {import("eslint").Linter.Config[]} */
export default [
  {
    plugins: { unicorn },
    rules: {
      // Mirror the 400-line file / 50-line function caps from CODE-LAYOUT rules.
      "max-lines": ["error", { max: 400, skipBlankLines: true, skipComments: true }],
      "max-lines-per-function": ["error", { max: 50, skipBlankLines: true, skipComments: true }],
      // No catch-all utils modules — name the module.
      "unicorn/prevent-abbreviations": "warn",
    },
  },
  {
    ignores: ["dist/**", "node_modules/**"],
  },
];
