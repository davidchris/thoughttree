// @ts-check
import js from "@eslint/js";
import tseslint from "typescript-eslint";

const TEST_FILES = ["**/*.test.{ts,tsx}", "**/*.spec.{ts,tsx}"];

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "target/**",
      ".sidecar-build/**",
      "**/vitest.config.ts",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["packages/*/src/**/*.ts"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
          destructuredArrayIgnorePattern: "^_",
          ignoreRestSiblings: true,
        },
      ],

      // Nesting and complexity budgets. Warnings for now; promote after cleanup.
      "max-depth": ["warn", 3],
      complexity: ["warn", { max: 20, variant: "modified" }],
      "no-else-return": ["warn", { allowElseIf: true }],

      // Async correctness (type-aware).
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
    },
  },
  {
    // Tests: keep the async checks, drop the complexity budget.
    files: TEST_FILES,
    rules: {
      "max-depth": "off",
      complexity: "off",
      "no-else-return": "off",
    },
  },
);
