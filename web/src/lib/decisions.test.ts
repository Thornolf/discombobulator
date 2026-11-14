import { describe, expect, it } from "vitest";
import { keepPlan, withFavorite, withMark, withRating } from "./decisions";
import { decided } from "./fixtures";

describe("marks", () => {
  it("toggles the same mark off and replaces another", () => {
    expect(withMark(decided({ mark: "approved" }), "approved").mark).toBeNull();
    expect(withMark(decided({ mark: "unsure" }), "delete").mark).toBe("delete");
    expect(withMark(decided({ mark: "delete", favorite: true }), "clear")).toEqual(
      decided({ favorite: true }),
    );
  });

  it("favorite and rating are independent of the mark", () => {
    expect(withFavorite(decided({ mark: "delete" }))).toEqual(decided({ mark: "delete", favorite: true }));
    expect(withRating(decided({ mark: "unsure" }), 4)).toEqual(decided({ mark: "unsure", rating: 4 }));
  });

  it("same rating again clears it and zero always clears", () => {
    expect(withRating(decided({ rating: 3 }), 3).rating).toBe(0);
    expect(withRating(decided({ rating: 3 }), 0).rating).toBe(0);
    expect(withRating(decided({ rating: 3 }), 5).rating).toBe(5);
  });
});

describe("keepPlan", () => {
  const stack = [
    { id: "a", decision: decided({}) },
    { id: "b", decision: decided({ mark: "unsure" }) },
    { id: "c", decision: decided({}) },
    { id: "d", decision: decided({ mark: "approved" }) },
    { id: "e", decision: decided({ mark: "delete" }) },
  ];

  it("keeps the focused frame and deletes only undecided others", () => {
    expect(keepPlan(stack, new Set(), "a")).toEqual({ approve: ["a"], remove: ["c"] });
  });

  it("keeps every selected frame", () => {
    expect(keepPlan(stack, new Set(["a", "c"]), "b")).toEqual({ approve: ["a", "c"], remove: [] });
  });

  it("does not re-approve a frame that is already approved", () => {
    expect(keepPlan(stack, new Set(), "d")).toEqual({ approve: [], remove: ["a", "c"] });
  });
});
