import { api, ApiError } from "./api";
import { rebase, sameDecision } from "./decisions";
import type { Decision, DecisionChange, Photo, ServerState } from "./types";

const pendingKey = "discombobulator.pending";
const rootKey = "discombobulator.root";
const undoLimit = 5_000;

interface UndoEntry {
  before: DecisionChange[];
}

interface PendingEntry {
  id: string;
  root: string | null;
  decision: Decision;
  base?: Decision;
}

type Pending = Map<string, PendingEntry>;

function pendingId(root: string | null, id: string): string {
  return JSON.stringify([root, id]);
}

function isDecision(value: unknown): value is Decision {
  return typeof value === "object" && value !== null && "mark" in value;
}

function readPending(raw: unknown, fallbackRoot: string | null): PendingEntry[] {
  if (Array.isArray(raw)) {
    return raw
      .filter(
        (entry): entry is PendingEntry =>
          typeof entry === "object" &&
          entry !== null &&
          typeof (entry as PendingEntry).id === "string" &&
          isDecision((entry as PendingEntry).decision),
      )
      .map((entry) =>
        entry.base === undefined || isDecision(entry.base) ? entry : { ...entry, base: undefined },
      );
  }
  if (typeof raw === "object" && raw !== null) {
    return Object.entries(raw).flatMap(([id, decision]) =>
      isDecision(decision) ? [{ id, root: fallbackRoot, decision }] : [],
    );
  }
  return [];
}

function loadRoot(): string | null {
  try {
    return localStorage.getItem(rootKey);
  } catch {
    return null;
  }
}

function saveRoot(root: string | null): void {
  try {
    if (root) localStorage.setItem(rootKey, root);
  } catch {
    return;
  }
}

function loadPending(fallbackRoot: string | null): Pending {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(pendingKey) ?? "[]");
    return new Map(readPending(raw, fallbackRoot).map((entry) => [pendingId(entry.root, entry.id), entry]));
  } catch {
    return new Map();
  }
}

