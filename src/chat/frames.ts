/**
 * Wire frames as emitted by the Rust BLE layer over the `ble://frame` Tauri event.
 *
 * The Rust `Frame` enum is serialized with `#[serde(tag = "type", rename_all = "camelCase")]`,
 * so every field arrives in camelCase and the discriminant lives on `type`.
 * These types mirror that wire shape exactly (LiqMesh BLE Interop Contract v1.4).
 *
 * Unknown frame types MUST be ignored (forward compatibility) — see the reducer.
 */

export type HelloFrame = {
  type: "hello";
  senderId: string;
  senderName: string;
  protoVer: number;
};

export type MsgFrame = {
  type: "msg";
  id: string;
  senderId: string;
  senderName: string;
  body: string;
  createdAt: string;
  /** Absent/empty -> normalized to DEFAULT_ROOM_ID. */
  roomId?: string;
  replyToId?: string;
};

export type ReactionFrame = {
  type: "reaction";
  messageId: string;
  senderId: string;
  emoji: string;
  op: "add" | "remove";
};

export type DeleteFrame = {
  type: "delete";
  messageId: string;
  senderId: string;
};

export type ReadFrame = {
  type: "read";
  /** Absent/empty -> normalized to DEFAULT_ROOM_ID. */
  roomId?: string;
  upToMessageId: string;
  senderId: string;
};

/** Discriminated union over all known wire frames. */
export type Frame =
  | HelloFrame
  | MsgFrame
  | ReactionFrame
  | DeleteFrame
  | ReadFrame;

/**
 * Canonical default room. Contract v1.2+: when `roomId` is absent or empty,
 * ALL platforms default to the literal string "general" (NOT "default").
 */
export const DEFAULT_ROOM_ID = "general";

/** Normalize a possibly-absent/empty roomId to the canonical default room. */
export function normalizeRoomId(roomId?: string): string {
  return roomId && roomId.length > 0 ? roomId : DEFAULT_ROOM_ID;
}
