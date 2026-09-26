// @ts-check
import { defineConfig } from 'astro/config';
import react from '@astrojs/react';
import sitemap from '@astrojs/sitemap';

import tailwindcss from '@tailwindcss/vite';

// https://astro.build/config
// GitHub Pages target: https://bethropolis.github.io/podbox/
// Static output: every route prerenders at build time, no adapter.
export default defineConfig({
  output: 'static',
  site: 'https://bethropolis.github.io',
  base: '/podbox',
  integrations: [react(), sitemap()],
  vite: {
    plugins: [tailwindcss()]
  },
  markdown: {
    shikiConfig: {
      theme: 'github-dark',
    },
  },
});