import { describe, expect, it } from "vitest";
import { decided } from "../lib/fixtures";
import { swipeDecision } from "./swipe";

describe("swipeDecision", () => {
  it("maps each direction to its mark", () => {
    expect(swipeDecision(decided({}), "left").mark).toBe("delete");
    expect(swipeDecision(decided({}), "right").mark).toBe("approved");
    expect(swipeDecision(decided({}), "down").mark).toBe("unsure");
  });

  it("up approves and favorites without toggling an existing favorite off", () => {
    expect(swipeDecision(decided({}), "up")).toEqual(decided({ mark: "approved", favorite: true }));
    expect(swipeDecision(decided({ favorite: true, rating: 3 }), "up")).toEqual(
      decided({ mark: "approved", favorite: true, rating: 3 }),
    );
  });

  it("keeps the rating", () => {
    expect(swipeDecision(decided({ rating: 4 }), "left").rating).toBe(4);
  });
});
