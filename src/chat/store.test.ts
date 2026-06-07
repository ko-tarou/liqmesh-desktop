import { describe, it, expect } from "vitest";
import type {
  Frame,
  MsgFrame,
  ReactionFrame,
  DeleteFrame,
  ReadFrame,
} from "./frames";
import {
  initialState,
  applyFrame,
  addLocalMessage,
  messagesIn,
  addRoom,
  roomList,
  peerName,
  peerReadUpTo,
  unreadCount,
  type ChatState,
} from "./store";
import { DEFAULT_ROOM_ID } from "./frames";

// ---- helpers -------------------------------------------------------------

function msg(over: Partial<MsgFrame> & { id: string }): MsgFrame {
  return {
    type: "msg",
    senderId: "alice",
    senderName: "Alice",
    body: "hello",
    createdAt: 1_704_067_200_000, // 2024-01-01T00:00:00Z in epoch ms
    ...over,
  };
}

function reaction(over: Partial<ReactionFrame> & { messageId: string }): ReactionFrame {
  return {
    type: "reaction",
    senderId: "alice",
    emoji: "👍",
    op: "add",
    ...over,
  };
}

function del(over: Partial<DeleteFrame> & { messageId: string }): DeleteFrame {
  return { type: "delete", senderId: "alice", ...over };
}

function read(over: Partial<ReadFrame> & { upToMessageId: string }): ReadFrame {
  return { type: "read", senderId: "alice", ...over };
}

// ---- msg -----------------------------------------------------------------

describe("applyFrame: msg", () => {
  it("adds a message (1)", () => {
    const s = applyFrame(initialState, msg({ id: "m1", roomId: "r1" }));
    expect(messagesIn(s, "r1")).toHaveLength(1);
    expect(messagesIn(s, "r1")[0]).toMatchObject({
      id: "m1",
      senderId: "alice",
      body: "hello",
      deleted: false,
      reactions: {},
    });
  });

  it("is idempotent on duplicate id (2)", () => {
    let s = applyFrame(initialState, msg({ id: "m1", roomId: "r1", body: "first" }));
    s = applyFrame(s, msg({ id: "m1", roomId: "r1", body: "DIFFERENT" }));
    expect(messagesIn(s, "r1")).toHaveLength(1);
    // dedup keeps the original content; backfill duplicates are dropped
    expect(messagesIn(s, "r1")[0].body).toBe("first");
  });

  it("sorts by createdAt ascending regardless of apply order (3)", () => {
    let s = initialState;
    s = applyFrame(s, msg({ id: "c", roomId: "r1", createdAt: 1_704_240_000_000 }));
    s = applyFrame(s, msg({ id: "a", roomId: "r1", createdAt: 1_704_067_200_000 }));
    s = applyFrame(s, msg({ id: "b", roomId: "r1", createdAt: 1_704_153_600_000 }));
    expect(messagesIn(s, "r1").map((m) => m.id)).toEqual(["a", "b", "c"]);
  });

  it("tie-breaks equal createdAt by id ascending", () => {
    let s = initialState;
    s = applyFrame(s, msg({ id: "z", roomId: "r1", createdAt: 1_704_067_200_000 }));
    s = applyFrame(s, msg({ id: "a", roomId: "r1", createdAt: 1_704_067_200_000 }));
    s = applyFrame(s, msg({ id: "m", roomId: "r1", createdAt: 1_704_067_200_000 }));
    expect(messagesIn(s, "r1").map((m) => m.id)).toEqual(["a", "m", "z"]);
  });

  it("orders by epoch across differing valid timestamp formats (A#1)", () => {
    let s = initialState;
    // "+09:00" 08:00 == 23:00Z the day before -> must sort before the 00:00Z one.
    s = applyFrame(s, msg({ id: "late", roomId: "r1", createdAt: 1_749_168_000_000 }));
    s = applyFrame(s, msg({ id: "early", roomId: "r1", createdAt: 1_749_164_400_000 }));
    expect(messagesIn(s, "r1").map((m) => m.id)).toEqual(["early", "late"]);
  });

  it("is stable for equal instants in different formats, tie-broken by id (A#1)", () => {
    let s = initialState;
    // 09:00+09:00 == 00:00Z -> same epoch; falls through to id tie-break.
    s = applyFrame(s, msg({ id: "z", roomId: "r1", createdAt: 1_749_168_000_000 }));
    s = applyFrame(s, msg({ id: "a", roomId: "r1", createdAt: 1_749_168_000_000 }));
    expect(messagesIn(s, "r1").map((m) => m.id)).toEqual(["a", "z"]);
  });

  it("orders a zero/missing timestamp before real ones, tie-broken by id", () => {
    // createdAt is now epoch ms (number). A 0 (e.g. a lenient-decoded legacy/bad
    // value) simply sorts earliest; equal values fall through to the id tie-break.
    let s = initialState;
    s = applyFrame(s, msg({ id: "b", roomId: "r1", createdAt: 0 }));
    s = applyFrame(s, msg({ id: "a", roomId: "r1", createdAt: 0 }));
    s = applyFrame(s, msg({ id: "c", roomId: "r1", createdAt: 1_704_067_200_000 }));
    expect(messagesIn(s, "r1").map((m) => m.id)).toEqual(["a", "b", "c"]);
  });

  it("defaults missing/empty roomId to 'general' (4)", () => {
    let s = applyFrame(initialState, msg({ id: "m1" })); // no roomId
    s = applyFrame(s, msg({ id: "m2", roomId: "" })); // empty roomId
    expect(messagesIn(s, "general")).toHaveLength(2);
  });

  it("preserves replyToId (10)", () => {
    const s = applyFrame(initialState, msg({ id: "m1", roomId: "r1", replyToId: "parent" }));
    expect(messagesIn(s, "r1")[0].replyToId).toBe("parent");
  });
});

