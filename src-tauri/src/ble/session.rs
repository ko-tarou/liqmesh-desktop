//! Sans-IO BLE session state machine.
//!
//! A [`Session`] is a pure, fully-synchronous state machine that owns the codec
//! pipeline (framing + chunking) for one BLE GATT connection. It performs **no
//! I/O**: it turns outgoing [`Frame`]s into the packet byte-vectors the caller
//! must write to the TX characteristic, and folds incoming RX-notification
//! packets back into complete [`Frame`]s. The transport layer (PR-B2) owns the
//! `btleplug` plumbing and drives this state machine.
//!
//! This separation keeps all wire/chunk/normalization logic exhaustively unit
//! testable without async, OS BLE, or a real peer — see the `tests` module.
//!
//! roomId default (Contract v1.2/v1.3) is applied on receive: decoded frames are
//! run through [`Frame::normalized`] so an empty `roomId` becomes `"general"`
//! (the serde `default` already covers a *missing* `roomId`).

use super::chunk::{payload_limit, ChunkError, Reassembler};
use super::frame::Frame;

/// MTU requested at connect time (Contract: "request MTU 247, fall back to 23").
/// Used to seed the default outgoing payload limit before the real negotiated
/// MTU is known.
pub const DEFAULT_MTU: usize = 247;

/// Sans-IO per-connection session state.
///
/// Outgoing frames are encoded + chunked here (the caller writes the resulting
/// packets to TX); incoming packets are reassembled + decoded + normalized here.
pub struct Session {
    reassembler: Reassembler,
    sender_id: String,
    sender_name: String,
    /// Maximum payload bytes per outgoing packet (MTU minus ATT + header
    /// overhead). Updated post-connect via [`Session::set_max_payload`].
    max_payload: usize,
    /// Monotonically-increasing id stamped on the next outgoing logical message.
    next_msg_id: u32,
    /// Count of completed payloads that decoded to a **malformed known frame**
    /// (`Frame::decode` → `None`). These are dropped silently here for protocol
    /// robustness, but PR-B2 must surface this counter via log/metrics so the
    /// otherwise-invisible loss is observable. Forward-compatible `Unknown`
    /// frames are *not* counted (they are a normal case, not a violation).
    protocol_violations: u64,
}

impl Session {
    /// Creates a session for the local peer. `max_payload` is seeded from the
    /// requested MTU ([`DEFAULT_MTU`] → 238 bytes) and corrected post-connect by
    /// [`Session::set_max_payload`] once the real MTU is negotiated.
    pub fn new(sender_id: String, sender_name: String) -> Self {
        Session {
            reassembler: Reassembler::new(),
            sender_id,
            sender_name,
            max_payload: payload_limit(DEFAULT_MTU),
            next_msg_id: 0,
            protocol_violations: 0,
        }
    }

    /// Number of completed payloads dropped as **malformed known frames**
    /// (`Frame::decode` → `None`) over this session's lifetime.
    ///
    /// Such frames are discarded by [`Session::on_packet`] while incrementing
    /// this counter; PR-B2 should surface it via log/metrics so the loss is
    /// observable. Forward-compatible `Unknown` frames do **not** count.
    pub fn protocol_violations(&self) -> u64 {
        self.protocol_violations
    }

    /// Updates the outgoing payload limit from the negotiated MTU's effective
    /// payload size (i.e. the value of [`payload_limit`] for the live MTU).
    ///
    /// A `0` (or otherwise degenerate) value is ignored: [`super::chunk::split`]
    /// requires at least one payload byte per packet to make progress, so we
    /// never let the limit drop to zero. The previous limit is retained instead.
    pub fn set_max_payload(&mut self, max_payload: usize) {
        if max_payload > 0 {
            self.max_payload = max_payload;
        }
    }

    /// The current outgoing per-packet payload limit, in bytes.
    pub fn max_payload(&self) -> usize {
        self.max_payload
    }

    /// Builds this peer's `hello` frame, sent in both directions right after
    /// connect (Contract). Encode it with [`Session::encode_frame`] to get the
    /// packets to write.
    pub fn hello_frame(&self) -> Frame {
        Frame::Hello {
            sender_id: self.sender_id.clone(),
            sender_name: self.sender_name.clone(),
            proto_ver: 1,
        }
    }

