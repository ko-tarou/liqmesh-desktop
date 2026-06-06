# LiqMesh BLE Interop Contract v1

Canonical — do not diverge. Changes go through the architect session.

## Goal
When the online path (Supabase) is unavailable, Android / iOS / Windows-Desktop talk directly over BLE (P2P).

## Roles
- Every client is dual-role (advertise/peripheral + scan/connect/central) where the OS allows.
- iOS / Android = full equal peers (both roles).
- Windows Desktop = central only (cannot reliably advertise — OS limitation); it connects to advertising phones.
- After connect, the GATT connection is held as a persistent, full-duplex session (WebSocket-like). Disconnect on out-of-range is acceptable.
- P2P pairs all supported: iOS<->Android, Android<->Desktop, Desktop<->iOS (Desktop is always the central).
- v1 = direct links only. Multi-hop relay = v2.

## GATT (fixed across all platforms)
- Service UUID:  `B1E5C0DE-1A2B-4C3D-8E9F-000000000001`
- TX  (Write / WriteWithoutResponse, central -> peripheral):  `B1E5C0DE-1A2B-4C3D-8E9F-000000000002`
- RX  (Notify, peripheral -> central):                        `B1E5C0DE-1A2B-4C3D-8E9F-000000000003`
- Advertise localName: `"LQM-" + first 4 chars of deviceId`
- Central: scan for the Service UUID -> connect -> subscribe RX -> write to TX.

## Connection / MTU / chunking
- Request MTU 247 on connect (fall back to 23 if refused).
- Split each logical message (UTF-8 JSON) into chunks. Each packet:
  `[msgId: 4 bytes big-endian][seq: 1 byte][total: 1 byte][payload...]`
  payload max = negotiatedMTU - 3 (ATT) - 6 (header). Reassemble by msgId using seq/total. total=1 means no split.

## Payload (identical JSON to the app wire; unknown `type` ignored = forward-compatible)
- `hello`    `{ "type":"hello", "senderId":"", "senderName":"", "protoVer":1 }`  (required both directions right after connect)
- `msg`      `{ "type":"msg", "id":"", "senderId":"", "senderName":"", "body":"", "createdAt":0, "roomId":"", "replyToId":null }`
- `reaction` `{ "type":"reaction", "messageId":"", "senderId":"", "emoji":"", "op":"add" }`
- `delete`   `{ "type":"delete", "messageId":"", "senderId":"" }`
- `read`     `{ "type":"read", "roomId":"", "upToMessageId":"", "senderId":"" }`
- (optional) `backfill`: send recent N messages after hello.

## Integrity / security
- senderId is bound per-connection (trust-on-first-use) to prevent spoofing.
- Encryption / pairing beyond standard BLE is out of scope for v1.

## Interop acceptance test
- Two devices pair: exchange hello -> bidirectional msg -> reaction/delete/read round-trip -> a long message that must be chunked arrives with no loss.
