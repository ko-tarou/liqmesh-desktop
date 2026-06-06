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
//! INTEROP (RESOLVED): `seq`/`total` numbering is **0-based**
//! (`seq ∈ 0..total`, `total` = chunk count, `total == 1` = unsplit). Confirmed
//! in the architect session and matched against the existing iOS/Android wire —
//! all three platforms agree, so no code change is required on any side. See
//! `.claude/plans/001-2_ble-codec.md`.

use std::collections::HashMap;

/// Fixed header length in bytes: msgId(4) + seq(1) + total(1).
pub const HEADER_LEN: usize = 6;
/// ATT protocol overhead subtracted from the negotiated MTU.
pub const ATT_OVERHEAD: usize = 3;
/// Maximum number of chunks (`total` must fit in a single byte).
pub const MAX_CHUNKS: usize = u8::MAX as usize; // 255

/// Returns the maximum payload size that fits in a single packet for the given
/// negotiated MTU: `mtu - 3(ATT) - 6(header)`.
///
/// If the MTU is too small to fit even the ATT + header overhead, returns 0.
pub fn payload_limit(mtu: usize) -> usize {
    mtu.saturating_sub(ATT_OVERHEAD + HEADER_LEN)
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
///
/// Each packet is `[msgId:4 BE][seq:1][total:1][payload-chunk]`. `seq` is
/// 0-based (`0..total`). An empty payload yields a single packet with
/// `total == 1` and an empty payload chunk.
///
/// Returns [`ChunkError::TooManyChunks`] if the payload would require more than
/// 255 chunks for the given `max_payload`.
///
/// `max_payload == 0` is a caller bug (a packet must carry at least one payload
/// byte to make progress). Rather than panicking, it returns
/// [`ChunkError::TooManyChunks`] — including for an empty payload, so the error
/// path is unified and the bug is surfaced early at the call site.
pub fn split(msg_id: u32, payload: &[u8], max_payload: usize) -> Result<Vec<Vec<u8>>, ChunkError> {
    if max_payload == 0 {
        return Err(ChunkError::TooManyChunks);
    }

    // Even an empty payload is one chunk (total == 1, no split).
    let total = if payload.is_empty() {
        1
    } else {
        payload.len().div_ceil(max_payload)
    };

    if total > MAX_CHUNKS {
        return Err(ChunkError::TooManyChunks);
    }

    let id = msg_id.to_be_bytes();
    let mut packets = Vec::with_capacity(total);
    for seq in 0..total {
        let start = seq * max_payload;
        let end = (start + max_payload).min(payload.len());
        let chunk = &payload[start..end];

        let mut pkt = Vec::with_capacity(HEADER_LEN + chunk.len());
        pkt.extend_from_slice(&id);
        pkt.push(seq as u8);
        pkt.push(total as u8);
        pkt.extend_from_slice(chunk);
        packets.push(pkt);
    }
    Ok(packets)
}

/// In-progress reassembly state for a single `msgId`.
struct Partial {
    total: u8,
    /// One slot per `seq`; `None` until that chunk arrives.
    chunks: Vec<Option<Vec<u8>>>,
    received: usize,
}

/// Reassembles chunked packets into complete payloads, keyed by `msgId`.
///
/// Supports out-of-order delivery and multiple concurrent `msgId`s.
#[derive(Default)]
pub struct Reassembler {
    partials: HashMap<u32, Partial>,
}

impl Reassembler {
    pub fn new() -> Self {
        Reassembler {
            partials: HashMap::new(),
        }
    }

    /// Feeds one packet. Returns `Ok(Some(payload))` when the message for that
    /// `msgId` is complete, `Ok(None)` while still waiting for more chunks.
    pub fn push(&mut self, packet: &[u8]) -> Result<Option<Vec<u8>>, ChunkError> {
        if packet.len() < HEADER_LEN {
            return Err(ChunkError::PacketTooShort);
        }
        let msg_id = u32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]);
        let seq = packet[4];
        let total = packet[5];
        let body = &packet[HEADER_LEN..];

        if total == 0 {
            return Err(ChunkError::InvalidTotal);
        }
        if seq >= total {
            return Err(ChunkError::SeqOutOfRange);
        }

        let entry = self.partials.entry(msg_id).or_insert_with(|| Partial {
            total,
            chunks: vec![None; total as usize],
            received: 0,
        });

        if entry.total != total {
            return Err(ChunkError::TotalMismatch);
        }

        // Idempotent on duplicate seq: only count the first arrival.
        if entry.chunks[seq as usize].is_none() {
            entry.chunks[seq as usize] = Some(body.to_vec());
            entry.received += 1;
        }

        if entry.received == entry.total as usize {
            let partial = self.partials.remove(&msg_id).expect("present");
            let mut out = Vec::new();
            for chunk in partial.chunks {
                out.extend_from_slice(&chunk.expect("all chunks present"));
            }
            return Ok(Some(out));
        }
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
    fn split_with_zero_max_payload_errors_not_panics() {
        // max_payload == 0 is a caller bug; surface it as an error, never panic.
        assert_eq!(split(1, b"x", 0), Err(ChunkError::TooManyChunks));
    }

    #[test]
    fn split_empty_payload_with_zero_max_payload_errors() {
        // Even an empty payload with max_payload == 0 is treated as a caller bug
        // (unified error path for early detection), not a successful 1-chunk split.
        assert_eq!(split(2, b"", 0), Err(ChunkError::TooManyChunks));
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