// ---- reaction ------------------------------------------------------------

describe("applyFrame: reaction", () => {
  function base(): ChatState {
    return applyFrame(initialState, msg({ id: "m1", roomId: "r1" }));
  }

  it("adds a reaction, is idempotent per sender, removes, and prunes empty emoji key (5)", () => {
    let s = base();
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "👍", senderId: "alice", op: "add" }));
    expect(messagesIn(s, "r1")[0].reactions).toEqual({ "👍": ["alice"] });

    // duplicate add by same sender is idempotent
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "👍", senderId: "alice", op: "add" }));
    expect(messagesIn(s, "r1")[0].reactions).toEqual({ "👍": ["alice"] });

    // remove -> empty array -> key pruned
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "👍", senderId: "alice", op: "remove" }));
    expect(messagesIn(s, "r1")[0].reactions).toEqual({});
  });

  it("aggregates multiple senders for the same emoji (6)", () => {
    let s = base();
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "🎉", senderId: "alice", op: "add" }));
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "🎉", senderId: "bob", op: "add" }));
    expect(messagesIn(s, "r1")[0].reactions).toEqual({ "🎉": ["alice", "bob"] });
  });

  it("is a no-op for an unknown messageId (9)", () => {
    const s = base();
    const next = applyFrame(s, reaction({ messageId: "nope", emoji: "👍", op: "add" }));
    expect(next).toEqual(s);
  });

  it("removing an absent sender is a no-op and does not throw", () => {
    let s = base();
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "👍", senderId: "alice", op: "add" }));
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "👍", senderId: "ghost", op: "remove" }));
    expect(messagesIn(s, "r1")[0].reactions).toEqual({ "👍": ["alice"] });
  });

  it("treats a non-remove op as add (lenient; matches phones/contract) (A#2)", () => {
    // Contract: ONLY op === "remove" removes; anything else (incl. unknown or
    // missing) is an add. A new sender with an unknown op must be ADDED.
    let s = base();
    s = applyFrame(s, reaction({ messageId: "m1", emoji: "👍", senderId: "alice", op: "add" }));
    const unknownOp = reaction({ messageId: "m1", emoji: "👍", senderId: "bob" });
    const next = applyFrame(s, { ...unknownOp, op: "toggle" as ReactionFrame["op"] });
    expect(messagesIn(next, "r1")[0].reactions).toEqual({ "👍": ["alice", "bob"] });
  });

  it("treats a missing/empty op as add (lenient)", () => {
    let s = base();
    const noOp = reaction({ messageId: "m1", emoji: "🎉", senderId: "alice" });
    const next = applyFrame(s, { ...noOp, op: "" as ReactionFrame["op"] });
    expect(messagesIn(next, "r1")[0].reactions).toEqual({ "🎉": ["alice"] });
  });
});

