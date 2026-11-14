import { emptyDecision, type Decision, type Photo } from "./types";

export function photo(id: string, time_ms: number, overrides: Partial<Photo> = {}): Photo {
  return {
    id,
    name: `DSCF${id}.JPG`,
    folder: "",
    time_ms,
    captured: true,
    dhash: null,
    width: 100,
    height: 100,
    raf: false,
    jpg: true,
    size: 1,
    error: null,
    version: "v1",
    camera_rating: null,
    decision: { ...emptyDecision },
    ...overrides,
  };
}

export function decided(decision: Partial<Decision>): Decision {
  return { ...emptyDecision, ...decision };
}
