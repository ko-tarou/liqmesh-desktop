/**
 * Bridges the Rust BLE transport (`ble://…` Tauri events + `ble_*` commands) to
 * the React chat store.
 *
 * Inbound `ble://frame` payloads are `Frame` JSON (camelCase) and are folded
 * straight into `useChatStore`. Connection lifecycle events drive a small status
 * machine; the latest `stats` snapshot and last error are surfaced for the UI.
 * Outgoing messages are added optimistically to the store *before* the wire send
 * so the composer feels instant; the later echo over `ble://frame` dedups by id.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Frame } from "./frames";
import { nearbyPeerToAnnounce, type NearbyPeer } from "./nearbyPeers";
import { useChatStore } from "./useChatStore";

export type BleStatus = "offline" | "connecting" | "connected";

/** Mirrors the Rust `StatsPayload` (camelCase) emitted on `ble://stats`. */
export type BleStats = {
  protocolViolations: number;
  impersonationRejections: number;
  incompatibleProto: number;
};

/** Mirrors the Rust `LinkErrorPayload` emitted on `ble://error`. */
export type BleErrorPayload = {
  kind: "io" | "disconnected";
  message?: string;
};

export type UseBle = {
  status: BleStatus;
  stats: BleStats | null;
  /** Last error string surfaced to the UI (connect failure or link error). */
  error: string | null;
  /**
   * senderId of the most recent `hello` peer (single-link assumption). The
   * display name is resolved from the store's `peers` map by the UI. Cleared
   * on disconnect.
   */
  peerId: string | null;
  /**
   * The most recently NEWLY-discovered nearby peer, for the "近くに人がいます"
   * banner. Re-pops only for a peer we have not announced before (deduped by
   * senderId for the hook's lifetime). `key` is a monotonic id so the banner
   * component can treat each announcement as a distinct event even if the same
   * name reappears. `null` until the first new peer is seen.
   */
  nearbyPeer: (NearbyPeer & { key: number }) | null;
  connect: (myId: string, myName: string) => Promise<void>;
  disconnect: () => Promise<void>;
  /** Optimistically store + send a chat message to the given room. */
  sendMessage: (body: string, roomId: string, myId: string, myName: string) => Promise<void>;
  /**
   * Optimistically apply + send a reaction toggle. `op` is "add" / "remove";
   * the reducer is idempotent so a repeated op or the later wire echo is a no-op.
   */
  sendReaction: (
    messageId: string,
    emoji: string,
    op: "add" | "remove",
    myId: string,
  ) => Promise<void>;
  /**
   * Optimistically apply + send a delete for one of my own messages. The
   * reducer authorizes by sender (only the original author may delete) and is
   * idempotent, so the later wire echo is a no-op.
   */
  sendDelete: (messageId: string, myId: string) => Promise<void>;
  /**
   * Optimistically apply + send a read high-water-mark for a room. The reducer
   * is last-write-wins per (room, sender), so re-sending an older or equal mark
   * is harmless; callers should still send only when the mark advances to keep
   * the BLE link quiet.
   */
  sendRead: (roomId: string, upToMessageId: string, myId: string) => Promise<void>;
};

