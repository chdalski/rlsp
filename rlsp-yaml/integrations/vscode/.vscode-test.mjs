import { defineConfig } from '@vscode/test-cli';

export default defineConfig({
  files: 'out/src/test/integration/**/*.test.js',
  mocha: { timeout: 20000 },
});
