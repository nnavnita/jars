// Offline shell for jars.
//
// Trunk fingerprints the wasm and JS filenames, so a new build produces new
// URLs and the old cache is simply dropped wholesale on activate. That makes
// stale-asset bugs impossible at the cost of one re-download per deploy.
const CACHE = 'jars-v1';

self.addEventListener('install', (event) => {
  self.skipWaiting();
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(['./', './manifest.webmanifest'])));
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET' || new URL(request.url).origin !== self.location.origin) return;

  // Navigations go to the network first so a deploy is picked up promptly,
  // falling back to the cached shell when offline.
  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request)
        .then((response) => {
          const copy = response.clone();
          caches.open(CACHE).then((c) => c.put('./', copy));
          return response;
        })
        .catch(() => caches.match('./'))
    );
    return;
  }

  // Fingerprinted assets never change under the same URL, so cache-first.
  event.respondWith(
    caches.match(request).then((hit) => hit || fetch(request).then((response) => {
      if (response.ok) {
        const copy = response.clone();
        caches.open(CACHE).then((c) => c.put(request, copy));
      }
      return response;
    }))
  );
});
