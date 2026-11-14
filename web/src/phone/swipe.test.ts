import { describe, expect, it } from "vitest";
import { decided, photo } from "../lib/fixtures";
import { belongsInDeck, extendDeck, isDoubleTap, swipeDecision, zoomOrigin } from "./swipe";

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

describe("deck", () => {
  it("appends newly unmarked photos without repeating cards", () => {
    const photos = [
      photo("a", 0),
      photo("b", 1, { decision: decided({ mark: "approved" }) }),
      photo("c", 2),
      photo("d", 3, { decision: decided({ mark: "unsure" }) }),
    ];
    expect(extendDeck(["a"], photos, "inbox")).toEqual(["a", "c"]);
    expect(extendDeck(["a", "c"], photos, "inbox")).toEqual(["a", "c"]);
    expect(extendDeck([], photos, "unsure")).toEqual(["d"]);
  });

  it("knows which cards still belong to the deck", () => {
    expect(belongsInDeck("inbox", decided({}))).toBe(true);
    expect(belongsInDeck("inbox", decided({ mark: "delete" }))).toBe(false);
    expect(belongsInDeck("unsure", decided({ mark: "unsure" }))).toBe(true);
    expect(belongsInDeck("unsure", decided({}))).toBe(false);
  });
});

describe("double tap", () => {
  it("needs two taps close in time and space", () => {
    const first = { at: 1_000, x: 100, y: 100 };
    expect(isDoubleTap(null, first)).toBe(false);
    expect(isDoubleTap(first, { at: 1_250, x: 110, y: 105 })).toBe(true);
    expect(isDoubleTap(first, { at: 1_300, x: 100, y: 100 })).toBe(false);
    expect(isDoubleTap(first, { at: 1_100, x: 130, y: 100 })).toBe(false);
  });

  it("zooms around the tapped point", () => {
    const rect = { left: 10, top: 20, width: 200, height: 100 };
    expect(zoomOrigin(rect, 60, 95)).toBe("25% 75%");
    expect(zoomOrigin(rect, 0, 500)).toBe("0% 100%");
    expect(zoomOrigin({ left: 0, top: 0, width: 0, height: 0 }, 5, 5)).toBe("50% 50%");
  });
});
