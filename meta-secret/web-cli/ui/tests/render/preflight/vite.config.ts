import { fileURLToPath, URL } from 'node:url';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
export default defineConfig({
  root: fileURLToPath(new URL('../../..', import.meta.url)),
  plugins: [vue()],
  resolve: { alias: { '@': fileURLToPath(new URL('../../../src', import.meta.url)) } },
  server: { host: '127.0.0.1', port: 5181, strictPort: true },
});
