import { previewUrl, thumbUrl } from "./api";
import type { Photo } from "./types";

const photosCache = "discombobulator-photos-v1";

export function registerServiceWorker(): void {
  if ("serviceWorker" in navigator && window.isSecureContext) {
    navigator.serviceWorker.register("/sw.js").catch(() => undefined);
  }
}

export async function cachedCount(photos: Photo[]): Promise<number> {
  if (!("caches" in window)) {
    return 0;
  }
  const cache = await caches.open(photosCache);
  const keys = new Set(
    (await cache.keys()).map((request) => new URL(request.url).pathname + new URL(request.url).search),
  );
  return photos.filter((photo) => keys.has(previewUrl(photo))).length;
}

export async function downloadForOffline(
  photos: Photo[],
  onprogress: (done: number, total: number) => void,
  signal?: AbortSignal,
): Promise<number> {
  if (!("caches" in window)) {
    throw new Error("This browser cannot keep photos offline");
  }
  const cache = await caches.open(photosCache);
  await cache.add("/api/photos");
  const urls = photos.flatMap((photo) => [thumbUrl(photo), previewUrl(photo)]);
  let done = 0;
  let failed = 0;
  const queue = [...urls];
  const worker = async () => {
    for (let url = queue.shift(); url !== undefined; url = queue.shift()) {
      if (signal?.aborted) {
        return;
      }
      try {
        if (!(await cache.match(url))) {
          await cache.add(url);
        }
      } catch {
        failed += 1;
      }
      done += 1;
      onprogress(done, urls.length);
    }
  };
  await Promise.all(Array.from({ length: 4 }, worker));
  return failed;
}

export async function clearOffline(): Promise<void> {
  if ("caches" in window) {
    await caches.delete(photosCache);
  }
}
