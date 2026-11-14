import { beforeEach, describe, expect, it, vi } from "vitest";
import { photo } from "./fixtures";
import type { Decision, DecisionChange, Photo } from "./types";

const server = {
  photos: [] as Photo[],
  decided: [] as DecisionChange[][],
  log: [] as DecisionChange[],
  photosGate: null as Promise<void> | null,
  pullGate: null as Promise<void> | null,
  unreachable: false,
};

function record(id: string, decision: Decision) {
  const target = server.photos.find((item) => item.id === id);
  if (target) target.decision = decision;
  server.log.push({ id, decision });
}

vi.mock("./api", () => ({
  ApiError: class extends Error {
    constructor(
      readonly status: number,
      message: string,
    ) {
      super(message);
    }
  },
  api: {
    state: async () => ({
      root: "/p",
      version: 1,
      processing: false,
      progress: null,
      can_open_folders: true,
      app_version: "0.0.0",
      can_check_updates: false,
      decisions_seq: server.log.length,
    }),
    photos: async () => {
      const snapshot = server.photos.map((item) => ({ ...item, decision: { ...item.decision } }));
      if (server.photosGate) await server.photosGate;
      return snapshot;
    },
    open: async () => ({
      root: "/p",
      version: 2,
      processing: false,
      progress: null,
      can_open_folders: true,
      app_version: "0.0.0",
      can_check_updates: false,
      decisions_seq: server.log.length,
    }),
    decisionsSince: async (since: number) => {
      const reply = { seq: server.log.length, changes: server.log.slice(since) };
      if (server.pullGate) await server.pullGate;
      return reply;
    },
    decide: async (changes: DecisionChange[]) => {
      if (server.unreachable) throw new Error("offline");
      server.decided.push(changes);
      for (const change of changes) record(change.id, change.decision);
      return { applied: changes.length, missing: [] };
    },
  },
}));

const { LibraryStore } = await import("./library.svelte");

function serverState(root: string, version = 1) {
  return {
    root,
    version,
    processing: false,
    progress: null,
    can_open_folders: true,
    decisions_seq: 0,
    app_version: "0.0.0",
    can_check_updates: false,
  };
}

async function loaded() {
  const store = new LibraryStore();
  store.server = serverState("/p");
  await store.refresh();
  return store;
}

describe("LibraryStore", () => {
  beforeEach(() => {
    server.photos = [photo("a", 0), photo("b", 1)];
    server.decided = [];
    server.log = [];
    server.photosGate = null;
    server.pullGate = null;
    server.unreachable = false;
  });

  it("syncs decisions and undoes a multi-photo change at once", async () => {
    const store = await loaded();
    store.decide([
      { id: "a", decision: { mark: "approved", favorite: false, rating: 0 } },
      { id: "b", decision: { mark: "delete", favorite: false, rating: 0 } },
    ]);
    await store.flush();
    expect(server.photos.map((item) => item.decision.mark)).toEqual(["approved", "delete"]);
    store.undo();
    await store.flush();
    expect(server.photos.map((item) => item.decision.mark)).toEqual([null, null]);
    expect(store.pendingCount).toBe(0);
  });

  it("keeps a mark made while a stale refresh was in flight", async () => {
    const store = await loaded();
    let release: () => void = () => undefined;
    server.photosGate = new Promise((resolve) => (release = resolve));
    const refreshing = store.refresh();
    store.decide([{ id: "a", decision: { mark: "approved", favorite: false, rating: 0 } }]);
    await store.flush();
    release();
    await refreshing;
    expect(store.photo("a")?.decision.mark).toBe("approved");
  });

  it("flushes pending marks before opening another folder", async () => {
    const store = await loaded();
    store.decide([{ id: "b", decision: { mark: "unsure", favorite: false, rating: 0 } }]);
    await store.open("/other");
    expect(server.decided.flat().some((change) => change.id === "b")).toBe(true);
  });

  it("marks for another folder stay queued", async () => {
    const store = await loaded();
    server.unreachable = true;
    store.decide([{ id: "a", decision: { mark: "delete", favorite: false, rating: 0 } }]);
    await store.flush();
    expect(store.pendingCount).toBe(1);
    server.unreachable = false;
    store.server = serverState("/q");
    store.setPhotos(store.photos, "/q");
    store.decide([{ id: "b", decision: { mark: "approved", favorite: false, rating: 0 } }]);
    await store.flush();
    expect(server.decided.flat().map((change) => change.id)).toEqual(["b"]);
    expect(store.pendingCount).toBe(1);
    store.server = serverState("/p");
    await store.flush();
    expect(server.decided.flat().map((change) => change.id)).toEqual(["b", "a"]);
    expect(store.pendingCount).toBe(0);
  });

  it("tags marks with the folder of the list on screen during a folder switch", async () => {
    const store = await loaded();
    store.server = serverState("/q", 2);
    server.unreachable = true;
    store.decide([{ id: "a", decision: { mark: "delete", favorite: false, rating: 0 } }]);
    await store.flush();
    server.unreachable = false;
    await store.flush();
    expect(server.decided.flat().map((change) => change.id)).toEqual([]);
    expect(store.pendingCount).toBe(1);
  });

  it("applies marks made on another client without a rescan", async () => {
    const store = new LibraryStore();
    await store.poll();
    store.stop();
    record("b", { mark: "delete", favorite: true, rating: 2 });
    await store.poll();
    store.stop();
    expect(store.photo("b")?.decision).toEqual({ mark: "delete", favorite: true, rating: 2 });
  });

  it("keeps a mark made while a stale pull was in flight", async () => {
    const store = new LibraryStore();
    await store.poll();
    store.stop();
    record("a", { mark: "delete", favorite: false, rating: 0 });
    let release: () => void = () => undefined;
    server.pullGate = new Promise((resolve) => (release = resolve));
    const pulling = store.pullDecisions();
    store.decide([{ id: "a", decision: { mark: "approved", favorite: false, rating: 0 } }]);
    await store.flush();
    release();
    await pulling;
    expect(store.photo("a")?.decision.mark).toBe("approved");
  });

  it("keeps pending local marks over remote changes", async () => {
    const store = new LibraryStore();
    await store.poll();
    store.stop();
    server.unreachable = true;
    store.decide([{ id: "a", decision: { mark: "approved", favorite: false, rating: 0 } }]);
    record("a", { mark: "delete", favorite: false, rating: 0 });
    record("b", { mark: "unsure", favorite: false, rating: 0 });
    await store.poll();
    store.stop();
    expect(store.photo("a")?.decision.mark).toBe("approved");
    expect(store.photo("b")?.decision.mark).toBe("unsure");
  });

  it("sends only the fields changed on this client once it syncs", async () => {
    const store = new LibraryStore();
    await store.poll();
    store.stop();
    server.unreachable = true;
    store.decide([{ id: "a", decision: { mark: "approved", favorite: false, rating: 0 } }]);
    record("a", { mark: null, favorite: true, rating: 4 });
    await store.poll();
    store.stop();
    expect(store.photo("a")?.decision).toEqual({ mark: "approved", favorite: true, rating: 4 });
    server.unreachable = false;
    await store.flush();
    expect(server.photos.find((item) => item.id === "a")?.decision).toEqual({
      mark: "approved",
      favorite: true,
      rating: 4,
    });
    expect(store.pendingCount).toBe(0);
  });
});
