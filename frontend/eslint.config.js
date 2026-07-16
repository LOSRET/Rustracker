import { defineConfig } from "eslint/config";
import vue from "eslint-plugin-vue";
import ts from "@vue/eslint-config-typescript";
import prettier from "@vue/eslint-config-prettier";

export default defineConfig([
  {
    name: "app/files-to-lint",
    files: ["**/*.{ts,mts,tsx,vue}"],
  },

  {
    name: "app/files-to-ignore",
    ignores: ["**/dist/**", "**/dist-ssr/**", "**/node_modules/**", "**/auto-imports.d.ts", "**/components.d.ts"],
  },

  ...vue.configs["flat/recommended"],
  ...ts(),

  prettier,

  {
    name: "app/vue-rules",
    rules: {
      "vue/multi-word-component-names": "off",
      "vue/no-setup-props-reactivity-loss": "warn",
    },
  },
]);
