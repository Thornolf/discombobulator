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

export type DeckMode = "inbox" | "unsure";

export function belongsInDeck(mode: DeckMode, decision: Decision): boolean {
  return mode === "inbox" ? decision.mark === null : decision.mark === "unsure";
}

export function extendDeck(
  deck: readonly string[],
  photos: readonly { id: string; decision: Decision }[],
  mode: DeckMode,
  position = 0,
  departed: ReadonlySet<string> = new Set(),
): string[] {
  const known = new Set(deck.filter((id, index) => index >= position || !departed.has(id)));
  const fresh = photos
    .filter((photo) => !known.has(photo.id) && belongsInDeck(mode, photo.decision))
    .map((photo) => photo.id);
  return fresh.length ? [...deck, ...fresh] : [...deck];
}

export function jumpToScene(
  deck: readonly string[],
  position: number,
  scene: ReadonlySet<string>,
): string[] | null {
  const rest = deck.slice(position);
  const first = rest.findIndex((id) => scene.has(id));
  if (first < 0) return null;
  return [...deck.slice(0, position), ...rest.slice(first), ...rest.slice(0, first)];
}

export interface Tap {
  at: number;
  x: number;
  y: number;
}

export const doubleTapMs = 300;
export const doubleTapDistance = 20;

export function isDoubleTap(previous: Tap | null, tap: Tap): boolean {
  return (
    previous !== null &&
    tap.at - previous.at < doubleTapMs &&
    Math.hypot(tap.x - previous.x, tap.y - previous.y) < doubleTapDistance
  );
}

export function zoomOrigin(
  rect: { left: number; top: number; width: number; height: number },
  x: number,
  y: number,
): string {
  const share = (offset: number, size: number) =>
    size > 0 ? Math.min(100, Math.max(0, (offset / size) * 100)) : 50;
  return `${share(x - rect.left, rect.width)}% ${share(y - rect.top, rect.height)}%`;
}
