import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { VitePWA } from 'vite-plugin-pwa';

const version = process.env.npm_package_version ?? '0.1.0';

export default defineConfig({
  define: {
    // Shown in About and quotable in a support question, so a version never has to be guessed.
    __APP_VERSION__: JSON.stringify(version),
    __BUILD_TIME__: JSON.stringify(new Date().toISOString()),
  },
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      // The worker is written by hand rather than generated: it has to handle the Android share
      // target, which is a POST, and no generated strategy covers that.
      strategies: 'injectManifest',
      srcDir: 'src',
      filename: 'sw.ts',

      // Never "autoUpdate": a new worker waits for the app to say the moment is safe, which it
      // will not do while a live session is running.
      registerType: 'prompt',
      injectRegister: null,

      injectManifest: {
        globPatterns: ['**/*.{js,css,html,woff2,png,svg}'],
      },

      manifest: {
        id: '/',
        name: 'Aurum Presenter',
        short_name: 'Aurum',
        description: 'Chord charts, sheet music and live presentation — offline first.',
        start_url: '/library',
        display: 'standalone',
        display_override: ['window-controls-overlay', 'standalone'],
        orientation: 'any',
        theme_color: '#0f172a',
        background_color: '#0f172a',
        categories: ['music', 'productivity'],
        icons: [
          { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
          { src: '/icon-512-maskable.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
          { src: '/icon-mono.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'monochrome' },
        ],
        shortcuts: [
          { name: 'Sets', url: '/sets' },
          { name: 'Library', url: '/library' },
          { name: 'Join a session', url: '/join' },
        ],
        file_handlers: [
          {
            action: '/share',
            accept: {
              'text/plain': ['.txt', '.pro', '.chopro', '.cho', '.crd'],
              'application/pdf': ['.pdf'],
            },
          },
        ],
        share_target: {
          action: '/share',
          method: 'POST',
          enctype: 'multipart/form-data',
          params: {
            files: [
              { name: 'files', accept: ['text/plain', 'application/pdf', '.chopro', '.pro', '.cho'] },
            ],
          },
        },
      },
    }),
  ],
  server: {
    port: 5173,
  },
});