// ---- delete --------------------------------------------------------------

describe("applyFrame: delete", () => {
  function base(): ChatState {
    return applyFrame(initialState, msg({ id: "m1", roomId: "r1", senderId: "alice", body: "secret" }));
  }

  it("tombstones when the original sender deletes (7)", () => {
    const s = applyFrame(base(), del({ messageId: "m1", senderId: "alice" }));
    const m = messagesIn(s, "r1")[0];
    expect(m.deleted).toBe(true);
    expect(m.body).toBe("");
  });

  it("ignores a delete from a different sender (authorization) (7)", () => {
    const s0 = base();
    const s = applyFrame(s0, del({ messageId: "m1", senderId: "mallory" }));
    expect(s).toEqual(s0);
    expect(messagesIn(s, "r1")[0].deleted).toBe(false);
  });

  it("is a no-op for an unknown messageId (7/9)", () => {
    const s0 = base();
    const s = applyFrame(s0, del({ messageId: "ghost", senderId: "alice" }));
    expect(s).toEqual(s0);
  });
});

// ---- read ----------------------------------------------------------------

describe("applyFrame: read", () => {
  it("records and last-write-wins overwrites (8)", () => {
    let s = applyFrame(initialState, read({ roomId: "r1", upToMessageId: "m1", senderId: "alice" }));
    expect(s.reads.r1.alice).toBe("m1");
    s = applyFrame(s, read({ roomId: "r1", upToMessageId: "m5", senderId: "alice" }));
    expect(s.reads.r1.alice).toBe("m5");
  });

  it("normalizes missing roomId to 'general'", () => {
    const s = applyFrame(initialState, read({ upToMessageId: "m1", senderId: "alice" }));
    expect(s.reads.general.alice).toBe("m1");
  });
});

// ---- peerReadUpTo (C3) ---------------------------------------------------

describe("peerReadUpTo (C3)", () => {
  it("returns undefined when the peer has no read mark for the room", () => {
    expect(peerReadUpTo(initialState, "r1", "alice")).toBeUndefined();
  });

  it("returns the high-water-mark and reflects last-write-wins", () => {
    let s = applyFrame(initialState, read({ roomId: "r1", upToMessageId: "m1", senderId: "alice" }));
    expect(peerReadUpTo(s, "r1", "alice")).toBe("m1");
    s = applyFrame(s, read({ roomId: "r1", upToMessageId: "m9", senderId: "alice" }));
    expect(peerReadUpTo(s, "r1", "alice")).toBe("m9");
  });

  it("normalizes an empty roomId to the default room", () => {
    const s = applyFrame(initialState, read({ upToMessageId: "m3", senderId: "bob" }));
    expect(peerReadUpTo(s, "", "bob")).toBe("m3");
  });
});

// ---- unreadCount (C3) ----------------------------------------------------