export function useBle(): UseBle {
  const [status, setStatus] = useState<BleStatus>("offline");
  const [stats, setStats] = useState<BleStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [peerId, setPeerId] = useState<string | null>(null);
  const [nearbyPeer, setNearbyPeer] = useState<(NearbyPeer & { key: number }) | null>(null);
  // Ids we have already announced (deduped, one banner per newly-seen peer).
  // A ref (not state) so updating it never triggers a re-render on its own.
  const announcedPeerIds = useRef<Set<string>>(new Set());
  const nearbyKey = useRef(0);

  const applyFrame = useChatStore((s) => s.applyFrame);
  const addLocalMessage = useChatStore((s) => s.addLocalMessage);

  // Subscribe to every ble:// event for the hook's lifetime.
  useEffect(() => {
    const subscriptions: Promise<UnlistenFn>[] = [
      listen("ble://connected", () => {
        setStatus("connected");
        setError(null);
      }),
      listen("ble://disconnected", () => {
        setStatus("offline");
        setPeerId(null);
      }),
      listen("ble://error", (event) => {
        setStatus("offline");
        setPeerId(null);
        const p = event.payload as BleErrorPayload | null;
        setError(p ? `${p.kind}${p.message ? `: ${p.message}` : ""}` : "link error");
      }),
      listen<Frame>("ble://frame", (event) => {
        const frame = event.payload;
        applyFrame(frame);
        // Track the connected peer (single-link). `hello` is the remote peer's
        // presence beacon, so it identifies who we're talking to. We don't use
        // `msg.senderId` here because that also echoes our own outgoing sends.
        if (frame.type === "hello") {
          setPeerId(frame.senderId);
          // Pop the nearby-peer banner once per newly-seen peer.
          const peer = nearbyPeerToAnnounce(announcedPeerIds.current, frame);
          if (peer) {
            announcedPeerIds.current.add(peer.senderId);
            setNearbyPeer({ ...peer, key: ++nearbyKey.current });
          }
        }
      }),
      listen<BleStats>("ble://stats", (event) => {
        setStats(event.payload);
      }),
    ];

    return () => {
      subscriptions.forEach((p) => p.then((un) => un()));
    };
  }, [applyFrame]);

  const connect = useCallback(async (myId: string, myName: string) => {
    setError(null);
    setStatus("connecting");
    try {
      await invoke("ble_start", { myId, myName });
    } catch (e) {
      setStatus("offline");
      setError(String(e));
    }
  }, []);

  const disconnect = useCallback(async () => {
    try {
      await invoke("ble_stop");
    } catch (e) {
      setError(String(e));
    }
    setStatus("offline");
    setPeerId(null);
  }, []);

  const sendMessage = useCallback(
    async (body: string, roomId: string, myId: string, myName: string) => {
      const id = crypto.randomUUID();
      const createdAt = new Date().toISOString();

      // Optimistic local insert (deduped against the later wire echo by id).
      addLocalMessage({ id, senderId: myId, senderName: myName, body, createdAt, roomId });

      const frame: Frame = {
        type: "msg",
        id,
        senderId: myId,
        senderName: myName,
        body,
        createdAt,
        roomId,
      };
      try {
        await invoke("ble_send", { frameJson: JSON.stringify(frame) });
      } catch (e) {
        setError(`send failed: ${String(e)}`);
      }
    },
    [addLocalMessage],
  );

  const sendReaction = useCallback(
    async (messageId: string, emoji: string, op: "add" | "remove", myId: string) => {
      const frame: Frame = { type: "reaction", messageId, senderId: myId, emoji, op };

      // Optimistic local apply (idempotent; the later wire echo is a no-op).
      applyFrame(frame);

      try {
        await invoke("ble_send", { frameJson: JSON.stringify(frame) });
      } catch (e) {
        setError(`reaction failed: ${String(e)}`);
      }
    },
    [applyFrame],
  );

  const sendDelete = useCallback(
    async (messageId: string, myId: string) => {
      const frame: Frame = { type: "delete", messageId, senderId: myId };

      // Optimistic local apply (reducer enforces own-sender-only + tombstone).
      applyFrame(frame);

      try {
        await invoke("ble_send", { frameJson: JSON.stringify(frame) });
      } catch (e) {
        setError(`delete failed: ${String(e)}`);
      }
    },
    [applyFrame],
  );

  const sendRead = useCallback(
    async (roomId: string, upToMessageId: string, myId: string) => {
      const frame: Frame = { type: "read", roomId, upToMessageId, senderId: myId };

      // Optimistic local apply (last-write-wins per room+sender).
      applyFrame(frame);

      try {
        await invoke("ble_send", { frameJson: JSON.stringify(frame) });
      } catch (e) {
        setError(`read failed: ${String(e)}`);
      }
    },
    [applyFrame],
  );

  return {
    status,
    stats,
    error,
    peerId,
    nearbyPeer,
    connect,
    disconnect,
    sendMessage,
    sendReaction,
    sendDelete,
    sendRead,
  };
}
