// SilverWave Offline-First PWA Service Worker (06-SPEC §3.5.3)
const CACHE_VERSION = "silverwave-pwa-v1.0.0-main";
const SAMPLES_CAS_CACHE = "silverwave-samples-cas-v1";
const PRECACHE_ASSETS = [
  "/index.html",
  "/silverwave_bg.wasm",
  "/silverwave_worklet.js",
  "/manifest.webmanifest",
  "/akwf-core.swpack"
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_VERSION).then((cache) => cache.addAll(PRECACHE_ASSETS))
  );
  // Zero-Interruption Live Performance Rule: never call self.skipWaiting() automatically while AudioContext is running.
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key.startsWith("silverwave-pwa-v") && key !== CACHE_VERSION && key !== SAMPLES_CAS_CACHE)
          .map((key) => caches.delete(key))
      )
    )
  );
});
