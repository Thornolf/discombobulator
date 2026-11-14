import { describe, expect, it } from "vitest";
import { decided, photo } from "../lib/fixtures";
import {
  belongsInDeck,
  extendDeck,
  isDoubleTap,
  jumpToScene,
  swipeDecision,
  waitingInScene,
  zoomOrigin,
} from "./swipe";

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

  it("brings back a card that returned to the inbox after it was swiped", () => {
    const photos = [photo("a", 0), photo("c", 1, { decision: decided({ mark: "approved" }) })];
    expect(extendDeck(["a", "c"], photos, "inbox", 2, new Set(["a"]))).toEqual(["a", "c", "a"]);
    expect(extendDeck(["a", "c", "a"], photos, "inbox", 2, new Set(["a"]))).toEqual(["a", "c", "a"]);
  });

  it("does not bring back a card that never left the deck's pile", () => {
    const photos = [
      photo("u1", 0, { decision: decided({ mark: "unsure" }) }),
      photo("u2", 1, { decision: decided({ mark: "approved" }) }),
      photo("u3", 2, { decision: decided({ mark: "unsure" }) }),
    ];
    expect(extendDeck(["u1", "u2", "u3"], photos, "unsure", 2, new Set(["u2"]))).toEqual(["u1", "u2", "u3"]);
  });

  it("knows which cards still belong to the deck", () => {
    expect(belongsInDeck("inbox", decided({}))).toBe(true);
    expect(belongsInDeck("inbox", decided({ mark: "delete" }))).toBe(false);
    expect(belongsInDeck("unsure", decided({ mark: "unsure" }))).toBe(true);
    expect(belongsInDeck("unsure", decided({}))).toBe(false);
  });
});

describe("jumpToScene", () => {
  it("brings the scene's first remaining card forward and keeps every other card", () => {
    expect(jumpToScene(["a", "b", "c", "d", "e"], 1, new Set(["d", "e"]))).toEqual(["a", "d", "e", "b", "c"]);
  });

  it("only counts the scene's cards that still belong to the deck", () => {
    const scene = [photo("a", 0, { decision: decided({ mark: "approved" }) }), photo("b", 1)];
    expect([...waitingInScene(scene, "inbox")]).toEqual(["b"]);
    expect(jumpToScene(["x", "a"], 0, waitingInScene(scene.slice(0, 1), "inbox"))).toBeNull();
  });

  it("leaves the deck alone when nothing of the scene is left", () => {
    expect(jumpToScene(["a", "b"], 1, new Set(["a"]))).toBeNull();
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
