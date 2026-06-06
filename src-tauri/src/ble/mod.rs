//! BLE interop: framing (JSON payloads) and chunking (wire packets).
//!
//! Pure logic only — no `btleplug` / OS transport here. See
//! `docs/BLE_CONTRACT.md` for the canonical wire format.

pub mod chunk;
pub mod frame;
