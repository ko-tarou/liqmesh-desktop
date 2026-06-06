import { describe, it, expect } from "vitest";
import type { HelloFrame } from "./frames";
import { nearbyPeerToAnnounce } from "./nearbyPeers";

function hello(over: Partial<HelloFrame> = {}): HelloFrame {
  return { type: "hello", senderId: "alice", senderName: "Alice", protoVer: 1, ...over };
}

describe("nearbyPeerToAnnounce", () => {
  it("announces a peer the first time it is seen", () => {
    expect(nearbyPeerToAnnounce(new Set(), hello())).toEqual({
      senderId: "alice",
      senderName: "Alice",
    });
  });

  it("does not re-announce a peer already in the seen set", () => {
    expect(nearbyPeerToAnnounce(new Set(["alice"]), hello())).toBeNull();
  });

  it("announces a different peer even when others are already seen", () => {
    const peer = nearbyPeerToAnnounce(new Set(["alice"]), hello({ senderId: "bob", senderName: "Bob" }));
    expect(peer).toEqual({ senderId: "bob", senderName: "Bob" });
  });

  it("uses the name carried by the hello (latest-wins is the caller's concern)", () => {
    const peer = nearbyPeerToAnnounce(new Set(), hello({ senderName: "Alice Renamed" }));
    expect(peer?.senderName).toBe("Alice Renamed");
  });

  it("never announces a blank senderId", () => {
    expect(nearbyPeerToAnnounce(new Set(), hello({ senderId: "" }))).toBeNull();
  });
});
