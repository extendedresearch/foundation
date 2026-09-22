// Adopted from extendedresearch/foundation style/configs/eslint.config.mjs.
// S16 to S21. Flat config; ESLint 9 or later.
//
// Type-aware rules are on, so a repository sets `projectService` to its own
// tsconfig. Copy this file to the repository root as `eslint.config.mjs` and
// edit nothing but the `files` globs; `style/scripts/check-configs.py` compares
// everything else against foundation's copy.
import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  {
    languageOptions: {
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    rules: {
      // S19: no `any`. `unknown` plus narrowing instead.
      "@typescript-eslint/no-explicit-any": "error",
      // A disable comment states its reason on the same line.
      "@eslint-community/eslint-comments/require-description": "off",

      // S1, S2: casing, with acronyms as words. The linter cannot check that a
      // name matches its sibling in another language; it checks the shape.
      "@typescript-eslint/naming-convention": [
        "error",
        { selector: "default", format: ["camelCase"] },
        { selector: "variable", format: ["camelCase", "UPPER_CASE"] },
        { selector: "parameter", format: ["camelCase"], leadingUnderscore: "allow" },
        { selector: "typeLike", format: ["PascalCase"] },
        // S17: no `I` prefix on an interface.
        {
          selector: "interface",
          format: ["PascalCase"],
          custom: { regex: "^I[A-Z]", match: false },
        },
        { selector: "enumMember", format: ["UPPER_CASE", "PascalCase"] },
        { selector: "objectLiteralProperty", format: null },
      ],

      // S5: a conversion that allocates is named for it.
      "@typescript-eslint/no-unnecessary-condition": "error",
      "@typescript-eslint/consistent-type-definitions": ["error", "type"],
      "@typescript-eslint/explicit-module-boundary-types": "error",
      "no-console": "warn",
      eqeqeq: ["error", "always"],
    },
  },
  { ignores: ["**/dist/**", "**/node_modules/**", "**/target/**", "**/*.d.ts"] },
);