function savePending(pending: Pending): void {
  try {
    localStorage.setItem(pendingKey, JSON.stringify([...pending.values()]));
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
  #lastRoot = loadRoot();
  #photosRoot: string | null = null;
  #pending = loadPending(this.#lastRoot);
  #version = -1;
  #seq = -1;
  #flushing: Promise<void> | null = null;
  #edits = 0;
  #editedAt = new Map<string, number>();
  #timer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    this.pendingCount = this.#pending.size;
  }

  get #root(): string | null {
    return this.server?.root ?? this.#lastRoot;
  }

  #adopt(root: string | null): void {
    if (!root) {
      return;
    }
    this.#lastRoot = root;
    saveRoot(root);
    let changed = false;
    for (const [key, entry] of this.#pending) {
      if (entry.root === null) {
        this.#pending.delete(key);
        this.#pending.set(pendingId(root, entry.id), { ...entry, root });
        changed = true;
      }
    }
    if (changed) {
      savePending(this.#pending);
    }
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
      this.#adopt(server.root);
      this.online = true;
      if (server.version !== this.#version) {
        this.#version = server.version;
        this.#seq = server.decisions_seq;
        await this.refresh();
      } else if (server.decisions_seq < this.#seq) {
        this.#seq = server.decisions_seq;
        await this.refresh();
      } else if (server.decisions_seq !== this.#seq) {
        await this.pullDecisions();
      }
      await this.flush();
    } catch (error) {
      this.online = false;
      this.error = error instanceof Error ? error.message : String(error);
      if (!this.photos.length) {
        await this.refresh(true).catch(() => undefined);
      }
    }
    const delay = this.server?.processing ? 1_000 : 4_000;
    this.#timer = setTimeout(() => void this.poll(), delay);
  }

  async refresh(offline = false): Promise<void> {
    if (!offline && !this.server?.root) {
      this.setPhotos([]);
      return;
    }
    this.loading = true;
    const startedAt = this.#edits;
    const unsynced = new Set([...this.#pending.values()].map((entry) => entry.id));
    const root = this.#root;
    try {
      const photos = await api.photos();
      for (const photo of photos) {
        const local = this.#byId.get(photo.id);
        const pending = this.#merge(root, photo.id, photo.decision);
        if (pending) {
          photo.decision = pending;
        } else if (local && (unsynced.has(photo.id) || (this.#editedAt.get(photo.id) ?? -1) >= startedAt)) {
          photo.decision = { ...local.decision };
        }
      }
      savePending(this.#pending);
      this.setPhotos(photos, root);
      this.error = null;
    } finally {
      this.loading = false;
    }
  }

  async pullDecisions(): Promise<void> {
    const since = Math.max(0, this.#seq);
    const root = this.#photosRoot ?? this.#root;
    const result = await api.decisionsSince(since);
    for (const change of result.changes) {
      const photo = this.#byId.get(change.id);
      if (!photo) {
        continue;
      }
      const next = this.#merge(root, change.id, change.decision) ?? change.decision;
      if (!sameDecision(photo.decision, next)) {
        photo.decision = next;
      }
    }
    this.#seq = result.seq;
    savePending(this.#pending);
    this.pendingCount = this.#pending.size;
  }

  #merge(root: string | null, id: string, remote: Decision): Decision | null {
    const key = pendingId(root, id);
    const entry = this.#pending.get(key);
    if (!entry) {
      return null;
    }
    const merged = rebase(entry.base, entry.decision, remote);
    this.#pending.set(key, { ...entry, decision: merged, base: { ...remote } });
    return merged;
  }

  setPhotos(photos: Photo[], root: string | null = this.#root): void {
    this.#photosRoot = root;
    this.photos = photos;
    this.#byId = new Map(this.photos.map((photo) => [photo.id, photo]));
  }

  async open(path: string): Promise<void> {
    await this.#flushing;
    await this.flush();
    this.server = await api.open(path);
    this.#adopt(this.server.root);
    this.#version = this.server.version;
    this.#seq = this.server.decisions_seq;
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
    const root = this.#photosRoot ?? this.#root;
    for (const change of effective) {
      const photo = this.#byId.get(change.id);
      const key = pendingId(root, change.id);
      const base = this.#pending.has(key) ? this.#pending.get(key)?.base : photo && { ...photo.decision };
      if (photo) {
        photo.decision = change.decision;
      }
      this.#pending.set(key, { id: change.id, root, decision: change.decision, base });
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
      return this.#flushing;
    }
    const root = this.server?.root;
    if (!root) {
      return;
    }
    const snapshot: Pending = new Map([...this.#pending].filter(([, entry]) => entry.root === root));
    const changes = [...snapshot.values()].map(({ id, decision }) => ({ id, decision }));
    if (!changes.length) {
      return;
    }
    let again = false;
    const run = (async () => {
      try {
        const result = await api.decide(changes);
        for (const id of result.missing) {
          this.#pending.delete(pendingId(root, id));
        }
        this.#settle(snapshot);
        this.online = true;
        again = [...this.#pending.values()].some((entry) => entry.root === root);
      } catch (error) {
        if (error instanceof ApiError && (error.status === 400 || error.status === 422)) {
          this.#settle(snapshot);
          this.error = `Some marks were rejected: ${error.message}`;
        } else {
          this.online = false;
        }
      }
    })();
    this.#flushing = run;
    try {
      await run;
    } finally {
      this.#flushing = null;
    }
    if (again) {
      await this.flush();
    }
  }

  #settle(snapshot: Pending): void {
    for (const [key, entry] of snapshot) {
      const current = this.#pending.get(key);
      if (current && sameDecision(current.decision, entry.decision)) {
        this.#pending.delete(key);
      }
    }
    savePending(this.#pending);
    this.pendingCount = this.#pending.size;
  }
}
