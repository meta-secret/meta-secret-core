import { readFileSync } from 'node:fs';
import { fileURLToPath, URL } from 'node:url';

import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import vueJsx from '@vitejs/plugin-vue-jsx';
import tailwindcss from '@tailwindcss/vite';
import wasm from 'vite-plugin-wasm';

const pkgPath = fileURLToPath(new URL('./package.json', import.meta.url));
const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8')) as { version: string };

export default defineConfig({
  define: {
    __WEB_UI_VERSION__: JSON.stringify(pkg.version),
  },
  plugins: [tailwindcss(), vue(), vueJsx(), wasm()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  build: {
    rolldownOptions: {
      onLog(level, log, defaultHandler) {
        // @vueuse/core@14.3.0 — remove when vueuse ships fixed PURE annotations (vueuse#5387)
        if (log.code === 'INVALID_ANNOTATION') return;
        defaultHandler(level, log);
      },
    },
  },
});
