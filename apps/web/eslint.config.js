// Flat ESLint config for the web app.
// Ref: https://eslint.org/docs/latest/use/configure/configuration-files
import eslint from '@eslint/js'
import prettier from 'eslint-config-prettier'
import vue from 'eslint-plugin-vue'
import tseslint from 'typescript-eslint'

export default tseslint.config(
  {
    // Global ignores. `dist/` and `node_modules/` are covered by ESLint
    // defaults but listing them explicitly is clearer.
    ignores: ['dist/**', 'node_modules/**', 'coverage/**'],
  },
  eslint.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  ...vue.configs['flat/recommended'],
  {
    files: ['**/*.{ts,vue}'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
        project: './tsconfig.json',
        extraFileExtensions: ['.vue'],
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // Vue 3 no longer requires multi-word component names for top-level views.
      'vue/multi-word-component-names': 'off',
      // The `unused-vars` rule from `@typescript-eslint` is already active.
      'no-unused-vars': 'off',
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
    },
  },
  {
    // Config files run under Node and are not part of the TS project graph.
    files: ['*.config.{js,ts}', '*.config.*.js', 'postcss.config.js'],
    languageOptions: {
      parserOptions: { project: null },
    },
    ...tseslint.configs.disableTypeChecked,
  },
  prettier,
)
