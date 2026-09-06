/// <reference lib="webworker" />
import { cleanupOutdatedCaches, createHandlerBoundToURL, precacheAndRoute } from 'workbox-precaching';
import { NavigationRoute, registerRoute } from 'workbox-routing';
import { CacheFirst, NetworkFirst } from 'workbox-strategies';

/**
 * The service worker.
 *
 * It precaches the app shell and does nothing clever with data: IndexedDB is the cache, and an
 * HTTP cache in front of it would fight the sync watermark by serving deltas the local database
 * has already applied.
 *
 * It never activates itself. A musician mid-song must not have the page reload underneath them,
 * so a new worker waits until the app says the moment is safe (business rule 1).
 */
declare const self: ServiceWorkerGlobalScope;

const SHARE_CACHE = 'aurum-shared-files';

precacheAndRoute(self.__WB_MANIFEST);
cleanupOutdatedCaches();

// The document itself is network-first with a short timeout, so a new build is picked up
// quickly when there is a connection and instantly from cache when there is not.
registerRoute(
  new NavigationRoute(
    new NetworkFirst({ cacheName: 'aurum-shell', networkTimeoutSeconds: 2 }),
    { denylist: [/^\/api\//] },
  ),
);

registerRoute(
  ({ request }) => request.destination === 'font' || request.destination === 'image',
  new CacheFirst({ cacheName: 'aurum-static' }),
);

/**
 * The Android share target. A POST cannot be served by static hosting, so the worker takes the
 * files, parks them in a cache, and redirects to a page that knows what to do with them.
 */
self.addEventListener('fetch', (event: FetchEvent) => {
  const url = new URL(event.request.url);

  if (event.request.method !== 'POST' || url.pathname !== '/share') {
    return;
  }

  event.respondWith((async () => {
    try {
      const form = await event.request.formData();
      const files = form.getAll('files').filter((entry): entry is File => entry instanceof File);
      const cache = await caches.open(SHARE_CACHE);

      await Promise.all(files.map((file, index) => cache.put(
        `/shared/${index}-${encodeURIComponent(file.name)}`,
        new Response(file, { headers: { 'content-type': file.type || 'application/octet-stream' } }),
      )));
    } catch {
      // A share that cannot be read is not worth failing the navigation for; the page will
      // simply find nothing waiting.
    }

    return Response.redirect('/share?from=share-target', 303);
  })());
});

self.addEventListener('message', (event: ExtendableMessageEvent) => {
  // The only way a new worker takes over: the app asked, having decided it is safe.
  if ((event.data as { type?: string } | null)?.type === 'SKIP_WAITING') {
    void self.skipWaiting();
  }
});

// Kept for the app shell's own reference; `createHandlerBoundToURL` is what NavigationRoute
// uses when the precached document is the right answer.
void createHandlerBoundToURL;
