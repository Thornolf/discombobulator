export type Mark = "approved" | "delete" | "unsure";

export interface Decision {
  mark: Mark | null;
  favorite: boolean;
  rating: number;
}

export interface Photo {
  id: string;
  name: string;
  folder: string;
  time_ms: number;
  captured: boolean;
  dhash: string | null;
  width: number | null;
  height: number | null;
  raf: boolean;
  jpg: boolean;
  size: number;
  error: string | null;
  version: string;
  camera_rating: number | null;
  decision: Decision;
}

export interface Progress {
  total: number;
  done: number;
  failed: number;
  running: boolean;
}

export interface ServerState {
  root: string | null;
  version: number;
  processing: boolean;
  progress: Progress | null;
}

export interface FolderEntry {
  name: string;
  path: string;
}

export interface FolderListing {
  path: string | null;
  parent: string | null;
  entries: FolderEntry[];
}

export interface DecisionChange {
  id: string;
  decision: Decision;
}

export const emptyDecision: Decision = { mark: null, favorite: false, rating: 0 };

export interface ApplyPlan {
  sidecars: number;
  deletes: number;
  files_to_move: number;
}

export interface ApplyReport {
  sidecars_written: number;
  photos_moved: number;
  files_moved: number;
  failures: { name: string; error: string }[];
}

export interface DuplicateGroup {
  kind: "exact" | "similar";
  ids: string[];
}

export interface PhoneStatus {
  tailscale: boolean;
  url: string | null;
  qr_svg: string | null;
  error: string | null;
  action_url: string | null;
}
