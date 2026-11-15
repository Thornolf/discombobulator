import { describe, expect, it } from "vitest";
import { BINS, analyzePixels, histogramPath, percent } from "./analysis";

function image(colors: [number, number, number][]): Uint8ClampedArray {
  return new Uint8ClampedArray(colors.flatMap(([red, green, blue]) => [red, green, blue, 255]));
}

describe("analysis", () => {
  it("counts every channel into the histogram", () => {
    const result = analyzePixels(
      image([
        [0, 128, 255],
        [255, 255, 255],
      ]),
      2,
      1,
    );
    expect(result.histogram.red[0]).toBe(1);
    expect(result.histogram.red[BINS - 1]).toBe(1);
    expect(result.histogram.green[BINS / 2]).toBe(1);
    expect(result.histogram.blue[BINS - 1]).toBe(2);
    expect(result.histogram.luma.reduce((sum, value) => sum + value, 0)).toBe(2);
  });

  it("marks clipped highlights and crushed shadows", () => {
    const result = analyzePixels(
      image([
        [255, 200, 10],
        [0, 1, 0],
        [120, 120, 120],
        [30, 30, 30],
      ]),
      2,
      2,
    );
    expect(result.highlights).toBe(0.25);
    expect(result.shadows).toBe(0.25);
    expect(result.mask[3]).toBeGreaterThan(0);
    expect(result.mask[7]).toBeGreaterThan(0);
    expect(result.mask[11]).toBe(0);
    expect(result.mask[15]).toBe(0);
  });

  it("draws a closed path across the full width", () => {
    const path = histogramPath([0, 4, 16, 0], 300, 100);
    expect(path.startsWith("M0,100 ")).toBe(true);
    expect(path.endsWith("L300,100 Z")).toBe(true);
    expect(path).toContain("L200.0,0.0");
  });

  it("formats clipped shares", () => {
    expect(percent(0)).toBe("0 %");
    expect(percent(0.0004)).toBe("< 0.1 %");
    expect(percent(0.021)).toBe("2.1 %");
  });
});
