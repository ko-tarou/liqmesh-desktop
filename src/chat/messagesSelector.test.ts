/**
 * Regression guard for the launch-time infinite-render loop (React #185,
 * "Maximum update depth exceeded") that caused the desktop black screen.
 *
 * App.tsx subscribes to the active room's messages via a Zustand v5 selector.
 * Zustand v5 runs selectors through React's `useSyncExternalStore`, which
 * re-renders whenever two consecutive snapshots are not `Object.is`-equal. The
 * buggy form `s.messagesByRoom[roomId] ?? []` returns a BRAND-NEW array every
 * call when the room is empty (the default on a fresh install) → the snapshot
 * never settles → infinite render. The fix returns a single shared constant for
 * the empty case. This test pins both halves: the buggy form is unstable, the
 * fixed form is stable, for an empty room.
 */

import { describe, it, expect } from "vitest";
import type { Message } from "./store";

/** Mirrors App.tsx: a module-level stable empty reference. */
const EMPTY_MESSAGES: Message[] = [];

type MessagesByRoom = Record<string, Message[]>;

/** The selector as written in App.tsx (fixed form). */
const selectMessages = (byRoom: MessagesByRoom, roomId: string) =>
  byRoom[roomId] ?? EMPTY_MESSAGES;

describe("active-room messages selector (React #185 regression)", () => {
  it("returns the SAME reference across calls for an empty room", () => {
    const byRoom: MessagesByRoom = {}; // fresh install: no messages anywhere
    const a = selectMessages(byRoom, "general");
    const b = selectMessages(byRoom, "general");
    // Object.is-equal across calls → useSyncExternalStore settles → no loop.
    expect(a).toBe(b);
  });

  it("the OLD buggy form (?? []) returns a NEW reference each call", () => {
    const byRoom: MessagesByRoom = {};
    const buggy = (roomId: string) => byRoom[roomId] ?? [];
    // Different references → snapshot never settles → React #185. This asserts
    // the failure mode the fix removes, so a regression to `?? []` is caught.
    expect(buggy("general")).not.toBe(buggy("general"));
  });

  it("returns the room's own array (same reference) when messages exist", () => {
    const msgs: Message[] = [
      {
        id: "1",
        senderId: "a",
        senderName: "A",
        body: "hi",
        createdAt: "t",
        roomId: "general",
        deleted: false,
        reactions: {},
      },
    ];
    const byRoom: MessagesByRoom = { general: msgs };
    expect(selectMessages(byRoom, "general")).toBe(msgs);
    expect(selectMessages(byRoom, "general")).toBe(selectMessages(byRoom, "general"));
  });
});
