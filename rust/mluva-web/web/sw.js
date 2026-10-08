"use strict";

// Network only: Cloudflare Access checks every request, including installed launches.
// Do not cache recordings, transcripts, authenticated pages or sign-in responses.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (event) => event.respondWith(fetch(event.request)));
