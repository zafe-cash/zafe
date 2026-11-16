import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://zafe.cash',
  output: 'static',
  build: {
    format: 'file',
    inlineStylesheets: 'never',
    assets: 'assets/_astro',
  },
  compressHTML: true,
  devToolbar: { enabled: false },
  prefetch: false,
  vite: {
    build: { assetsInlineLimit: 0 },
  },
});
