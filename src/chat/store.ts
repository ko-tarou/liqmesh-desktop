/**
 * Pure, React-independent chat message store (sans-IO reducer).
 *
 * The store applies wire `Frame`s to an immutable `ChatState`. Every public
 * function returns a NEW state and never mutates its inputs, so it composes
 * cleanly with Zustand / React (wired up in PR-C1b) and is trivially testable.
 *
 * Invariants enforced here:
 *  - dedup:        a `msg` with an already-stored id is dropped (idempotent;
 *                  protects against backfill / re-delivery duplicates).
 *  - order:        messages in a room are kept sorted by `createdAt` ascending,
 *                  tie-broken by `id` ascending (stable, total order).
 *  - delete auth:  a `delete` only takes effect when its `senderId` matches the
 *                  original message's `senderId` (anti-spoofing). Tombstone =
 *                  `deleted: true`, `body: ""`; reactions are cleared.
 *  - read LWW:     `read` is last-write-wins per (roomId, senderId).
 *  - hello:        no-op for the message store (presence handled separately).
 *  - unknown ids:  reaction/delete against an unknown messageId are no-ops.
 */

import {
  type Frame,
  type MsgFrame,
  type ReactionFrame,
  type DeleteFrame,
  type ReadFrame,
  normalizeRoomId,
} from "./frames";

// ---- domain model --------------------------------------------------------

export type Message = {
  id: string;
  senderId: string;
  senderName: string;
  body: string;
  createdAt: string;
  roomId: string;
  replyToId?: string;
  /** delete tombstone */
  deleted: boolean;
  /** emoji -> senderId[] (no duplicates, stable insertion order) */
  reactions: Record<string, string[]>;
};

export type ChatState = {
  /** roomId -> messages sorted by createdAt asc (ties by id asc) */
  messagesByRoom: Record<string, Message[]>;
  /** roomId -> senderId -> upToMessageId */
  reads: Record<string, Record<string, string>>;
};

export const initialState: ChatState = { messagesByRoom: {}, reads: {} };

// ---- read helpers --------------------------------------------------------

/** Messages for a room (empty array if the room is unknown). */
export function messagesIn(state: ChatState, roomId: string): Message[] {
  return state.messagesByRoom[roomId] ?? [];
}

// ---- internal helpers ----------------------------------------------------

/**
 * Total order over messages.
 *
 * Primary key: the instant `createdAt` denotes. We first compare by parsed epoch
 * (`Date.parse`) so that two equivalent timestamps written in different formats
 * (e.g. `"…Z"` vs `"+09:00"`) collate identically across platforms. If either
 * side fails to parse, or the epochs tie, we fall back to a lexical comparison of
 * the raw `createdAt` strings, and finally to `id` as a stable, total tie-break.
 */
function compareMessages(a: Message, b: Message): number {
  const ea = Date.parse(a.createdAt);
  const eb = Date.parse(b.createdAt);
  if (!Number.isNaN(ea) && !Number.isNaN(eb) && ea !== eb) {
    return ea < eb ? -1 : 1;
  }
  if (a.createdAt < b.createdAt) return -1;
  if (a.createdAt > b.createdAt) return 1;
  if (a.id < b.id) return -1;
  if (a.id > b.id) return 1;
  return 0;
}

/**
 * Insert `message` into the room, deduping by id and keeping the ordering
 * invariant. Returns a new state; if the id already exists the input state is
 * returned unchanged (idempotent).
 */
function insertMessage(state: ChatState, message: Message): ChatState {
  const existing = state.messagesByRoom[message.roomId] ?? [];
  if (existing.some((m) => m.id === message.id)) {
    return state; // dedup: drop duplicate / backfill
  }
  const next = [...existing, message].sort(compareMessages);
  return {
    ...state,
    messagesByRoom: { ...state.messagesByRoom, [message.roomId]: next },
  };
}

/** Find the (roomId, index) of a message by id across all rooms, or null. */
function locate(
  state: ChatState,
  messageId: string,
): { roomId: string; index: number } | null {
  for (const roomId of Object.keys(state.messagesByRoom)) {
    const index = state.messagesByRoom[roomId].findIndex((m) => m.id === messageId);
    if (index !== -1) return { roomId, index };
  }
  return null;
}

