//! Lightweight std-only diagnostic file log for field debugging.
//!
//! The BLE path is instrumented with `eprintln!` calls that are only visible in
//! a `tauri dev` terminal — in a **built `.app`** they go nowhere, so a user
//! reporting "connected but no messages arrive" leaves no trace to inspect.
//! This module appends the same diagnostics (plus the raw inbound bytes) to a
//! persistent file so a built app can be debugged after the fact:
//!
//! ```text
//! ~/Library/Logs/liqmesh-desktop.log
//! ```
//!
//! Design constraints:
//! - **std only** — no `log`/`tracing`/`chrono` dependency (keeps the build
//!   fast and avoids pulling a logging framework in just for field triage).
//! - **best-effort** — every write is fire-and-forget; a logging failure must
//!   never affect the BLE path. All I/O errors are swallowed.
//! - **append + line-buffered** — each record is one `\n`-terminated line so
//!   `tail -f` works and concurrent tasks don't interleave mid-line (a single
//!   `write_all` of the whole line is effectively atomic for these sizes).
//!
//! The timestamp is milliseconds since the Unix epoch (std `SystemTime`); we
//! deliberately avoid a human calendar format to stay dependency-free — pair it
//! with `date -r <seconds>` when reading if a wall-clock is needed.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Process-wide handle to the open log file, initialized on first use.
///
/// `Mutex<Option<File>>`: `None` means we tried to open the file and failed (or
/// could not resolve the path) — in that case logging silently no-ops for the
/// rest of the run rather than retrying on every call.
static LOG: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

/// Resolves `~/Library/Logs/liqmesh-desktop.log`.
///
/// Uses `$HOME` directly (std only). Returns `None` if `$HOME` is unset, in
/// which case logging is disabled for the run.
fn log_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut p = PathBuf::from(home);
    p.push("Library");
    p.push("Logs");
    // Best-effort: create the Logs dir if missing (it normally exists on macOS).
    let _ = std::fs::create_dir_all(&p);
    p.push("liqmesh-desktop.log");
    Some(p)
}

/// Opens (append, create) the log file once and caches the handle.
fn handle() -> &'static Mutex<Option<std::fs::File>> {
    LOG.get_or_init(|| {
        let file = log_path().and_then(|path| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        });
        Mutex::new(file)
    })
}

/// Milliseconds since the Unix epoch, or `0` if the clock is before the epoch.
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Appends one diagnostic line: `<epoch_ms> [ble] <msg>\n`.
///
/// Best-effort and infallible from the caller's view: a poisoned lock or an I/O
/// error is swallowed so diagnostics can never disturb the BLE path.
pub fn line(msg: &str) {
    if let Ok(mut guard) = handle().lock() {
        if let Some(file) = guard.as_mut() {
            // Single write_all of the full line keeps concurrent records from
            // interleaving mid-line.
            let _ = writeln!(file, "{} [ble] {msg}", now_ms());
            let _ = file.flush();
        }
    }
}

/// Renders bytes as lowercase hex for logging raw wire packets.
///
/// Capped at `max` bytes (with a `…(+N)` suffix) so a large reassembled payload
/// can't bloat the log; pass `usize::MAX` to render everything.
pub fn hex(bytes: &[u8], max: usize) -> String {
    let shown = bytes.len().min(max);
    let mut s = String::with_capacity(shown * 2 + 8);
    for b in &bytes[..shown] {
        s.push_str(&format!("{b:02x}"));
    }
    if bytes.len() > shown {
        s.push_str(&format!("…(+{})", bytes.len() - shown));
    }
    s
}

/// Describes a wire packet's chunk header for logging.
///
/// The wire framing (per `docs/BLE_CONTRACT.md` / `chunk.rs`) is
/// `[msgId:4 BE][seq:1][total:1][payload…]`. This parses just those 6 header
/// bytes so the receive log can show msgId/seq/total/payloadLen per chunk —
/// the key signal for diagnosing a reassembly that never completes (e.g. a
/// missing seq, a mismatched total, or a single chunk that never arrives).
///
/// Returns a `"<no chunk header: len=N>"` marker for a packet shorter than the
/// 6-byte header rather than panicking (a malformed/short packet is itself a
/// useful diagnostic).
pub fn chunk_header(packet: &[u8]) -> String {
    if packet.len() < 6 {
        return format!("<no chunk header: len={}>", packet.len());
    }
    let msg_id = u32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]);
    let seq = packet[4];
    let total = packet[5];
    let payload_len = packet.len() - 6;
    format!("msgId={msg_id} seq={seq}/{total} payloadLen={payload_len}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_header_parses_msgid_seq_total() {
        // msgId=1 (BE), seq=2, total=5, 3 payload bytes.
        let pkt = [0, 0, 0, 1, 2, 5, 0xaa, 0xbb, 0xcc];
        assert_eq!(chunk_header(&pkt), "msgId=1 seq=2/5 payloadLen=3");
    }

    #[test]
    fn chunk_header_marks_short_packet() {
        assert_eq!(chunk_header(&[0, 0, 0]), "<no chunk header: len=3>");
    }

    #[test]
    fn hex_renders_lowercase_and_caps() {
        assert_eq!(hex(&[0x00, 0xab, 0xff], usize::MAX), "00abff");
        // Cap at 2 bytes → 4 hex chars + a remainder marker.
        assert_eq!(hex(&[0x01, 0x02, 0x03, 0x04], 2), "0102…(+2)");
        assert_eq!(hex(&[], usize::MAX), "");
    }

    #[test]
    fn now_ms_is_after_2020() {
        // Sanity: the clock is well past 2020-01-01 (1_577_836_800_000 ms).
        assert!(now_ms() > 1_577_836_800_000);
    }

    #[test]
    fn line_never_panics_without_home() {
        // Even if the file could not be opened, logging must be a silent no-op.
        // (We don't assert file contents here — that would depend on $HOME and
        // pollute a real log; we only assert the call is infallible.)
        line("test diagnostic line — safe to ignore");
    }
}
