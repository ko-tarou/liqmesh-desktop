/**
 * React-facing chat store, built on top of the pure reducer in `./store`.
 *
 * Zustand holds the immutable `ChatState` and exposes thin actions that delegate
 * to the pure functions (`applyFrame` / `addLocalMessage` / `addRoom`). The
 * `persist` middleware mirrors only the serialisable data (`messagesByRoom` /
 * `reads` / `rooms` / `peers` / `activeRoomId`) to `localStorage`, so history,
 * room list and the selected room survive reloads while actions are
 * reconstructed on boot. Keeping all the domain logic in the pure reducer means
 * the existing unit tests still cover the behaviour; this layer is just IO +
 * React glue.
 *
 * Backwards compatibility: older persisted payloads (pre-C2) lack `rooms` /
 * `peers` / `activeRoomId`. Zustand's default `merge` shallow-merges the
 * persisted value over the initializer state, so missing keys keep their
 * initializer defaults (`rooms=[general]`, `peers={}`, `activeRoomId=general`).
 * `roomList` additionally unions DEFAULT_ROOM_ID defensively. No migration
 * needed.
 */

import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";
import type { Frame } from "./frames";
import { DEFAULT_ROOM_ID } from "./frames";
import {
  type ChatState,
  type Message,
  initialState,
  applyFrame as applyFramePure,
  addLocalMessage as addLocalMessagePure,
  addRoom as addRoomPure,
  messagesIn as messagesInPure,
  roomList as roomListPure,
  peerName as peerNamePure,
} from "./store";

/** localStorage key for the persisted chat history. */
export const CHAT_STORAGE_KEY = "liqmesh-chat";

export type ChatStore = ChatState & {
  /** Currently-selected room in the UI (persisted; defaults to general). */
  activeRoomId: string;
  /** Fold an inbound wire frame into the store. */
  applyFrame: (frame: Frame) => void;
  /** Optimistically add a locally-composed outgoing message. */
  addLocalMessage: (msg: Omit<Message, "deleted" | "reactions">) => void;
  /** Register a room (normalized, deduped). Does not switch to it. */
  addRoom: (roomId: string) => void;
  /** Switch the active room (normalizing empty -> general). */
  setActiveRoom: (roomId: string) => void;
  /** Read helper: messages for a room (sorted asc, empty if unknown). */
  messagesIn: (roomId: string) => Message[];
  /** Read helper: known rooms (always includes general). */
  roomList: () => string[];
  /** Read helper: latest display name for a peer, or undefined. */
  peerName: (senderId: string) => string | undefined;
  /** Wipe all persisted history (e.g. a "clear chat" affordance). */
  clear: () => void;
};

export const useChatStore = create<ChatStore>()(
  persist(
    (set, get) => ({
      ...initialState,
      activeRoomId: DEFAULT_ROOM_ID,

      applyFrame: (frame) =>
        set((state) => applyFramePure(toChatState(state), frame)),

      addLocalMessage: (msg) =>
        set((state) => addLocalMessagePure(toChatState(state), msg)),

      addRoom: (roomId) => set((state) => addRoomPure(toChatState(state), roomId)),

      setActiveRoom: (roomId) =>
        set({ activeRoomId: roomId && roomId.length > 0 ? roomId : DEFAULT_ROOM_ID }),

      messagesIn: (roomId) => messagesInPure(toChatState(get()), roomId),

      roomList: () => roomListPure(toChatState(get())),

      peerName: (senderId) => peerNamePure(toChatState(get()), senderId),

      clear: () => set({ ...initialState, activeRoomId: DEFAULT_ROOM_ID }),
    }),
    {
      name: CHAT_STORAGE_KEY,
      storage: createJSONStorage(() => localStorage),
      // Persist data only; actions are recreated by the initializer on load.
      partialize: (state) => ({
        messagesByRoom: state.messagesByRoom,
        reads: state.reads,
        rooms: state.rooms,
        peers: state.peers,
        activeRoomId: state.activeRoomId,
      }),
    },
  ),
);

/** Narrow the full store down to the pure `ChatState` the reducer expects. */
function toChatState(s: ChatStore): ChatState {
  return {
    messagesByRoom: s.messagesByRoom,
    reads: s.reads,
    rooms: s.rooms,
    peers: s.peers,
  };
}
