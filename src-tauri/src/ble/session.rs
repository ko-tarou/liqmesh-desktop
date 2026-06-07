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
//!
//! Security — trust-on-first-use (Contract: "senderId is bound to the
//! connection = anti-impersonation"): on receive, the first known,
//! protocol-compatible frame's `senderId` is bound to the connection
//! ([`Session::peer_id`]). Every later frame must carry that same `senderId`; a
//! mismatching one is dropped as impersonation and counted in
//! [`Session::impersonation_rejections`]. The protocol-version check ([`PROTO_VER`])
//! runs *before* binding, so an incompatible peer is never trusted.

use super::chunk::{payload_limit, ChunkError, Reassembler};
use super::frame::Frame;

/// MTU requested at connect time (Contract: "request MTU 247, fall back to 23").
/// Used to seed the default outgoing payload limit before the real negotiated
/// MTU is known.
pub const DEFAULT_MTU: usize = 247;

/// Protocol version this session speaks (Contract v1). A `hello` advertising a
/// different `protoVer` is rejected as incompatible: v1 accepts only v1, and any
/// future version negotiation is handled out of band, not by silently interop'ing
/// across versions.
pub const PROTO_VER: u32 = 1;

/// Reserved sender id for AI-authored messages (mirrors iOS `aiSenderID` /
/// Android `AI_SENDER_ID` = `"ai"`). A relayed `msg` legitimately carries the
/// ORIGINAL author's senderId (not the connected peer's), so multi-hop requires
/// relaxing the per-peer TOFU check for `msg` frames — but a peer must never be
/// allowed to impersonate the AI, so a `msg` claiming this id is still dropped.
pub const AI_SENDER_ID: &str = "ai";

/// How long (ms) a partial reassembly may sit idle before
/// [`Session::evict_expired`] reaps it. Bounds memory against a peer that starts
/// a chunked message and never finishes it. PR-B2b feeds a monotonic clock.
pub const REASSEMBLY_TTL_MS: u64 = 30_000;

/// Errors produced while encoding an outgoing frame.
///
/// Wraps the chunk-layer [`ChunkError`] (via [`From`], so the chunking `?`
/// propagates transparently) and adds [`SessionError::EncodeUnknown`] for the
/// caller-bug case of trying to serialize a [`Frame::Unknown`], which has no
/// wire representation.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SessionError {
    /// A failure in the chunking layer (e.g. payload over the 255-chunk cap).
    Chunk(ChunkError),
    /// Attempted to encode [`Frame::Unknown`], which has no wire form.
    EncodeUnknown,
}

impl From<ChunkError> for SessionError {
    fn from(e: ChunkError) -> Self {
        SessionError::Chunk(e)
    }
}

