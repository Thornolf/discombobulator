import type {
  ApplyPlan,
  ApplyReport,
  DecisionChange,
  DecisionChanges,
  DuplicateGroup,
  FolderListing,
  PhoneStatus,
  RecyclePlan,
  RecycleReport,
  Photo,
  ServerState,
} from "./types";

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      "content-type": "application/json",
      "x-discombobulator": "1",
      ...init.headers,
    },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as { error?: string } | null;
    throw new ApiError(response.status, body?.error ?? response.statusText);
  }
  return (await response.json()) as T;
}

export const api = {
  state: () => request<ServerState>("/api/state"),
  folders: (path?: string) =>
    request<FolderListing>(`/api/folders${path ? `?path=${encodeURIComponent(path)}` : ""}`),
  open: (path: string) =>
    request<ServerState>("/api/library", { method: "POST", body: JSON.stringify({ path }) }),
  photos: () => request<Photo[]>("/api/photos"),
  duplicates: (tolerance: number) => request<DuplicateGroup[]>(`/api/duplicates?tolerance=${tolerance}`),
  phone: () => request<PhoneStatus>("/api/phone"),
  enablePhone: () => request<PhoneStatus>("/api/phone", { method: "POST" }),
  recyclePlan: () => request<RecyclePlan>("/api/recycle"),
  recycle: (confirmed: RecyclePlan) =>
    request<RecycleReport>("/api/recycle", { method: "POST", body: JSON.stringify(confirmed) }),
  applyPlan: () => request<ApplyPlan>("/api/apply"),
  apply: () => request<ApplyReport>("/api/apply", { method: "POST" }),
  decisionsSince: (since: number) => request<DecisionChanges>(`/api/decisions?since=${since}`),
  decide: (changes: DecisionChange[]) =>
    request<{ applied: number; missing: string[] }>("/api/decisions", {
      method: "POST",
      body: JSON.stringify({ changes }),
    }),
};

export function thumbUrl(photo: Photo): string {
  return `/api/photos/${photo.id}/thumb?v=${photo.version}`;
}

export function previewUrl(photo: Photo): string {
  return `/api/photos/${photo.id}/preview?v=${photo.version}`;
}
