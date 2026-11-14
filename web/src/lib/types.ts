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

export interface Point {
  x: number;
  y: number;
}

export interface Area {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface ShotInfo {
  camera: string | null;
  lens: string | null;
  focal_length: number | null;
  focal_length_35mm: number | null;
  exposure_time: string | null;
  f_number: number | null;
  iso: number | null;
  exposure_bias: number | null;
  program: string | null;
  film_simulation: string | null;
  dynamic_range: string | null;
  grain: string | null;
  focus_mode: string | null;
  af_area: string | null;
  focus_point: Point | null;
  faces: Area[];
  focus_warning: boolean | null;
  blur_warning: boolean | null;
  exposure_warning: boolean | null;
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
  can_open_folders: boolean;
  decisions_seq: number;
  app_version: string;
  can_check_updates: boolean;
}

export interface DecisionChanges {
  seq: number;
  changes: DecisionChange[];
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

export interface RecyclePlan {
  files: number;
  bytes: number;
}

export interface RecycleReport {
  recycled: number;
  bytes: number;
  failures: { name: string; error: string }[];
}
