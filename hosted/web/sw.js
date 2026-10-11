// Network-only: no authenticated text, audio or tokens in service-worker caches.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) =>
  event.waitUntil(self.clients.claim()),
);
self.addEventListener("fetch", (event) =>
  event.respondWith(fetch(event.request)),
);