/// Returns the `senderId` a frame claims, if any.
///
/// hello / msg / reaction / delete / read all carry a `senderId`; `Unknown` has
/// no identity. Used by the TOFU check to bind / verify the connection's peer.
fn frame_sender_id(frame: &Frame) -> Option<&str> {
    match frame {
        Frame::Hello { sender_id, .. }
        | Frame::Msg { sender_id, .. }
        | Frame::Reaction { sender_id, .. }
        | Frame::Delete { sender_id, .. }
        | Frame::Read { sender_id, .. } => Some(sender_id),
        Frame::Unknown => None,
    }
}

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
    /// Count of `hello` frames rejected because their `protoVer` did not match
    /// [`PROTO_VER`]. Such a peer speaks an incompatible protocol; the frame is
    /// dropped (`Ok(None)`) and no TOFU binding occurs. PR-B2 surfaces this.
    incompatible_proto: u64,
    /// Trust-on-first-use peer identity (Contract: "senderId is bound to the
    /// connection — anti-impersonation"). `None` until the first known,
    /// protocol-compatible frame carrying a `senderId` arrives; that `senderId`
    /// is then bound here for the connection's lifetime. Any later frame whose
    /// `senderId` differs is rejected as impersonation.
    peer_id: Option<String>,
    /// Count of frames dropped because their `senderId` did not match the bound
    /// [`Session::peer_id`] (impersonation attempts). PR-B2 surfaces this.
    impersonation_rejections: u64,
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
            incompatible_proto: 0,
            peer_id: None,
            impersonation_rejections: 0,
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

    /// Number of `hello` frames rejected as **protocol-incompatible** (their
    /// `protoVer` did not equal [`PROTO_VER`]) over this session's lifetime.
    /// Such frames are dropped without TOFU binding; PR-B2 surfaces this counter.
    pub fn incompatible_proto(&self) -> u64 {
        self.incompatible_proto
    }

    /// The trust-on-first-use peer identity bound to this connection, or `None`
    /// before the first known, compatible frame carrying a `senderId` arrives.
    /// Once bound it never changes; mismatching senders are rejected.
    pub fn peer_id(&self) -> Option<&str> {
        self.peer_id.as_deref()
    }

    /// Number of frames rejected because their `senderId` did not match the
    /// bound [`Session::peer_id`] (impersonation attempts) over this session's
    /// lifetime. PR-B2 surfaces this counter.
    pub fn impersonation_rejections(&self) -> u64 {
        self.impersonation_rejections
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
            proto_ver: PROTO_VER,
        }
    }

    /// Encodes `frame` into the ordered packet list to write to TX, stamping it
    /// with a fresh `msgId` and advancing the counter (`wrapping_add(1)`).
    ///
    /// Performs **no I/O** — the caller (PR-B2) writes each returned packet to
    /// the TX characteristic in order.
    ///
    /// Errors with [`SessionError::EncodeUnknown`] if `frame` is
    /// [`Frame::Unknown`] (it has no wire representation, so encoding it is a
    /// caller bug), and propagates chunk-layer failures as
    /// [`SessionError::Chunk`] (e.g. a payload that needs more than 255 chunks
    /// at the current `max_payload`).
    pub fn encode_frame(&mut self, frame: &Frame) -> Result<Vec<Vec<u8>>, SessionError> {
        if matches!(frame, Frame::Unknown) {
            return Err(SessionError::EncodeUnknown);
        }
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
    /// - `Ok(None)` — still awaiting chunks, OR the completed payload was
    ///   dropped for one of: an `Unknown`/future `type` (forward-compat, normal);
    ///   a malformed known frame (decode → `None`, increments
    ///   [`Session::protocol_violations`]); a `hello` with an incompatible
    ///   `protoVer` (increments [`Session::incompatible_proto`], no binding); or
    ///   a frame whose `senderId` does not match the trust-on-first-use binding
    ///   (increments [`Session::impersonation_rejections`]). PR-B2 must surface
    ///   these counters via log/metrics so the otherwise-invisible loss is
    ///   observable.
    /// - `Err(ChunkError)` — a malformed packet at the chunk layer (too short,
    ///   bad `total`, etc.); surfaced so the transport can react.
    ///
    /// `now_ms` is a caller-supplied monotonic timestamp recorded on the
    /// reassembly so [`Session::evict_expired`] can later reap stale partials
    /// (sans-IO: the session never reads a clock itself).
    pub fn on_packet(&mut self, packet: &[u8], now_ms: u64) -> Result<Option<Frame>, ChunkError> {
        let Some(payload) = self.reassembler.push(packet, now_ms)? else {
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
            // A hello advertising a different protocol version is incompatible.
            // Checked before any TOFU binding so an incompatible peer never gets
            // bound. v1 accepts only v1; future negotiation is out of band.
            Some(Frame::Hello { proto_ver, .. }) if proto_ver != PROTO_VER => {
                self.incompatible_proto += 1;
                Ok(None)
            }
            // A `msg` is relay-exempt from the per-peer TOFU check: in a multi-hop
            // mesh it legitimately carries the ORIGINAL author's senderId, not the
            // connected peer's, so binding/rejecting by peer id would kill relayed
            // hops. We still drop a `msg` that claims the reserved AI id (a peer
            // must not impersonate the AI), and we still record the peer binding
            // from a `msg` when nothing is bound yet (best-effort presence), but we
            // never REJECT a `msg` for a sender mismatch.
            Some(frame @ Frame::Msg { .. }) => {
                if frame_sender_id(&frame) == Some(AI_SENDER_ID) {
                    self.impersonation_rejections += 1;
                    return Ok(None);
                }
                Ok(Some(frame.normalized()))
            }
            // Other known, compatible frames (hello / reaction / delete / read):
            // enforce trust-on-first-use binding of the peer's senderId before
            // delivering (anti-impersonation, tied to the directly-connected peer).
            Some(frame) => {
                match (frame_sender_id(&frame), self.peer_id.as_deref()) {
                    // First identified frame: bind the connection to this sender.
                    (Some(sender), None) => self.peer_id = Some(sender.to_string()),
                    // Subsequent frame whose sender does not match the binding:
                    // reject as impersonation and keep the original binding.
                    (Some(sender), Some(bound)) if sender != bound => {
                        self.impersonation_rejections += 1;
                        return Ok(None);
                    }
                    // Matching sender (or a sender-less frame, which cannot
                    // exist for a known frame today): deliver normally.
                    _ => {}
                }
                Ok(Some(frame.normalized()))
            }
        }
    }

    /// Reaps partial reassemblies idle longer than [`REASSEMBLY_TTL_MS`],
    /// returning the number dropped. `now_ms` is the caller's monotonic clock
    /// (PR-B2b drives this periodically); the session holds no clock of its own.
    pub fn evict_expired(&mut self, now_ms: u64) -> usize {
        self.reassembler.evict_expired(now_ms, REASSEMBLY_TTL_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, name: &str) -> Session {
        Session::new(id.to_string(), name.to_string())
    }

    /// Pushes every packet into `rx` and returns the first completed frame.
    /// Time-agnostic tests pass a fixed `now_ms` of 0.
    fn feed(rx: &mut Session, packets: &[Vec<u8>]) -> Option<Frame> {
        let mut out = None;
        for p in packets {
            if let Some(f) = rx.on_packet(p, 0).expect("on_packet ok") {
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
            created_at: 1_749_168_000_000,
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
                if let Some(f) = rx.on_packet(p, 0).expect("rx a") {
                    got_a = Some(f);
                }
            }
            if let Some(p) = pb.get(i) {
                if let Some(f) = rx.on_packet(p, 0).expect("rx b") {
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
        // the ChunkError surfaces to the caller rather than being swallowed. It
        // is wrapped in SessionError::Chunk via the `?`/`From` path.
        let mut tx = session("u1", "Alice");
        tx.set_max_payload(1);
        assert_eq!(tx.max_payload(), 1, "a positive limit must be applied");
        let msg = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "Alice".into(),
            body: "x".repeat(1000),
            created_at: 1_749_168_000_000,
            room_id: "lobby".into(),
            reply_to_id: None,
        };
        assert_eq!(
            tx.encode_frame(&msg),
            Err(SessionError::Chunk(ChunkError::TooManyChunks))
        );
    }

    #[test]
    fn encode_frame_rejects_unknown() {
        // Frame::Unknown has no wire representation; encoding it is a caller bug
        // and must be rejected rather than emitting an empty payload.
        let mut tx = session("u1", "Alice");
        assert_eq!(
            tx.encode_frame(&Frame::Unknown),
            Err(SessionError::EncodeUnknown)
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
        assert_eq!(rx.on_packet(&[0, 0, 0], 0), Err(ChunkError::PacketTooShort));
    }

    /// Encodes a frame from a *given* sender id (not necessarily the session's
    /// own), so the rx side can be fed traffic that claims an arbitrary senderId.
    fn msg_from(sender_id: &str, body: &str) -> Frame {
        Frame::Msg {
            id: "m".into(),
            sender_id: sender_id.into(),
            sender_name: "X".into(),
            body: body.into(),
            created_at: 1,
            room_id: "lobby".into(),
            reply_to_id: None,
        }
    }

    #[test]
    fn first_hello_binds_peer_id() {
        let mut rx = session("me", "Me");
        assert_eq!(rx.peer_id(), None, "unbound before any frame");
        let hello = Frame::Hello {
            sender_id: "u1".into(),
            sender_name: "U1".into(),
            proto_ver: PROTO_VER,
        };
        let mut tx = session("u1", "U1");
        let packets = tx.encode_frame(&hello).expect("encode");
        assert_eq!(feed(&mut rx, &packets), Some(hello));
        assert_eq!(rx.peer_id(), Some("u1"));
        assert_eq!(rx.impersonation_rejections(), 0);
    }

    #[test]
    fn matching_sender_passes_after_binding() {
        let mut rx = session("me", "Me");
        let mut tx = session("u1", "U1");
        // Bind via hello, then a msg from the same sender must pass through.
        let hp = tx.encode_frame(&tx.hello_frame()).expect("hello");
        assert!(feed(&mut rx, &hp).is_some());

        let msg = msg_from("u1", "hi");
        let mp = tx.encode_frame(&msg).expect("msg");
        assert_eq!(feed(&mut rx, &mp), Some(msg));
        assert_eq!(rx.peer_id(), Some("u1"));
        assert_eq!(rx.impersonation_rejections(), 0);
    }

    #[test]
    fn mismatched_sender_reaction_is_rejected_as_impersonation() {
        let mut rx = session("me", "Me");
        // First bind to u1 via hello.
        let mut tx1 = session("u1", "U1");
        let hp = tx1.encode_frame(&tx1.hello_frame()).expect("hello");
        assert!(feed(&mut rx, &hp).is_some());
        assert_eq!(rx.peer_id(), Some("u1"));

        // A NON-msg frame (reaction) claiming u2 over the u1 connection is still
        // an impostor — TOFU stays strict for reaction/delete/read.
        let mut tx2 = session("u2", "U2");
        let imposter = Frame::Reaction {
            message_id: "m1".into(),
            sender_id: "u2".into(),
            emoji: "👍".into(),
            op: "add".into(),
        };
        let ip = tx2.encode_frame(&imposter).expect("reaction");
        assert_eq!(feed(&mut rx, &ip), None, "reaction impersonation dropped");
        assert_eq!(rx.impersonation_rejections(), 1);
        assert_eq!(rx.peer_id(), Some("u1"), "binding stays on u1");
    }

    #[test]
    fn mismatched_sender_msg_is_accepted_for_relay() {
        // Multi-hop: a relayed `msg` carries the ORIGINAL author's senderId, not
        // the connected peer's. After binding to u1 via hello, a msg claiming u2
        // must be DELIVERED (not rejected) so relayed hops survive.
        let mut rx = session("me", "Me");
        let mut tx1 = session("u1", "U1");
        let hp = tx1.encode_frame(&tx1.hello_frame()).expect("hello");
        assert!(feed(&mut rx, &hp).is_some());

        let mut tx2 = session("u2", "U2");
        let relayed = msg_from("u2", "hello from afar");
        let mp = tx2.encode_frame(&relayed).expect("msg");
        assert_eq!(feed(&mut rx, &mp), Some(relayed), "relayed msg accepted");
        assert_eq!(rx.impersonation_rejections(), 0);
        assert_eq!(rx.peer_id(), Some("u1"), "binding stays on the real peer u1");
    }

    #[test]
    fn msg_claiming_reserved_ai_sender_is_dropped() {
        // A peer must never impersonate the AI, even via the relay-relaxed path.
        let mut rx = session("me", "Me");
        let mut tx = session("u1", "U1");
        let fake_ai = msg_from(AI_SENDER_ID, "I am the AI");
        let mp = tx.encode_frame(&fake_ai).expect("msg");
        assert_eq!(feed(&mut rx, &mp), None, "fake-AI msg dropped");
        assert_eq!(rx.impersonation_rejections(), 1);
    }

    #[test]
    fn first_msg_does_not_bind_peer_id() {
        // A `msg` no longer establishes the TOFU binding (it may be a relayed
        // frame from a far author). Only hello binds the connection identity.
        let mut rx = session("me", "Me");
        let msg = msg_from("u1", "hi");
        let mut tx = session("u1", "U1");
        let mp = tx.encode_frame(&msg).expect("msg");
        assert_eq!(feed(&mut rx, &mp), Some(msg), "msg still delivered");
        assert_eq!(rx.peer_id(), None, "msg does not bind peer_id");
    }

    #[test]
    fn incompatible_hello_does_not_bind_then_valid_sender_binds() {
        // proto check precedes TOFU: an incompatible hello must NOT bind, so a
        // subsequent compatible HELLO from a *different* sender binds cleanly.
        // (Binding is established by hello; msg is relay-exempt and never binds.)
        let mut rx = session("me", "Me");
        let bad = hello_packets("u9", 2);
        assert_eq!(feed(&mut rx, &bad), None);
        assert_eq!(rx.peer_id(), None, "incompatible hello must not bind");
        assert_eq!(rx.incompatible_proto(), 1);

        let mut tx = session("u1", "U1");
        let hp = tx.encode_frame(&tx.hello_frame()).expect("hello");
        assert!(feed(&mut rx, &hp).is_some());
        assert_eq!(rx.peer_id(), Some("u1"));
    }

    /// Builds a hello frame's packets with an explicit protoVer (the public API
    /// always stamps protoVer:1, so we hand-craft the JSON to test other values).
    fn hello_packets(sender_id: &str, proto_ver: u32) -> Vec<Vec<u8>> {
        let json = format!(
            r#"{{"type":"hello","senderId":"{sender_id}","senderName":"X","protoVer":{proto_ver}}}"#
        );
        super::super::chunk::split(1, json.as_bytes(), payload_limit(DEFAULT_MTU))
            .expect("split")
    }

    #[test]
    fn incompatible_proto_ver_hello_is_rejected_and_counted() {
        let mut rx = session("u2", "Bob");
        let packets = hello_packets("u1", 2); // protoVer 2 != PROTO_VER (1)
        assert_eq!(feed(&mut rx, &packets), None, "incompatible hello dropped");
        assert_eq!(rx.incompatible_proto(), 1);
    }

    #[test]
    fn compatible_proto_ver_hello_is_accepted() {
        let mut rx = session("u2", "Bob");
        let packets = hello_packets("u1", PROTO_VER);
        match feed(&mut rx, &packets) {
            Some(Frame::Hello { proto_ver, .. }) => assert_eq!(proto_ver, PROTO_VER),
            other => panic!("expected Hello, got {other:?}"),
        }
        assert_eq!(rx.incompatible_proto(), 0);
    }

    #[test]
    fn evict_expired_drops_stale_partial_after_ttl() {
        // A 2-chunk message where only chunk 0 arrives stays partial; past the
        // session TTL it is evicted and the message can no longer complete.
        let mut tx = session("u1", "Alice");
        let mut rx = session("u2", "Bob");
        tx.set_max_payload(1); // force many small chunks
        let msg = Frame::Msg {
            id: "m1".into(),
            sender_id: "u1".into(),
            sender_name: "Alice".into(),
            body: "hello world".into(),
            created_at: 1,
            room_id: "lobby".into(),
            reply_to_id: None,
        };
        let packets = tx.encode_frame(&msg).expect("encode");
        assert!(packets.len() > 1);

        // Receive only the first chunk at t=0.
        assert_eq!(rx.on_packet(&packets[0], 0).expect("c0"), None);

        // Past REASSEMBLY_TTL_MS the partial is reaped.
        assert_eq!(rx.evict_expired(REASSEMBLY_TTL_MS + 10_000), 1);

        // The remaining chunks can no longer reconstruct the original message.
        let mut completed = None;
        for p in &packets[1..] {
            if let Some(f) = rx.on_packet(p, REASSEMBLY_TTL_MS + 11_000).expect("late") {
                completed = Some(f);
            }
        }
        assert_eq!(completed, None, "evicted partial must not complete");
    }

    #[test]
    fn evict_expired_keeps_partial_within_ttl() {
        let mut tx = session("u1", "Alice");
        let mut rx = session("u2", "Bob");
        tx.set_max_payload(1);
        let msg = Frame::Reaction {
            message_id: "m1".into(),
            sender_id: "u1".into(),
            emoji: "👍".into(),
            op: "add".into(),
        };
        let packets = tx.encode_frame(&msg).expect("encode");
        assert!(packets.len() > 1);

        // Push every chunk but the last at t=0.
        for p in &packets[..packets.len() - 1] {
            assert_eq!(rx.on_packet(p, 0).expect("c"), None);
        }
        // Within the TTL nothing is evicted, so the final chunk completes it.
        assert_eq!(rx.evict_expired(10_000), 0);
        let last = rx
            .on_packet(&packets[packets.len() - 1], 10_000)
            .expect("last");
        assert_eq!(last, Some(msg));
    }
}
