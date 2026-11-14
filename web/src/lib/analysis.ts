export const BINS = 64;
const HIGHLIGHT = 254;
const SHADOW = 1;
const ANALYSIS_EDGE = 1024;
const CACHE_LIMIT = 48;

export interface Histogram {
  red: number[];
  green: number[];
  blue: number[];
  luma: number[];
}

export interface Analysis {
  histogram: Histogram;
  highlights: number;
  shadows: number;
  width: number;
  height: number;
  mask: Uint8ClampedArray;
}

const HIGHLIGHT_COLOR = [236, 72, 153, 210] as const;
const SHADOW_COLOR = [59, 130, 246, 210] as const;

function count(bins: number[], value: number) {
  const index = Math.min(BINS - 1, Math.floor((value * BINS) / 256));
  bins[index] = (bins[index] ?? 0) + 1;
}

export function analyzePixels(pixels: Uint8ClampedArray, width: number, height: number): Analysis {
  const empty = () => new Array<number>(BINS).fill(0);
  const histogram: Histogram = { red: empty(), green: empty(), blue: empty(), luma: empty() };
  const mask = new Uint8ClampedArray(width * height * 4);
  let highlights = 0;
  let shadows = 0;
  const total = width * height;
  for (let index = 0; index < total; index += 1) {
    const at = index * 4;
    const red = pixels[at] ?? 0;
    const green = pixels[at + 1] ?? 0;
    const blue = pixels[at + 2] ?? 0;
    const luma = Math.round(0.2126 * red + 0.7152 * green + 0.0722 * blue);
    count(histogram.red, red);
    count(histogram.green, green);
    count(histogram.blue, blue);
    count(histogram.luma, luma);
    const color =
      Math.max(red, green, blue) >= HIGHLIGHT
        ? HIGHLIGHT_COLOR
        : Math.max(red, green, blue) <= SHADOW
          ? SHADOW_COLOR
          : null;
    if (color === HIGHLIGHT_COLOR) highlights += 1;
    if (color === SHADOW_COLOR) shadows += 1;
    if (color) mask.set(color, at);
  }
  return {
    histogram,
    highlights: total ? highlights / total : 0,
    shadows: total ? shadows / total : 0,
    width,
    height,
    mask,
  };
}

export function histogramPath(bins: number[], width: number, height: number): string {
  const peak = Math.max(1, ...bins.map((value) => Math.sqrt(value)));
  const step = width / Math.max(1, bins.length - 1);
  const points = bins.map(
    (value, index) =>
      `L${(index * step).toFixed(1)},${(height - (Math.sqrt(value) / peak) * height).toFixed(1)}`,
  );
  return `M0,${height} ${points.join(" ")} L${width},${height} Z`;
}

export function percent(share: number): string {
  if (share === 0) return "0 %";
  if (share < 0.001) return "< 0.1 %";
  return `${(share * 100).toFixed(1)} %`;
}

const cache = new Map<string, Analysis>();

export function analyzeImage(key: string, image: HTMLImageElement): Analysis | null {
  const known = cache.get(key);
  if (known) return known;
  if (!image.naturalWidth || !image.naturalHeight) return null;
  const ratio = Math.min(1, ANALYSIS_EDGE / Math.max(image.naturalWidth, image.naturalHeight));
  const width = Math.max(1, Math.round(image.naturalWidth * ratio));
  const height = Math.max(1, Math.round(image.naturalHeight * ratio));
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return null;
  try {
    context.drawImage(image, 0, 0, width, height);
    const analysis = analyzePixels(context.getImageData(0, 0, width, height).data, width, height);
    cache.set(key, analysis);
    if (cache.size > CACHE_LIMIT) {
      const oldest = cache.keys().next().value;
      if (oldest !== undefined) cache.delete(oldest);
    }
    return analysis;
  } catch {
    return null;
  }
}
