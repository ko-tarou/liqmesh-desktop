//! BLE message chunking / reassembly.
//!
//! Packet wire format (per `docs/BLE_CONTRACT.md`):
//! `[msgId: 4 bytes big-endian][seq: 1 byte][total: 1 byte][payload...]`
//!
//! - Header length = 6 bytes.
//! - `payload` upper bound = `negotiatedMTU - 3(ATT) - 6(header)`.
//! - **`seq` is 0-based: it ranges over `0..total`**, and `total` is the chunk
//!   count. `total == 1` means a single, unsplit packet.
//!
//! INTEROP OPEN QUESTION: Contract v1 does not state whether `seq`/`total` are
//! 0-based or 1-based. This implementation adopts **0-based `seq`**
//! (`0..total`). This must be confirmed with the architect session against the
//! existing iOS/Android wire implementation. See
//! `.claude/plans/001-2_ble-codec.md`.

// stub for red phase

/// Fixed header length in bytes: msgId(4) + seq(1) + total(1).
pub const HEADER_LEN: usize = 6;
/// ATT protocol overhead subtracted from the negotiated MTU.
pub const ATT_OVERHEAD: usize = 3;

/// Returns the maximum payload size that fits in a single packet for the given
/// negotiated MTU: `mtu - 3(ATT) - 6(header)`.
///
/// If the MTU is too small to fit even the ATT + header overhead, returns 0.
pub fn payload_limit(_mtu: usize) -> usize {
    0
}

/// Errors produced while reassembling chunked packets.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ChunkError {
    /// Packet shorter than the 6-byte header.
    PacketTooShort,
    /// `total` was 0 (illegal: every message has at least one chunk).
    InvalidTotal,
    /// `seq` was out of range for the declared `total` (must be `< total`).
    SeqOutOfRange,
    /// A later packet declared a different `total` than an earlier one for the
    /// same `msgId`.
    TotalMismatch,
    /// `total` exceeded 255 and cannot be encoded in a single byte.
    TooManyChunks,
}

/// Splits `payload` into header-prefixed packets for the given `msg_id`.
pub fn split(_msg_id: u32, _payload: &[u8], _max_payload: usize) -> Result<Vec<Vec<u8>>, ChunkError> {
    Ok(vec![])
}

/// Reassembles chunked packets into complete payloads, keyed by `msgId`.
#[derive(Default)]
pub struct Reassembler;

impl Reassembler {
    pub fn new() -> Self {
        Reassembler
    }

    /// Feeds one packet. Returns `Ok(Some(payload))` when the message for that
    /// `msgId` is complete, `Ok(None)` while still waiting for more chunks.
    pub fn push(&mut self, _packet: &[u8]) -> Result<Option<Vec<u8>>, ChunkError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // helper: round-trip a payload through split + a fresh Reassembler.
    fn round_trip(msg_id: u32, payload: &[u8], max_payload: usize) -> Vec<u8> {
        let packets = split(msg_id, payload, max_payload).expect("split ok");
        let mut r = Reassembler::new();
        let mut out = None;
        for p in &packets {
            if let Some(done) = r.push(p).expect("push ok") {
                out = Some(done);
            }
        }
        out.expect("reassembled")
    }

    #[test]
    fn payload_limit_arithmetic() {
        // 247 - 3 - 6 = 238 ; 23 - 3 - 6 = 14
        assert_eq!(payload_limit(247), 238);
        assert_eq!(payload_limit(23), 14);
    }

    #[test]
    fn payload_limit_too_small_saturates_to_zero() {
        assert_eq!(payload_limit(9), 0);
        assert_eq!(payload_limit(0), 0);
    }

    #[test]
    fn single_chunk_round_trip() {
        let payload = b"hello world";
        let packets = split(7, payload, 238).expect("split");
        assert_eq!(packets.len(), 1, "total=1 expected (no split)");
        // header sanity: total byte == 1, seq byte == 0
        assert_eq!(packets[0][4], 0, "seq");
        assert_eq!(packets[0][5], 1, "total");
        assert_eq!(round_trip(7, payload, 238), payload);
    }

