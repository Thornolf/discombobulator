import { hamming } from "./hash";
import type { Photo } from "./types";

export interface GroupingSettings {
  sceneGapMs: number;
  burstGapMs: number;
  burstDistance: number;
}

export const defaultGrouping: GroupingSettings = {
  sceneGapMs: 5 * 60_000,
  burstGapMs: 2_500,
  burstDistance: 12,
};

export type Stack = Photo[];

export interface Scene {
  photos: Photo[];
  stacks: Stack[];
}

export function sortPhotos(photos: Photo[]): Photo[] {
  return [...photos].sort(
    (a, b) => a.time_ms - b.time_ms || a.name.localeCompare(b.name, undefined, { numeric: true }),
  );
}

export function groupScenes(photos: Photo[], settings: GroupingSettings = defaultGrouping): Scene[] {
  const scenes: Scene[] = [];
  let scene: Scene | null = null;
  let previous: Photo | null = null;
  for (const photo of sortPhotos(photos)) {
    if (!scene || !previous || photo.time_ms - previous.time_ms > settings.sceneGapMs) {
      scene = { photos: [], stacks: [] };
      scenes.push(scene);
      previous = null;
    }
    const stack = scene.stacks.at(-1);
    const distance = previous ? hamming(photo.dhash, previous.dhash) : null;
    const sameBurst =
      stack !== undefined &&
      previous !== null &&
      (photo.time_ms - previous.time_ms <= settings.burstGapMs ||
        (distance !== null && distance <= settings.burstDistance));
    if (sameBurst && stack) {
      stack.push(photo);
    } else {
      scene.stacks.push([photo]);
    }
    scene.photos.push(photo);
    previous = photo;
  }
  return scenes;
}
