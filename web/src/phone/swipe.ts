import type { Decision } from "../lib/types";

export type Direction = "left" | "right" | "up" | "down";

export function swipeDecision(decision: Decision, direction: Direction): Decision {
  switch (direction) {
    case "left":
      return { ...decision, mark: "delete" };
    case "right":
      return { ...decision, mark: "approved" };
    case "up":
      return { ...decision, mark: "approved", favorite: true };
    case "down":
      return { ...decision, mark: "unsure" };
  }
}