    /// Encodes `frame` into the ordered packet list to write to TX, stamping it
    /// with a fresh `msgId` and advancing the counter (`wrapping_add(1)`).
    ///
    /// Performs **no I/O** — the caller (PR-B2) writes each returned packet to
    /// the TX characteristic in order. Propagates [`ChunkError`] (e.g. a payload
    /// that needs more than 255 chunks at the current `max_payload`).
    pub fn encode_frame(&mut self, frame: &Frame) -> Result<Vec<Vec<u8>>, ChunkError> {
        let bytes = frame.encode();
        let msg_id = self.next_msg_id;
        let packets = super::chunk::split(msg_id, &bytes, self.max_payload)?;
        // Only advance once the split succeeds, so a failed encode does not burn
        // a msgId (keeps ids contiguous for easier debugging).
        self.next_msg_id = self.next_msg_id.wrapping_add(1);
        Ok(packets)
    }

    /// Feeds one received RX packet into reassembly.
    ///
    /// - `Ok(Some(frame))` — a logical message completed and decoded to a known,
    ///   normalized frame.
    /// - `Ok(None)` — still awaiting chunks, OR the completed payload was not a
    ///   usable frame: an `Unknown`/future `type` (forward-compat, normal) or a
    ///   malformed known frame (decode → `None`, a protocol violation). Both are
    ///   dropped silently here, but a malformed known frame additionally
    ///   increments [`Session::protocol_violations`] so the loss is observable;
    ///   PR-B2 must surface that counter via log/metrics.
    /// - `Err(ChunkError)` — a malformed packet at the chunk layer (too short,
    ///   bad `total`, etc.); surfaced so the transport can react.
    pub fn on_packet(&mut self, packet: &[u8]) -> Result<Option<Frame>, ChunkError> {
        let Some(payload) = self.reassembler.push(packet)? else {
            return Ok(None); // still reassembling
        };
        // Completed payload: decode + normalize. Unknown/malformed → ignore.
        match Frame::decode(&payload) {
            // Malformed known frame: drop it, but record the violation so B2 can
            // surface the otherwise-invisible loss.
            None => {
                self.protocol_violations += 1;
                Ok(None)
            }
            // Forward-compatible unknown/future type: normal, not a violation.
            Some(Frame::Unknown) => Ok(None),
            Some(frame) => Ok(Some(frame.normalized())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, name: &str) -> Session {
        Session::new(id.to_string(), name.to_string())
    }

    /// Pushes every packet into `rx` and returns the first completed frame.
    fn feed(rx: &mut Session, packets: &[Vec<u8>]) -> Option<Frame> {
        let mut out = None;
        for p in packets {
            if let Some(f) = rx.on_packet(p).expect("on_packet ok") {
                out = Some(f);
            }
        }
        out
    }

    #[test]
    fn hello_round_trips_across_sessions() {
        let mut tx = session("u1", "Alice");
        let mut rx = session("u2", "Bob");
        let hello = tx.hello_frame();
        let packets = tx.encode_frame(&hello).expect("encode");
        assert_eq!(feed(&mut rx, &packets), Some(hello));
    }

    #[test]
    fn long_msg_splits_and_reassembles() {
        let mut tx = session("u1", "Alice");
        let mut rx = session("u2", "Bob");
        let body = "x".repeat(2000); // well over one 238-byte packet
        let msg = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "Alice".into(),
            body,
            created_at: "2026-06-06T00:00:00Z".into(),
            room_id: "lobby".into(),
            reply_to_id: None,
        };
        let packets = tx.encode_frame(&msg).expect("encode");
        assert!(packets.len() > 1, "long msg must span multiple chunks");
        assert_eq!(feed(&mut rx, &packets), Some(msg));
    }

    #[test]
    fn consecutive_frames_get_distinct_msg_ids_and_both_restore() {
        let mut tx = session("u1", "Alice");
        let mut rx = session("u2", "Bob");
        let a = Frame::Delete {
            message_id: "m1".into(),
            sender_id: "u1".into(),
        };
        let b = Frame::Reaction {
            message_id: "m2".into(),
            sender_id: "u1".into(),
            emoji: "👍".into(),
            op: "add".into(),
        };
        let pa = tx.encode_frame(&a).expect("encode a");
        let pb = tx.encode_frame(&b).expect("encode b");

        // Distinct msgIds (bytes 0..4 big-endian) on the two single-packet sends.
        assert_ne!(&pa[0][0..4], &pb[0][0..4], "msgIds must differ");

        // Interleaved receive still reassembles both independently.
        let mut got_a = None;
        let mut got_b = None;
        for i in 0..pa.len().max(pb.len()) {
            if let Some(p) = pa.get(i) {
                if let Some(f) = rx.on_packet(p).expect("rx a") {
                    got_a = Some(f);
                }
            }
            if let Some(p) = pb.get(i) {
                if let Some(f) = rx.on_packet(p).expect("rx b") {
                    got_b = Some(f);
                }
            }
        }
        assert_eq!(got_a, Some(a));
        assert_eq!(got_b, Some(b));
    }

    #[test]
    fn unknown_type_packet_is_ignored() {
        // A future/unknown frame type, small enough to fit one packet.
        let json = br#"{"type":"typing","senderId":"u1","extra":42}"#;
        let packets = super::super::chunk::split(7, json, payload_limit(DEFAULT_MTU))
            .expect("split");
        let mut rx = session("u2", "Bob");
        assert_eq!(feed(&mut rx, &packets), None);
    }

    #[test]
    fn malformed_known_frame_is_ignored_but_counted() {
        // Known `type` but missing required fields → decode None → dropped, and
        // recorded as a protocol violation so B2 can surface the loss.
        let json = br#"{"type":"msg","id":"x"}"#;
        let packets = super::super::chunk::split(8, json, payload_limit(DEFAULT_MTU))
            .expect("split");
        let mut rx = session("u2", "Bob");
        assert_eq!(feed(&mut rx, &packets), None);
        assert_eq!(rx.protocol_violations(), 1);
    }

    #[test]
    fn unknown_type_packet_does_not_count_as_violation() {
        // A forward-compatible unknown `type` is a normal case, not a violation.
        let json = br#"{"type":"typing","senderId":"u1","extra":42}"#;
        let packets = super::super::chunk::split(11, json, payload_limit(DEFAULT_MTU))
            .expect("split");
        let mut rx = session("u2", "Bob");
        assert_eq!(feed(&mut rx, &packets), None);
        assert_eq!(rx.protocol_violations(), 0);
    }

    #[test]
    fn encode_frame_propagates_too_many_chunks() {
        // A 1-byte payload limit forces a large frame past the 255-chunk cap, so
        // the ChunkError surfaces to the caller rather than being swallowed.
        let mut tx = session("u1", "Alice");
        tx.set_max_payload(1);
        assert_eq!(tx.max_payload(), 1, "a positive limit must be applied");
        let msg = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "Alice".into(),
            body: "x".repeat(1000),
            created_at: "2026-06-06T00:00:00Z".into(),
            room_id: "lobby".into(),
            reply_to_id: None,
        };
        assert_eq!(
            tx.encode_frame(&msg),
            Err(ChunkError::TooManyChunks)
        );
    }

