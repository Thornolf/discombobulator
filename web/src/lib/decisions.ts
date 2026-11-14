import type { Decision, Mark } from "./types";

export type MarkAction = Mark | "clear";

export function withMark(decision: Decision, action: MarkAction): Decision {
  if (action === "clear") {
    return { ...decision, mark: null };
  }
  return { ...decision, mark: decision.mark === action ? null : action };
}

export function withFavorite(decision: Decision): Decision {
  return { ...decision, favorite: !decision.favorite };
}

export function withRating(decision: Decision, rating: number): Decision {
  if (rating === 0) {
    return { ...decision, rating: 0 };
  }
  return { ...decision, rating: decision.rating === rating ? 0 : rating };
}

export function sameDecision(a: Decision, b: Decision): boolean {
  return a.mark === b.mark && a.favorite === b.favorite && a.rating === b.rating;
}

export interface Framed {
  id: string;
  decision: Decision;
}

export function keepPlan(
  stack: Framed[],
  selected: ReadonlySet<string>,
  focusedId: string | null,
): { approve: string[]; remove: string[] } {
  const keep = selected.size
    ? stack.filter((frame) => selected.has(frame.id))
    : stack.filter((frame) => frame.id === focusedId);
  const keepIds = new Set(keep.map((frame) => frame.id));
  return {
    approve: keep.filter((frame) => frame.decision.mark !== "approved").map((frame) => frame.id),
    remove: stack
      .filter((frame) => !keepIds.has(frame.id) && frame.decision.mark === null)
      .map((frame) => frame.id),
  };
}
