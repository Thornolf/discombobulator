import type { Scene, Stack } from "../lib/grouping";

export interface Cursor {
  scene: number;
  stack: number;
  frame: number;
}

export type Direction = 1 | -1;

export function stackAt(scenes: Scene[], cursor: Cursor): Stack {
  return scenes[cursor.scene]?.stacks[cursor.stack] ?? [];
}

export function sameStack(a: Cursor, b: Cursor): boolean {
  return a.scene === b.scene && a.stack === b.stack;
}

export function neighbourStack(scenes: Scene[], cursor: Cursor, direction: Direction): Stack | null {
  let scene = cursor.scene;
  let stack = cursor.stack + direction;
  while (scenes[scene] && (stack < 0 || stack >= (scenes[scene]?.stacks.length ?? 0))) {
    scene += direction;
    const next = scenes[scene];
    if (!next) return null;
    stack = direction > 0 ? 0 : next.stacks.length - 1;
  }
  return scenes[scene]?.stacks[stack] ?? null;
}

export function moveStack(
  scenes: Scene[],
  cursor: Cursor,
  direction: Direction,
  landOnLast = false,
): Cursor | null {
  const current = scenes[cursor.scene];
  if (!current) return null;
  let scene = cursor.scene;
  let stack = cursor.stack + direction;
  if (stack >= current.stacks.length) {
    if (scene >= scenes.length - 1) return null;
    scene += 1;
    stack = 0;
  }
  if (stack < 0) {
    if (scene === 0) return null;
    scene -= 1;
    stack = (scenes[scene]?.stacks.length ?? 1) - 1;
  }
  const target = scenes[scene]?.stacks[stack] ?? [];
  return { scene, stack, frame: landOnLast ? Math.max(0, target.length - 1) : 0 };
}

export function moveFocus(scenes: Scene[], cursor: Cursor, direction: Direction): Cursor | null {
  const frames = stackAt(scenes, cursor);
  const frame = cursor.frame + direction;
  if (frames.length < 2 || frame < 0 || frame >= frames.length) {
    return moveStack(scenes, cursor, direction, direction < 0);
  }
  return { ...cursor, frame };
}

export function moveScene(scenes: Scene[], cursor: Cursor, direction: Direction): Cursor | null {
  const scene = cursor.scene + direction;
  return scenes[scene] ? { scene, stack: 0, frame: 0 } : null;
}

export function nextUndecided(scenes: Scene[], cursor: Cursor): Cursor | null {
  const flat = scenes.flatMap((item, scene) =>
    item.stacks.map((frames, stack) => ({ scene, stack, frames })),
  );
  const start = flat.findIndex((entry) => entry.scene === cursor.scene && entry.stack === cursor.stack);
  for (let offset = 1; offset <= flat.length; offset += 1) {
    const entry = flat[(start + offset) % flat.length];
    if (entry?.frames.some((photo) => !photo.decision.mark)) {
      return { scene: entry.scene, stack: entry.stack, frame: 0 };
    }
  }
  return null;
}

export function locate(scenes: Scene[], id: string): Cursor | null {
  for (const [scene, item] of scenes.entries()) {
    for (const [stack, frames] of item.stacks.entries()) {
      const frame = frames.findIndex((photo) => photo.id === id);
      if (frame >= 0) return { scene, stack, frame };
    }
  }
  return null;
}
