import { describe, it, expect, beforeEach } from "vitest";
import { buildDemoSeed } from "./seedDemo";
import { DEFAULT_ROOM_ID } from "./frames";

// Minimal in-memory localStorage so the seed's persisted flag works under the
// (DOM-less) vitest node environment.
function installFakeLocalStorage() {
  const map = new Map<string, string>();
  (globalThis as unknown as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as Storage;
}

describe("buildDemoSeed", () => {
  beforeEach(installFakeLocalStorage);

  it("seeds a non-trivial disaster-mesh conversation into general on first run", () => {
    const now = 1_749_200_000_000;
    const seeds = buildDemoSeed(now, false);
    expect(seeds.length).toBeGreaterThanOrEqual(20);
    expect(seeds.every((m) => m.roomId === DEFAULT_ROOM_ID)).toBe(true);
    // createdAt is epoch-ms (number) in the past, ascending.
    expect(seeds.every((m) => typeof m.createdAt === "number" && m.createdAt < now)).toBe(true);
    for (let i = 1; i < seeds.length; i++) {
      expect(seeds[i].createdAt).toBeGreaterThanOrEqual(seeds[i - 1].createdAt);
    }
    // Varied senders (5-6) including a high-urgency one.
    const senders = new Set(seeds.map((m) => m.senderName));
    expect(senders.size).toBeGreaterThanOrEqual(5);
    expect(senders.has("消防団")).toBe(true);
  });

  it("is idempotent: returns [] on the second run (persisted flag)", () => {
    const now = 1_749_200_000_000;
    expect(buildDemoSeed(now, false).length).toBeGreaterThan(0);
    expect(buildDemoSeed(now, false)).toEqual([]); // flag set → no re-seed
  });

  it("never seeds when the general room already has messages", () => {
    expect(buildDemoSeed(1_749_200_000_000, true)).toEqual([]);
  });

  it("uses unique ids so the store dedup never collapses seeds", () => {
    const seeds = buildDemoSeed(1_749_200_000_000, false);
    expect(new Set(seeds.map((m) => m.id)).size).toBe(seeds.length);
  });
});
