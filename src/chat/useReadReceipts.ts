/**
 * Drives outgoing `read` high-water-marks for the active room.
 *
 * Policy (deliberately conservative to keep the low-bandwidth BLE link quiet):
 *  - Send a `read` only for the *newest* message in the active room.
 *  - Send only when that high-water-mark ADVANCES (the newest id changed and
 *    differs from the last id we sent). Re-renders / unchanged rooms send nothing.
 *  - Only while `connected`, and only while the window is actually VISIBLE
 *    (document.visibilityState === "visible"), so "seen" stays honest when the
 *    app is open but backgrounded. Becoming visible re-evaluates immediately.
 *  - Debounced, so a burst of inbound messages collapses into one send.
 *
 * The reducer applies `read` last-write-wins, so a duplicate is harmless; this
 * hook just minimises wire traffic. It owns no UI — App passes in the current
 * room / newest id / identity and the `sendRead` action.
 */

import { useEffect, useRef } from "react";

/** How long to wait after the high-water-mark advances before sending. */
const DEBOUNCE_MS = 400;

type Params = {
  roomId: string;
  /** Newest message id in `roomId`, or undefined when the room is empty. */
  newestMessageId: string | undefined;
  myId: string;
  connected: boolean;
  sendRead: (roomId: string, upToMessageId: string, myId: string) => void;
};

export function useReadReceipts({
  roomId,
  newestMessageId,
  myId,
  connected,
  sendRead,
}: Params): void {
  // The last (roomId, messageId) we actually sent a read for — guards against
  // re-sending an unchanged mark across re-renders, room switches and refocus.
  const lastSent = useRef<{ roomId: string; messageId: string } | null>(null);

  useEffect(() => {
    if (!connected || !newestMessageId) return;

    function flushIfVisibleAndAdvanced() {
      if (document.visibilityState !== "visible") return;
      if (!newestMessageId) return;
      const prev = lastSent.current;
      if (prev && prev.roomId === roomId && prev.messageId === newestMessageId) {
        return; // already acknowledged this exact high-water-mark
      }
      lastSent.current = { roomId, messageId: newestMessageId };
      sendRead(roomId, newestMessageId, myId);
    }

    const timer = setTimeout(flushIfVisibleAndAdvanced, DEBOUNCE_MS);
    // If the tab was hidden when the message arrived, send as soon as it's shown.
    document.addEventListener("visibilitychange", flushIfVisibleAndAdvanced);

    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", flushIfVisibleAndAdvanced);
    };
  }, [roomId, newestMessageId, myId, connected, sendRead]);
}