describe("unreadCount (C3)", () => {
  // Three messages from alice (the peer); "me" is bob.
  function withPeerMessages(): ChatState {
    let s = applyFrame(initialState, msg({ id: "a", roomId: "r1", createdAt: 1_704_067_201_000 }));
    s = applyFrame(s, msg({ id: "b", roomId: "r1", createdAt: 1_704_067_202_000 }));
    s = applyFrame(s, msg({ id: "c", roomId: "r1", createdAt: 1_704_067_203_000 }));
    return s;
  }

  it("is 0 for an empty / unknown room", () => {
    expect(unreadCount(initialState, "r1", "bob")).toBe(0);
  });

  it("counts every other-sender message when I have no read mark", () => {
    expect(unreadCount(withPeerMessages(), "r1", "bob")).toBe(3);
  });

  it("counts only messages after my own read high-water-mark", () => {
    let s = withPeerMessages();
    s = applyFrame(s, read({ roomId: "r1", upToMessageId: "b", senderId: "bob" })); // I read up to b
    expect(unreadCount(s, "r1", "bob")).toBe(1); // only c remains
  });

  it("excludes my own messages and tombstones", () => {
    let s = withPeerMessages();
    s = applyFrame(s, msg({ id: "mine", roomId: "r1", senderId: "bob", createdAt: 1_704_067_204_000 }));
    s = applyFrame(s, del({ messageId: "c", senderId: "alice" })); // alice deletes her own c
    // a, b remain unread (mine excluded, c tombstoned).
    expect(unreadCount(s, "r1", "bob")).toBe(2);
  });
});

// ---- hello ---------------------------------------------------------------

describe("applyFrame: hello", () => {
  it("records presence (peer name) without touching messages or rooms", () => {
    const frame: Frame = { type: "hello", senderId: "alice", senderName: "Alice", protoVer: 1 };
    const s = applyFrame(initialState, frame);
    // presence recorded
    expect(peerName(s, "alice")).toBe("Alice");
    // no message-store side effects
    expect(s.messagesByRoom).toEqual(initialState.messagesByRoom);
    expect(s.reads).toEqual(initialState.reads);
    // rooms untouched (hello does not introduce rooms)
    expect(s.rooms).toEqual(initialState.rooms);
  });

  it("updates an existing peer's name (latest-wins presence)", () => {
    let s = applyFrame(initialState, {
      type: "hello",
      senderId: "alice",
      senderName: "Alice",
      protoVer: 1,
    });
    s = applyFrame(s, { type: "hello", senderId: "alice", senderName: "Alice 2", protoVer: 1 });
    expect(peerName(s, "alice")).toBe("Alice 2");
  });
});

// ---- addLocalMessage -----------------------------------------------------

describe("addLocalMessage", () => {
  it("optimistically adds a local message with the same normalization", () => {
    const s = addLocalMessage(initialState, {
      id: "local1",
      senderId: "me",
      senderName: "Me",
      body: "hi",
      createdAt: 1_704_067_200_000,
      roomId: "",
    });
    expect(messagesIn(s, "general")).toHaveLength(1);
    expect(messagesIn(s, "general")[0]).toMatchObject({ id: "local1", deleted: false, reactions: {} });
  });

  it("dedups against an already-stored id", () => {
    let s = addLocalMessage(initialState, {
      id: "x",
      senderId: "me",
      senderName: "Me",
      body: "hi",
      createdAt: 1_704_067_200_000,
      roomId: "r1",
    });
    // echo back via the wire path -> still one
    s = applyFrame(s, msg({ id: "x", roomId: "r1", senderId: "me", body: "echo" }));
    expect(messagesIn(s, "r1")).toHaveLength(1);
  });
});

// ---- immutability --------------------------------------------------------

