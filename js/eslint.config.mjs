import js from '@eslint/js';
import globals from 'globals';

export default [
  {
    basePath: '..',
    files: ['**/*.{js,mjs,cjs,jsx}'],
    ...js.configs.recommended,
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      parserOptions: { ecmaFeatures: { jsx: true } },
      globals: { ...globals.node, ...globals.browser, Bun: 'readonly', Deno: 'readonly' },
    },
    rules: {
      ...js.configs.recommended.rules,
      'max-lines': ['error', 1000],
      'no-unused-vars': ['error', { argsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' }],
    },
  },
];
