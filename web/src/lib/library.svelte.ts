import { api, ApiError } from "./api";
import { sameDecision } from "./decisions";
import type { Decision, DecisionChange, Photo, ServerState } from "./types";

const pendingKey = "discombobulator.pending";
const undoLimit = 5_000;

interface UndoEntry {
  before: DecisionChange[];
}

function loadPending(): Map<string, Decision> {
  try {
    const stored = JSON.parse(localStorage.getItem(pendingKey) ?? "{}") as Record<string, Decision> | null;
    return new Map(Object.entries(stored ?? {}));
  } catch {
    return new Map();
  }
}

function savePending(pending: Map<string, Decision>): void {
  try {
    localStorage.setItem(pendingKey, JSON.stringify(Object.fromEntries(pending)));
  } catch {
    return;
  }
}

export class LibraryStore {
  photos = $state<Photo[]>([]);
  server = $state<ServerState | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);
  pendingCount = $state(0);
  online = $state(true);
  #byId = new Map<string, Photo>();
  #undo: UndoEntry[] = [];
  #pending = loadPending();
  #version = -1;
  #flushing = false;
  #edits = 0;
  #editedAt = new Map<string, number>();
  #timer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    this.pendingCount = this.#pending.size;
  }

  photo(id: string): Photo | undefined {
    return this.#byId.get(id);
  }

  start(): void {
    void this.poll();
  }

  stop(): void {
    if (this.#timer) {
      clearTimeout(this.#timer);
    }
  }

  async poll(): Promise<void> {
    try {
      const server = await api.state();
      this.server = server;
      this.online = true;
      if (server.version !== this.#version) {
        this.#version = server.version;
        await this.refresh();
      }
      await this.flush();
    } catch (error) {
      this.online = false;
      this.error = error instanceof Error ? error.message : String(error);
    }
    const delay = this.server?.processing ? 1_000 : 4_000;
    this.#timer = setTimeout(() => void this.poll(), delay);
  }

  async refresh(): Promise<void> {
    if (!this.server?.root) {
      this.setPhotos([]);
      return;
    }
    this.loading = true;
    const startedAt = this.#edits;
    try {
      const photos = await api.photos();
      for (const photo of photos) {
        const pending = this.#pending.get(photo.id);
        const local = this.#byId.get(photo.id);
        if (pending) {
          photo.decision = pending;
        } else if (local && (this.#editedAt.get(photo.id) ?? -1) >= startedAt) {
          photo.decision = { ...local.decision };
        }
      }
      this.setPhotos(photos);
      this.error = null;
    } finally {
      this.loading = false;
    }
  }

  setPhotos(photos: Photo[]): void {
    this.photos = photos;
    this.#byId = new Map(this.photos.map((photo) => [photo.id, photo]));
  }

  async open(path: string): Promise<void> {
    await this.flush();
    this.server = await api.open(path);
    this.#version = this.server.version;
    this.#undo = [];
    await this.refresh();
  }

  decide(changes: DecisionChange[], recordUndo = true): void {
    const effective = changes.filter((change) => {
      const photo = this.#byId.get(change.id);
      return photo !== undefined && !sameDecision(photo.decision, change.decision);
    });
    if (!effective.length) {
      return;
    }
    if (recordUndo) {
      this.#undo.push({
        before: effective.flatMap((change) => {
          const photo = this.#byId.get(change.id);
          return photo ? [{ id: change.id, decision: { ...photo.decision } }] : [];
        }),
      });
      if (this.#undo.length > undoLimit) {
        this.#undo.shift();
      }
    }
    for (const change of effective) {
      const photo = this.#byId.get(change.id);
      if (photo) {
        photo.decision = change.decision;
      }
      this.#pending.set(change.id, change.decision);
      this.#editedAt.set(change.id, this.#edits);
    }
    this.#edits += 1;
    savePending(this.#pending);
    this.pendingCount = this.#pending.size;
    void this.flush();
  }

  undo(): DecisionChange[] | null {
    const entry = this.#undo.pop();
    if (!entry) {
      return null;
    }
    this.decide(entry.before, false);
    return entry.before;
  }

  async flush(): Promise<void> {
    if (this.#flushing) {
      return;
    }
    const snapshot = new Map(this.#pending);
    const changes = [...snapshot].map(([id, decision]) => ({ id, decision }));
    if (!changes.length) {
      return;
    }
    this.#flushing = true;
    let again = false;
    try {
      const result = await api.decide(changes);
      for (const id of result.missing) {
        this.#pending.delete(id);
      }
      this.#settle(snapshot);
      this.online = true;
      again = this.#pending.size > 0;
    } catch (error) {
      if (error instanceof ApiError && error.status >= 400 && error.status < 500 && error.status !== 409) {
        this.#settle(snapshot);
        this.error = `Some marks were rejected: ${error.message}`;
      } else {
        this.online = false;
      }
    } finally {
      this.#flushing = false;
    }
    if (again) {
      await this.flush();
    }
  }

  #settle(snapshot: Map<string, Decision>): void {
    for (const [id, decision] of snapshot) {
      const current = this.#pending.get(id);
      if (current && sameDecision(current, decision)) {
        this.#pending.delete(id);
      }
    }
    savePending(this.#pending);
    this.pendingCount = this.#pending.size;
  }
}
