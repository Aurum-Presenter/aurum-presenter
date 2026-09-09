/**
 * Building the service worker.
 *
 * The worker is the one piece of the client that is still TypeScript, and deliberately: its job
 * is to answer `fetch` events, which WebAssembly cannot do any better, and the precache manifest
 * and update-hold behaviour were already exactly right. Workbox generates the manifest; esbuild
 * turns the worker into one file a browser can run.
 *
 * It runs after Trunk has written the distribution, because the list of files to precache is
 * exactly what Trunk just produced — hashed names and all.
 */

import { build } from 'esbuild';
import { injectManifest } from 'workbox-build';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const dist = process.env.TRUNK_STAGING_DIR ?? join(here, '..', 'crates', 'web', 'dist');

// Bundled first, with the placeholder still in it, so Workbox has one file to rewrite.
await build({
  entryPoints: [join(here, 'sw.ts')],
  outfile: join(dist, 'sw.js'),
  bundle: true,
  format: 'iife',
  target: 'es2022',
  minify: true,
  logLevel: 'error',
});

const { count, size, warnings } = await injectManifest({
  swSrc: join(dist, 'sw.js'),
  swDest: join(dist, 'sw.js'),
  globDirectory: dist,

  // Everything the shell needs to start with the radio off — and nothing else. The Pdfium
  // engine is four megabytes and is deliberately absent: a device that has never opened a sheet
  // has nothing to render, so it is fetched and runtime-cached on the first sheet instead.
  globPatterns: ['**/*.{js,css,html,wasm,woff2,png,svg,webmanifest}'],
  globIgnores: ['sw.js', 'pdfium/**'],

  // The shell's own WebAssembly is the largest thing here and the one thing that cannot be
  // left out: without it a device with no signal has a page and no application.
  maximumFileSizeToCacheInBytes: 24 * 1024 * 1024,
});

for (const warning of warnings) {
  console.warn(warning);
}

console.log(`service worker: ${count} files precached, ${Math.round(size / 1024)} KB`);
