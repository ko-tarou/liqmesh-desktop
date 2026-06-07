import { useEffect, useState } from "react";
import { Users } from "lucide-react";
import type { NearbyPeer } from "../chat/nearbyPeers";

type Props = {
  /**
   * The most recently newly-discovered peer (with a monotonic `key`), or null.
   * A new `key` re-shows the banner and restarts the auto-dismiss timer, so the
   * same component instance handles a stream of distinct discoveries.
   */
  peer: (NearbyPeer & { key: number }) | null;
};

/** How long the banner stays up before auto-dismissing. */
const DISMISS_MS = 4000;

/**
 * Top in-app banner announcing a newly-discovered nearby BLE peer
 * ("近くに人がいます: {name}"). Auto-dismisses after a few seconds; can also be
 * dismissed by tapping it. Renders nothing when there is no active peer to show.
 *
 * Dedupe (one pop per newly-seen senderId) is handled upstream in `useBle`; this
 * component only owns the show/auto-hide lifecycle, keyed off `peer.key`.
 */
export function NearbyPeerBanner({ peer }: Props) {
  const [shown, setShown] = useState(false);

  // Re-show + restart the timer whenever a new discovery arrives (peer.key).
  useEffect(() => {
    if (!peer) return;
    setShown(true);
    const timer = setTimeout(() => setShown(false), DISMISS_MS);
    return () => clearTimeout(timer);
  }, [peer?.key, peer]);

  if (!peer || !shown) return null;

  return (
    <div
      className="nearby-banner"
      role="status"
      aria-live="polite"
      onClick={() => setShown(false)}
    >
      <span className="nearby-banner-icon" aria-hidden>
        <Users size={16} />
      </span>
      <span className="nearby-banner-text">
        近くに人がいます: <strong>{peer.senderName}</strong>
      </span>
    </div>
  );
}
