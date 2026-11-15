import { describe, expect, it } from "vitest";
import { exposureBias, exposureLine, focalLine } from "./shotinfo.svelte";
import type { ShotInfo } from "./types";

const info = (fields: Partial<ShotInfo>): ShotInfo => ({
  camera: null,
  lens: null,
  focal_length: null,
  focal_length_35mm: null,
  exposure_time: null,
  f_number: null,
  iso: null,
  exposure_bias: null,
  program: null,
  film_simulation: null,
  dynamic_range: null,
  grain: null,
  focus_mode: null,
  af_area: null,
  focus_point: null,
  faces: [],
  focus_warning: null,
  blur_warning: null,
  exposure_warning: null,
  ...fields,
});

describe("shot info text", () => {
  it("joins what is known about the exposure", () => {
    expect(exposureLine(info({ exposure_time: "1/125", f_number: 2, iso: 3200 }))).toBe(
      "1/125 s · f/2 · ISO 3200",
    );
    expect(exposureLine(info({ f_number: 5.6 }))).toBe("f/5.6");
    expect(exposureLine(info({}))).toBe("");
  });

  it("writes exposure compensation in thirds", () => {
    expect(exposureBias(-1 / 3)).toBe("−⅓ EV");
    expect(exposureBias(1 + 2 / 3)).toBe("+1⅔ EV");
    expect(exposureBias(-2)).toBe("−2 EV");
    expect(exposureBias(0)).toBe("0 EV");
    expect(exposureBias(null)).toBeNull();
  });

  it("adds the 35 mm equivalent when it differs", () => {
    expect(focalLine(info({ focal_length: 23, focal_length_35mm: 35 }))).toBe("23 mm (35 mm eq.)");
    expect(focalLine(info({ focal_length: 50, focal_length_35mm: 50 }))).toBe("50 mm");
  });
});
