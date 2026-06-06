/**
 * Newly-discovered-peer detection for the "近くに人がいます" banner (friend
 * system, Phase 1 — UI only).
 *
 * A `hello` frame is a peer's presence beacon (it carries `senderId` +
 * `senderName`). The first time we ever see a given `senderId` we want to pop a
 * banner once; subsequent `hello`s from the same peer (re-advertised every link)
 * must NOT re-pop. This is pure dedup logic over a set of already-seen ids so it
 * is trivially unit-testable and has no React / wire dependencies.
 */

import type { HelloFrame } from "./frames";

/** A peer we want to announce in the nearby-peer banner. */
export type NearbyPeer = {
  senderId: string;
  senderName: string;
};

/**
 * Decide whether a `hello` is a NEWLY-seen peer worth announcing.
 *
 * Returns the `NearbyPeer` to announce when `frame.senderId` is absent from
 * `seen`, otherwise `null` (already announced -> no re-pop). Callers own the
 * `seen` set and should add the returned peer's id to it so the dedupe holds.
 * A blank `senderId` is never announced (defensive: an unidentifiable peer).
 */
export function nearbyPeerToAnnounce(
  seen: ReadonlySet<string>,
  frame: HelloFrame,
): NearbyPeer | null {
  const senderId = frame.senderId;
  if (!senderId || seen.has(senderId)) return null;
  return { senderId, senderName: frame.senderName };
}
