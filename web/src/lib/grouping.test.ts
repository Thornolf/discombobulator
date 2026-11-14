import { describe, expect, it } from "vitest";
import { photo } from "./fixtures";
import { groupScenes } from "./grouping";

const ids = (stacks: { id: string }[][]) => stacks.map((stack) => stack.map((p) => p.id));

describe("groupScenes", () => {
  it("starts a new scene after more than five minutes", () => {
    const scenes = groupScenes([photo("1", 0), photo("2", 5 * 60_000), photo("3", 10 * 60_000 + 1)]);
    expect(scenes.map((scene) => scene.photos.map((p) => p.id))).toEqual([["1", "2"], ["3"]]);
  });

  it("stacks shots at most 2.5 s apart", () => {
    const scenes = groupScenes([photo("1", 0), photo("2", 2_500), photo("3", 6_000), photo("4", 6_400)]);
    expect(ids(scenes[0]?.stacks ?? [])).toEqual([
      ["1", "2"],
      ["3", "4"],
    ]);
  });

  it("stacks look-alike frames even when further apart", () => {
    const scenes = groupScenes([
      photo("1", 0, { dhash: "ffffffffffffffff" }),
      photo("2", 30_000, { dhash: "fffffffffffff000" }),
      photo("3", 60_000, { dhash: "0000000000000000" }),
    ]);
    expect(ids(scenes[0]?.stacks ?? [])).toEqual([["1", "2"], ["3"]]);
  });

  it("sorts by time then by name", () => {
    const scenes = groupScenes([photo("10", 0), photo("9", 0), photo("1", -1)]);
    expect(scenes[0]?.photos.map((p) => p.id)).toEqual(["1", "9", "10"]);
  });

  it("returns nothing for no photos", () => {
    expect(groupScenes([])).toEqual([]);
  });
});