    #[test]
    fn msg_with_missing_room_id_restores_to_general() {
        let json = br#"{"type":"msg","id":"m1","senderId":"u1","senderName":"A","body":"b","createdAt":"t"}"#;
        let packets = super::super::chunk::split(9, json, payload_limit(DEFAULT_MTU))
            .expect("split");
        let mut rx = session("u2", "Bob");
        match feed(&mut rx, &packets) {
            Some(Frame::Msg { room_id, .. }) => assert_eq!(room_id, "general"),
            other => panic!("expected Msg, got {other:?}"),
        }
    }

    #[test]
    fn msg_with_empty_room_id_is_normalized_to_general() {
        let json = br#"{"type":"msg","id":"m1","senderId":"u1","senderName":"A","body":"b","createdAt":"t","roomId":""}"#;
        let packets = super::super::chunk::split(10, json, payload_limit(DEFAULT_MTU))
            .expect("split");
        let mut rx = session("u2", "Bob");
        match feed(&mut rx, &packets) {
            Some(Frame::Msg { room_id, .. }) => assert_eq!(room_id, "general"),
            other => panic!("expected Msg, got {other:?}"),
        }
    }

    #[test]
    fn reaction_delete_read_round_trip() {
        let frames = vec![
            Frame::Reaction {
                message_id: "m1".into(),
                sender_id: "u1".into(),
                emoji: "🎉".into(),
                op: "remove".into(),
            },
            Frame::Delete {
                message_id: "m1".into(),
                sender_id: "u1".into(),
            },
            Frame::Read {
                room_id: "lobby".into(),
                up_to_message_id: "m9".into(),
                sender_id: "u1".into(),
            },
        ];
        for f in frames {
            let mut tx = session("u1", "Alice");
            let mut rx = session("u2", "Bob");
            let packets = tx.encode_frame(&f).expect("encode");
            assert_eq!(feed(&mut rx, &packets), Some(f));
        }
    }

    #[test]
    fn set_max_payload_ignores_zero_and_applies_valid() {
        let mut s = session("u1", "Alice");
        let before = s.max_payload();
        s.set_max_payload(0);
        assert_eq!(s.max_payload(), before, "zero must be ignored");
        s.set_max_payload(14); // MTU 23 fallback
        assert_eq!(s.max_payload(), 14);
    }

    #[test]
    fn chunk_layer_error_propagates() {
        // Packet shorter than the 6-byte header → PacketTooShort surfaces.
        let mut rx = session("u2", "Bob");
        assert_eq!(rx.on_packet(&[0, 0, 0]), Err(ChunkError::PacketTooShort));
    }
}
