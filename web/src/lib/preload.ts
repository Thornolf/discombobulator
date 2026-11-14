const warm = new Map<string, HTMLImageElement>();
const capacity = 24;

export function warmUp(url: string): void {
  const existing = warm.get(url);
  if (existing) {
    warm.delete(url);
    warm.set(url, existing);
    return;
  }
  const image = new Image();
  image.decoding = "async";
  image.src = url;
  image.decode().catch(() => undefined);
  warm.set(url, image);
  while (warm.size > capacity) {
    const oldest = warm.keys().next().value;
    if (oldest === undefined) {
      break;
    }
    warm.delete(oldest);
  }
}
