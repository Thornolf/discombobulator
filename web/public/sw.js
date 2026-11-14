const SHELL = "discombobulator-shell-v1";
const PHOTOS = "discombobulator-photos-v1";
const SHELL_FILES = ["/", "/manifest.webmanifest", "/icon-192.png", "/icon-512.png"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(SHELL)
      .then((cache) => cache.addAll(SHELL_FILES))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== SHELL && key !== PHOTOS).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  );
});

async function cacheFirst(request, cacheName) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);
  if (cached) {
    return cached;
  }
  const response = await fetch(request);
  if (response.ok) {
    await cache.put(request, response.clone());
  }
  return response;
}

async function networkFirst(request, cacheName, fallbackUrl) {
  const cache = await caches.open(cacheName);
  try {
    const response = await fetch(request);
    if (response.ok) {
      await cache.put(fallbackUrl ?? request, response.clone());
    }
    return response;
  } catch {
    const cached = await cache.match(fallbackUrl ?? request);
    if (cached) {
      return cached;
    }
    return new Response(JSON.stringify({ error: "offline" }), {
      status: 503,
      headers: { "content-type": "application/json" },
    });
  }
}

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== self.location.origin) {
    return;
  }
  if (/^\/api\/photos\/[0-9a-f]{20}\/(thumb|preview)$/.test(url.pathname)) {
    event.respondWith(cacheFirst(request, PHOTOS));
    return;
  }
  if (url.pathname === "/api/photos") {
    event.respondWith(networkFirst(request, PHOTOS));
    return;
  }
  if (url.pathname.startsWith("/api/")) {
    return;
  }
  if (url.pathname.startsWith("/assets/")) {
    event.respondWith(cacheFirst(request, SHELL));
    return;
  }
  if (request.mode === "navigate") {
    event.respondWith(networkFirst(request, SHELL, "/"));
  }
});
