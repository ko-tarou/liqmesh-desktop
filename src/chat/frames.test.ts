import { describe, it, expect } from "vitest";
import { DEFAULT_ROOM_ID, normalizeRoomId } from "./frames";

describe("normalizeRoomId", () => {
  it("returns DEFAULT_ROOM_ID for undefined", () => {
    expect(normalizeRoomId(undefined)).toBe(DEFAULT_ROOM_ID);
  });

  it("returns DEFAULT_ROOM_ID for empty string", () => {
    expect(normalizeRoomId("")).toBe(DEFAULT_ROOM_ID);
  });

  it("passes through a non-empty room id", () => {
    expect(normalizeRoomId("lobby")).toBe("lobby");
  });

  it("uses the canonical 'general' default (not 'default')", () => {
    expect(DEFAULT_ROOM_ID).toBe("general");
  });
});