describe("immutability (11)", () => {
  it("does not mutate the prior state object", () => {
    const before = applyFrame(initialState, msg({ id: "m1", roomId: "r1" }));
    const snapshot = structuredClone(before);

    const after = applyFrame(before, reaction({ messageId: "m1", emoji: "👍", op: "add" }));

    // new object identity
    expect(after).not.toBe(before);
    // prior state untouched (deep)
    expect(before).toEqual(snapshot);
    expect(messagesIn(before, "r1")[0].reactions).toEqual({});
  });

  it("initialState is never mutated", () => {
    const snapshot = structuredClone(initialState);
    applyFrame(initialState, msg({ id: "m1", roomId: "r1" }));
    expect(initialState).toEqual(snapshot);
  });
});

// ---- messagesIn ----------------------------------------------------------

describe("messagesIn", () => {
  it("returns an empty array for an unknown room", () => {
    expect(messagesIn(initialState, "nope")).toEqual([]);
  });
});

// ---- presence / peers (C2) ----------------------------------------------

describe("applyFrame: msg presence + room tracking (C2)", () => {
  it("records the sender's name in peers and keeps it latest-wins", () => {
    let s = applyFrame(initialState, msg({ id: "m1", roomId: "r1", senderId: "bob", senderName: "Bob" }));
    expect(peerName(s, "bob")).toBe("Bob");
    // a later message with a renamed display name updates the peer entry
    s = applyFrame(s, msg({ id: "m2", roomId: "r1", senderId: "bob", senderName: "Bobby" }));
    expect(peerName(s, "bob")).toBe("Bobby");
  });

  it("adds a newly-seen roomId to the known room list (normalized)", () => {
    let s = applyFrame(initialState, msg({ id: "m1", roomId: "r1" }));
    expect(roomList(s)).toContain("r1");
    // default room is always present alongside discovered rooms
    expect(roomList(s)).toContain(DEFAULT_ROOM_ID);

    // an empty roomId normalizes to general and does not add a blank room
    s = applyFrame(s, msg({ id: "m2", roomId: "" }));
    expect(roomList(s)).not.toContain("");
    expect(roomList(s)).toContain(DEFAULT_ROOM_ID);
  });

  it("does not duplicate a roomId already in the list", () => {
    let s = applyFrame(initialState, msg({ id: "m1", roomId: "r1" }));
    s = applyFrame(s, msg({ id: "m2", roomId: "r1" }));
    expect(roomList(s).filter((r) => r === "r1")).toHaveLength(1);
  });
});

describe("addRoom (C2)", () => {
  it("adds a new room", () => {
    const s = addRoom(initialState, "design");
    expect(roomList(s)).toContain("design");
  });

  it("is idempotent (no duplicates)", () => {
    let s = addRoom(initialState, "design");
    s = addRoom(s, "design");
    expect(roomList(s).filter((r) => r === "design")).toHaveLength(1);
  });

  it("normalizes an empty/missing room to general (no blank entry)", () => {
    const s = addRoom(initialState, "");
    expect(roomList(s)).not.toContain("");
    expect(roomList(s)).toContain(DEFAULT_ROOM_ID);
    // general already exists, so nothing new is added
    expect(roomList(s).filter((r) => r === DEFAULT_ROOM_ID)).toHaveLength(1);
  });

  it("does not mutate the prior state", () => {
    const before = structuredClone(initialState);
    addRoom(initialState, "design");
    expect(initialState).toEqual(before);
  });
});

describe("roomList (C2)", () => {
  it("always includes the default room", () => {
    expect(roomList(initialState)).toContain(DEFAULT_ROOM_ID);
  });

  it("includes the default room even if a state somehow lacks it (safety union)", () => {
    const broken: ChatState = { messagesByRoom: {}, reads: {}, rooms: ["r1"], peers: {} };
    expect(roomList(broken)).toContain(DEFAULT_ROOM_ID);
    expect(roomList(broken)).toContain("r1");
  });
});

describe("peerName (C2)", () => {
  it("returns undefined for an unknown peer", () => {
    expect(peerName(initialState, "ghost")).toBeUndefined();
  });
});
