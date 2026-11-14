import { beforeEach, describe, expect, it, vi } from "vitest";
import { photo } from "./fixtures";
import type { DecisionChange, Photo } from "./types";

const server = {
  photos: [] as Photo[],
  decided: [] as DecisionChange[][],
  photosGate: null as Promise<void> | null,
  unreachable: false,
};

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
    state: async () => ({ root: "/p", version: 1, processing: false, progress: null }),
    photos: async () => {
      const snapshot = server.photos.map((item) => ({ ...item, decision: { ...item.decision } }));
      if (server.photosGate) await server.photosGate;
      return snapshot;
    },
    open: async () => ({ root: "/p", version: 2, processing: false, progress: null }),
    decide: async (changes: DecisionChange[]) => {
      if (server.unreachable) throw new Error("offline");
      server.decided.push(changes);
      for (const change of changes) {
        const target = server.photos.find((item) => item.id === change.id);
        if (target) target.decision = change.decision;
      }
      return { applied: changes.length, missing: [] };
    },
  },
}));

const { LibraryStore } = await import("./library.svelte");

async function loaded() {
  const store = new LibraryStore();
  store.server = { root: "/p", version: 1, processing: false, progress: null };
  await store.refresh();
  return store;
}

describe("LibraryStore", () => {
  beforeEach(() => {
    server.photos = [photo("a", 0), photo("b", 1)];
    server.decided = [];
    server.photosGate = null;
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
    store.server = { root: "/q", version: 1, processing: false, progress: null };
    store.decide([{ id: "b", decision: { mark: "approved", favorite: false, rating: 0 } }]);
    await store.flush();
    expect(server.decided.flat().map((change) => change.id)).toEqual(["b"]);
    expect(store.pendingCount).toBe(1);
    store.server = { root: "/p", version: 1, processing: false, progress: null };
    await store.flush();
    expect(server.decided.flat().map((change) => change.id)).toEqual(["b", "a"]);
    expect(store.pendingCount).toBe(0);
  });
});
