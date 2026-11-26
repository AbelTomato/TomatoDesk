import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "artifacts", "src-tauri/target"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
      // Markdown 渲染不产生 innerHTML；出现这个属性说明有人绕过了 sanitize 链路。
      "no-restricted-syntax": [
        "error",
        {
          selector: "JSXAttribute[name.name='dangerouslySetInnerHTML']",
          message: "不要使用 dangerouslySetInnerHTML，Markdown 请走 react-markdown + sanitize。",
        },
      ],
    },
  },
  {
    files: ["vite.config.ts", "eslint.config.js", "scripts/**/*.mjs"],
    languageOptions: {
      globals: globals.node,
    },
  },
);
