//! BLE interop: framing (JSON payloads) and chunking (wire packets).
//!
//! Pure logic only — no `btleplug` / OS transport here. See
//! `docs/BLE_CONTRACT.md` for the canonical wire format.
//!
//! This module is a public codec API consumed by the transport layer in a
//! later PR; until that wiring lands, its items are not referenced by the
//! binary, so dead-code warnings are silenced here.
#![allow(dead_code)]

pub mod chunk;
pub mod frame;
pub mod session;
