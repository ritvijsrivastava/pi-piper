// Minimal app-shell cache so the PWA opens instantly on the phone. This
// only caches static assets; chat data always goes over a live WebSocket
// and is never cached (there is no meaningful "offline" mode for a
// remote-control UI).
//
// Network-first, falling back to cache: a pure cache-first strategy
// means every future deploy is invisible to an already-installed PWA
// forever, since the fetch handler would keep serving the stale cached
// copy no matter how many times the page is reloaded (only an update to
// this file's own bytes makes the browser re-check it at all, and even
// then the old cache entries would otherwise just sit there unused but
// never refreshed). Network-first means a deploy is picked up on the
// very next reload, with the cache only kicking in when actually
// offline. Bump CACHE_NAME on a breaking app-shell change if you ever
// need to force-evict old entries immediately.
const CACHE_NAME = "pi-piper-shell-v7";
const APP_SHELL = [
  "/",
  "/style.css",
  "/markdown.js",
  "/attachments.js",
  "/app.js",
  "/manifest.json",
  "/icons/icon.svg",
  "/icons/icon-192.png",
  "/icons/icon-512.png",
  "/icons/sprite.svg",
  "/fonts/inter-variable.woff2",
  "/fonts/jetbrains-mono-variable.woff2",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(APP_SHELL)),
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key)),
        ),
      ),
  );
  self.clients.claim();
});

self.addEventListener("fetch", (event) => {
  // Never intercept the WebSocket upgrade request or non-GET requests.
  if (event.request.method !== "GET") return;

  event.respondWith(
    fetch(event.request)
      .then((response) => {
        const copy = response.clone();
        caches.open(CACHE_NAME).then((cache) => cache.put(event.request, copy));
        return response;
      })
      .catch(() => caches.match(event.request)),
  );
});
