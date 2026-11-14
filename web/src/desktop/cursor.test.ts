import { describe, expect, it } from "vitest";
import { decided, photo } from "../lib/fixtures";
import { groupScenes } from "../lib/grouping";
import { locate, moveFocus, moveScene, moveStack, neighbourStack, nextUndecided } from "./cursor";

const minute = 60_000;
const scenes = groupScenes([
  photo("a1", 0),
  photo("a2", 1_000),
  photo("b", 10_000),
  photo("c1", 20 * minute),
  photo("c2", 20 * minute + 1_000),
  photo("c3", 20 * minute + 2_000),
  photo("d", 20 * minute + 10_000),
]);

describe("cursor", () => {
  it("moves focus inside a burst", () => {
    expect(moveFocus(scenes, { scene: 0, stack: 0, frame: 0 }, 1)).toEqual({ scene: 0, stack: 0, frame: 1 });
  });

  it("moves to the next stack past the last frame", () => {
    expect(moveFocus(scenes, { scene: 0, stack: 0, frame: 1 }, 1)).toEqual({ scene: 0, stack: 1, frame: 0 });
    expect(moveFocus(scenes, { scene: 0, stack: 1, frame: 0 }, 1)).toEqual({ scene: 1, stack: 0, frame: 0 });
  });

  it("lands on the last frame of the previous stack when moving back past the first frame", () => {
    expect(moveFocus(scenes, { scene: 1, stack: 0, frame: 0 }, -1)).toEqual({ scene: 0, stack: 1, frame: 0 });
    expect(moveFocus(scenes, { scene: 1, stack: 1, frame: 0 }, -1)).toEqual({ scene: 1, stack: 0, frame: 2 });
  });

  it("stays on the last stack and on the first stack", () => {
    expect(moveStack(scenes, { scene: 1, stack: 1, frame: 0 }, 1)).toBeNull();
    expect(moveFocus(scenes, { scene: 1, stack: 1, frame: 0 }, 1)).toBeNull();
    expect(moveStack(scenes, { scene: 0, stack: 0, frame: 1 }, -1)).toBeNull();
  });

  it("moves stacks across scenes and lands on the first frame", () => {
    expect(moveStack(scenes, { scene: 0, stack: 1, frame: 0 }, 1)).toEqual({ scene: 1, stack: 0, frame: 0 });
    expect(moveStack(scenes, { scene: 1, stack: 0, frame: 2 }, -1)).toEqual({ scene: 0, stack: 1, frame: 0 });
  });

  it("moves between scenes and stops at either end", () => {
    expect(moveScene(scenes, { scene: 0, stack: 1, frame: 0 }, 1)).toEqual({ scene: 1, stack: 0, frame: 0 });
    expect(moveScene(scenes, { scene: 1, stack: 0, frame: 0 }, 1)).toBeNull();
    expect(moveScene(scenes, { scene: 0, stack: 0, frame: 0 }, -1)).toBeNull();
  });

  it("finds the next undecided stack and wraps around", () => {
    const marked = groupScenes([
      photo("a", 0),
      photo("b", 10_000, { decision: decided({ mark: "approved" }) }),
      photo("c", 20_000, { decision: decided({ mark: "delete" }) }),
    ]);
    expect(nextUndecided(marked, { scene: 0, stack: 1, frame: 0 })).toEqual({ scene: 0, stack: 0, frame: 0 });
    expect(nextUndecided(marked, { scene: 0, stack: 0, frame: 0 })).toEqual({ scene: 0, stack: 0, frame: 0 });
  });

  it("reports when everything is decided", () => {
    const done = groupScenes([photo("a", 0, { decision: decided({ mark: "unsure" }) })]);
    expect(nextUndecided(done, { scene: 0, stack: 0, frame: 0 })).toBeNull();
  });

  it("locates a photo and finds neighbouring stacks", () => {
    expect(locate(scenes, "c3")).toEqual({ scene: 1, stack: 0, frame: 2 });
    expect(locate(scenes, "zz")).toBeNull();
    expect(neighbourStack(scenes, { scene: 0, stack: 1, frame: 0 }, 1)?.map((p) => p.id)).toEqual([
      "c1",
      "c2",
      "c3",
    ]);
    expect(neighbourStack(scenes, { scene: 0, stack: 0, frame: 0 }, -1)).toBeNull();
  });
});
