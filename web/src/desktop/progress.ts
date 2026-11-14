import type { Photo } from "../lib/types";

export interface Tally {
  approved: number;
  unsure: number;
  delete: number;
  undecided: number;
  total: number;
}

export function tally(photos: Photo[]): Tally {
  const counts: Tally = { approved: 0, unsure: 0, delete: 0, undecided: 0, total: photos.length };
  for (const photo of photos) {
    const mark = photo.decision.mark;
    if (mark) {
      counts[mark] += 1;
    } else {
      counts.undecided += 1;
    }
  }
  return counts;
}

export function percent(part: number, total: number): string {
  return `${total ? (part / total) * 100 : 0}%`;
}

export function clock(ms: number, withSeconds = true): string {
  return new Date(ms).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    ...(withSeconds ? { second: "2-digit" } : {}),
    timeZone: "UTC",
  });
}

export function day(ms: number): string {
  return new Date(ms).toLocaleDateString([], {
    weekday: "short",
    day: "numeric",
    month: "short",
    timeZone: "UTC",
  });
}
