import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      workbox: {
        // The app shell is precached. API responses deliberately are NOT: IndexedDB is the
        // cache, and an HTTP cache sitting in front of it would fight the sync watermark by
        // serving deltas the local database has already applied.
        globPatterns: ['**/*.{js,css,html,woff2}'],
        navigateFallback: '/index.html',
        runtimeCaching: [],
      },
      manifest: {
        name: 'Aurum Presenter',
        short_name: 'Aurum',
        description: 'Chord charts, sheet music and live presentation — offline first.',
        theme_color: '#0f172a',
        background_color: '#0f172a',
        display: 'standalone',
        orientation: 'any',
        start_url: '/',
      },
    }),
  ],
  server: {
    port: 5173,
  },
});
