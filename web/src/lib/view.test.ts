import { describe, expect, it } from "vitest";
import { defaultView, restoreView } from "./view.svelte";

describe("view options", () => {
  it("keeps only known on/off settings", () => {
    expect(restoreView(null)).toEqual(defaultView);
    expect(restoreView({ histogram: false, clipping: "yes", other: true })).toEqual({
      ...defaultView,
      histogram: false,
    });
  });
});
