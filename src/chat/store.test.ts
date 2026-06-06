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
  type ChatState,
} from "./store";

// ---- helpers -------------------------------------------------------------

function msg(over: Partial<MsgFrame> & { id: string }): MsgFrame {
  return {
    type: "msg",
    senderId: "alice",
    senderName: "Alice",
    body: "hello",
    createdAt: "2024-01-01T00:00:00.000Z",
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
    s = applyFrame(s, msg({ id: "c", roomId: "r1", createdAt: "2024-01-03T00:00:00.000Z" }));
    s = applyFrame(s, msg({ id: "a", roomId: "r1", createdAt: "2024-01-01T00:00:00.000Z" }));
    s = applyFrame(s, msg({ id: "b", roomId: "r1", createdAt: "2024-01-02T00:00:00.000Z" }));
    expect(messagesIn(s, "r1").map((m) => m.id)).toEqual(["a", "b", "c"]);
  });

  it("tie-breaks equal createdAt by id ascending", () => {
    let s = initialState;
    s = applyFrame(s, msg({ id: "z", roomId: "r1", createdAt: "2024-01-01T00:00:00.000Z" }));
    s = applyFrame(s, msg({ id: "a", roomId: "r1", createdAt: "2024-01-01T00:00:00.000Z" }));
    s = applyFrame(s, msg({ id: "m", roomId: "r1", createdAt: "2024-01-01T00:00:00.000Z" }));
    expect(messagesIn(s, "r1").map((m) => m.id)).toEqual(["a", "m", "z"]);
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

// ---- hello ---------------------------------------------------------------

describe("applyFrame: hello", () => {
  it("is a no-op for the message store", () => {
    const frame: Frame = { type: "hello", senderId: "alice", senderName: "Alice", protoVer: 1 };
    const s = applyFrame(initialState, frame);
    expect(s).toEqual(initialState);
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
      createdAt: "2024-01-01T00:00:00.000Z",
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
      createdAt: "2024-01-01T00:00:00.000Z",
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
