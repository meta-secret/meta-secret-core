import { defineConfig } from 'cypress';

export default defineConfig({
  e2e: {
    setupNodeEvents(on, config) {},
    baseUrl: 'http://127.0.0.1:5050',
  },

  component: {
    devServer: {
      framework: 'vue',
      bundler: 'vite',
    },
  },
});
