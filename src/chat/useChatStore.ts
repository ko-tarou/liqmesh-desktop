/**
 * React-facing chat store, built on top of the pure reducer in `./store`.
 *
 * Zustand holds the immutable `ChatState` and exposes thin actions that delegate
 * to the pure functions (`applyFrame` / `addLocalMessage`). The `persist`
 * middleware mirrors only the serialisable data (`messagesByRoom` / `reads`) to
 * `localStorage`, so history survives reloads while actions are reconstructed on
 * boot. Keeping all the logic in the pure reducer means the 25+ existing unit
 * tests still cover the behaviour; this layer is just IO + React glue.
 */

import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";
import type { Frame } from "./frames";
import {
  type ChatState,
  type Message,
  initialState,
  applyFrame as applyFramePure,
  addLocalMessage as addLocalMessagePure,
  messagesIn as messagesInPure,
} from "./store";

/** localStorage key for the persisted chat history. */
export const CHAT_STORAGE_KEY = "liqmesh-chat";

export type ChatStore = ChatState & {
  /** Fold an inbound wire frame into the store. */
  applyFrame: (frame: Frame) => void;
  /** Optimistically add a locally-composed outgoing message. */
  addLocalMessage: (msg: Omit<Message, "deleted" | "reactions">) => void;
  /** Read helper: messages for a room (sorted asc, empty if unknown). */
  messagesIn: (roomId: string) => Message[];
  /** Wipe all persisted history (e.g. a "clear chat" affordance). */
  clear: () => void;
};

export const useChatStore = create<ChatStore>()(
  persist(
    (set, get) => ({
      ...initialState,

      applyFrame: (frame) =>
        set((state) => applyFramePure(toChatState(state), frame)),

      addLocalMessage: (msg) =>
        set((state) => addLocalMessagePure(toChatState(state), msg)),

      messagesIn: (roomId) => messagesInPure(toChatState(get()), roomId),

      clear: () => set({ ...initialState }),
    }),
    {
      name: CHAT_STORAGE_KEY,
      storage: createJSONStorage(() => localStorage),
      // Persist data only; actions are recreated by the initializer on load.
      partialize: (state): ChatState => ({
        messagesByRoom: state.messagesByRoom,
        reads: state.reads,
      }),
    },
  ),
);

/** Narrow the full store down to the pure `ChatState` the reducer expects. */
function toChatState(s: ChatStore): ChatState {
  return { messagesByRoom: s.messagesByRoom, reads: s.reads };
}
