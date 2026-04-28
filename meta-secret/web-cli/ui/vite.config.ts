import { fileURLToPath, URL } from 'url';
import { readFileSync } from 'fs';

import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import vueJsx from '@vitejs/plugin-vue-jsx';

import wasm from 'vite-plugin-wasm';

const packageJsonPath = fileURLToPath(new URL('./package.json', import.meta.url));
const packageVersion = JSON.parse(readFileSync(packageJsonPath, 'utf-8')).version || '0.0.0';
const appVersion = process.env.APP_VERSION || packageVersion;
const appCommit = process.env.APP_COMMIT || 'unknown';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue(), vueJsx(), wasm()],
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
    __APP_COMMIT__: JSON.stringify(appCommit),
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
});