    #[test]
    fn empty_payload_round_trip() {
        let payload: &[u8] = b"";
        let packets = split(1, payload, 238).expect("split");
        assert_eq!(packets.len(), 1);
        assert_eq!(round_trip(1, payload, 238), payload);
    }

    #[test]
    fn long_payload_splits_and_reassembles_mtu247() {
        // payload_limit for MTU 247 is 238; use something well over it.
        let payload: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let max = payload_limit(247);
        let packets = split(42, &payload, max).expect("split");
        assert!(packets.len() > 1, "must split into multiple chunks");
        // every chunk's payload portion respects the limit
        for p in &packets {
            assert!(p.len() - HEADER_LEN <= max);
        }
        assert_eq!(round_trip(42, &payload, max), payload);
    }

    #[test]
    fn out_of_order_reassembly() {
        let payload: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let mut packets = split(99, &payload, payload_limit(247)).expect("split");
        assert!(packets.len() >= 3);
        packets.reverse(); // feed in reverse order
        let mut r = Reassembler::new();
        let mut out = None;
        for p in &packets {
            if let Some(done) = r.push(p).expect("push") {
                out = Some(done);
            }
        }
        assert_eq!(out.expect("done"), payload);
    }

    #[test]
    fn interleaved_msg_ids() {
        let p_a: Vec<u8> = (0..500u32).map(|i| (i % 251) as u8).collect();
        let p_b: Vec<u8> = (0..500u32).map(|i| ((i + 7) % 251) as u8).collect();
        let max = payload_limit(247);
        let packets_a = split(1, &p_a, max).expect("split a");
        let packets_b = split(2, &p_b, max).expect("split b");
        assert!(packets_a.len() >= 2 && packets_b.len() >= 2);

        let mut r = Reassembler::new();
        let mut done_a = None;
        let mut done_b = None;
        // interleave: a0, b0, a1, b1, ...
        let n = packets_a.len().max(packets_b.len());
        for i in 0..n {
            if let Some(p) = packets_a.get(i) {
                if let Some(d) = r.push(p).expect("push a") {
                    done_a = Some(d);
                }
            }
            if let Some(p) = packets_b.get(i) {
                if let Some(d) = r.push(p).expect("push b") {
                    done_b = Some(d);
                }
            }
        }
        assert_eq!(done_a.expect("a done"), p_a);
        assert_eq!(done_b.expect("b done"), p_b);
    }

    #[test]
    fn packet_too_short_errors() {
        let mut r = Reassembler::new();
        assert_eq!(r.push(&[0, 0, 0]), Err(ChunkError::PacketTooShort));
    }

    #[test]
    fn total_zero_errors() {
        // header with total = 0
        let pkt = [0u8, 0, 0, 1, 0, 0];
        let mut r = Reassembler::new();
        assert_eq!(r.push(&pkt), Err(ChunkError::InvalidTotal));
    }

    #[test]
    fn seq_out_of_range_errors() {
        // total = 2, seq = 2 (must be < total)
        let pkt = [0u8, 0, 0, 1, 2, 2];
        let mut r = Reassembler::new();
        assert_eq!(r.push(&pkt), Err(ChunkError::SeqOutOfRange));
    }

    #[test]
    fn total_mismatch_errors() {
        let mut r = Reassembler::new();
        // first chunk says total=2
        let p0 = [0u8, 0, 0, 1, 0, 2, b'x'];
        assert_eq!(r.push(&p0), Ok(None));
        // second chunk for same msgId says total=3
        let p1 = [0u8, 0, 0, 1, 1, 3, b'y'];
        assert_eq!(r.push(&p1), Err(ChunkError::TotalMismatch));
    }

    #[test]
    fn too_many_chunks_errors() {
        // max_payload = 1 with a 300-byte payload would need 300 chunks > 255.
        let payload = vec![0u8; 300];
        assert_eq!(split(1, &payload, 1), Err(ChunkError::TooManyChunks));
    }

    #[test]
    fn header_encoding_is_big_endian() {
        let packets = split(0x01020304, b"z", 238).expect("split");
        assert_eq!(&packets[0][0..4], &[0x01, 0x02, 0x03, 0x04]);
    }
}