/** Immutably replace the message at (roomId, index) using `transform`. */
function updateMessageAt(
  state: ChatState,
  roomId: string,
  index: number,
  transform: (m: Message) => Message,
): ChatState {
  const room = state.messagesByRoom[roomId];
  const nextRoom = room.slice();
  nextRoom[index] = transform(room[index]);
  return {
    ...state,
    messagesByRoom: { ...state.messagesByRoom, [roomId]: nextRoom },
  };
}

// ---- frame handlers ------------------------------------------------------

function applyMsg(state: ChatState, frame: MsgFrame): ChatState {
  const message: Message = {
    id: frame.id,
    senderId: frame.senderId,
    senderName: frame.senderName,
    body: frame.body,
    createdAt: frame.createdAt,
    roomId: normalizeRoomId(frame.roomId),
    ...(frame.replyToId !== undefined ? { replyToId: frame.replyToId } : {}),
    deleted: false,
    reactions: {},
  };
  return insertMessage(state, message);
}

function applyReaction(state: ChatState, frame: ReactionFrame): ChatState {
  const found = locate(state, frame.messageId);
  if (!found) return state; // unknown messageId -> no-op

  return updateMessageAt(state, found.roomId, found.index, (m) => {
    const current = m.reactions[frame.emoji] ?? [];

    if (frame.op === "add") {
      if (current.includes(frame.senderId)) return m; // idempotent add
      return {
        ...m,
        reactions: { ...m.reactions, [frame.emoji]: [...current, frame.senderId] },
      };
    }

    if (frame.op === "remove") {
      if (!current.includes(frame.senderId)) return m; // nothing to remove
      const remaining = current.filter((id) => id !== frame.senderId);
      const reactions = { ...m.reactions };
      if (remaining.length === 0) {
        delete reactions[frame.emoji]; // prune empty emoji key
      } else {
        reactions[frame.emoji] = remaining;
      }
      return { ...m, reactions };
    }

    // Unknown op (e.g. a future "toggle"): ignore rather than guessing. Strict
    // handling keeps cross-platform behaviour predictable / forward-compatible.
    return m;
  });
}

function applyDelete(state: ChatState, frame: DeleteFrame): ChatState {
  const found = locate(state, frame.messageId);
  if (!found) return state; // unknown messageId -> no-op

  const target = state.messagesByRoom[found.roomId][found.index];
  // Authorization: only the original sender may delete (anti-spoofing).
  if (target.senderId !== frame.senderId) return state;
  if (target.deleted) return state; // already a tombstone -> idempotent

  return updateMessageAt(state, found.roomId, found.index, (m) => ({
    ...m,
    deleted: true,
    body: "",
    reactions: {},
  }));
}

function applyRead(state: ChatState, frame: ReadFrame): ChatState {
  const roomId = normalizeRoomId(frame.roomId);
  const roomReads = state.reads[roomId] ?? {};
  return {
    ...state,
    reads: {
      ...state.reads,
      [roomId]: { ...roomReads, [frame.senderId]: frame.upToMessageId }, // last-write-wins
    },
  };
}

// ---- public reducer ------------------------------------------------------

/**
 * Apply a single wire frame to the state, returning a new immutable state.
 * Unknown / no-op frames return the input state unchanged.
 */
export function applyFrame(state: ChatState, frame: Frame): ChatState {
  switch (frame.type) {
    case "msg":
      return applyMsg(state, frame);
    case "reaction":
      return applyReaction(state, frame);
    case "delete":
      return applyDelete(state, frame);
    case "read":
      return applyRead(state, frame);
    case "hello":
      // Presence is handled separately (PR-C1b+); no message-store effect.
      return state;
    default: {
      // Forward compatibility: unknown frame types are ignored. The exhaustive
      // check keeps the union honest at compile time.
      const _exhaustive: never = frame;
      void _exhaustive;
      return state;
    }
  }
}

/**
 * Optimistically add a locally-composed outgoing message. Shares the exact
 * normalization / dedup / ordering rules as the inbound `msg` path, so a later
 * echo of the same id over the wire is a no-op.
 */
export function addLocalMessage(
  state: ChatState,
  msg: Omit<Message, "deleted" | "reactions">,
): ChatState {
  const message: Message = {
    ...msg,
    roomId: normalizeRoomId(msg.roomId),
    deleted: false,
    reactions: {},
  };
  return insertMessage(state, message);
}
