//! Tauri entry point + BLE command/event wiring (PR-B2b-2).
//!
//! The frontend drives BLE over two commands and a stream of events:
//! - [`ble_start`] scans/connects/runs the transport for one peer.
//! - [`ble_send`] enqueues a [`Frame`] (parsed from JSON) onto the live link.
//! - [`ble_stop`] tears the current connection down (cancels the driver +
//!   disconnects the GATT link).
//! - events `ble://connected | frame | stats | disconnected | error` are emitted
//!   to the webview as the connection progresses.
//!
//! All btleplug plumbing lives in [`ble::central`]; this module only owns the
//! Tauri state (the outbound [`mpsc::Sender`] of the current link) and the bridge
//! from [`TransportEvent`] to `app.emit`.

mod ai;
mod ble;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot};

use ble::central::{adapter_available, connect_and_run_multi};
use ble::frame::Frame;
use ble::transport::{LinkError, TransportEvent};
use tokio::sync::broadcast;

/// Broadcast capacity for app→link outbound frames. In Room Model A a single
/// send fans out to every connected peer; the capacity bounds how far a slow
/// peer may lag before it drops missed frames.
const OUTBOUND_CAPACITY: usize = 64;
/// Channel capacity for link→app events.
const EVENTS_CAPACITY: usize = 256;

/// Shared Tauri state for the multi-peer BLE supervisor (Room Model A).
///
/// - `outbound` — a broadcast sender; `ble_send` publishes one frame and every
///   connected peer's task receives a clone (group fan-out). Present while the
///   supervisor runs.
/// - `shutdown` — the one-shot stop signal for the supervisor. Firing it cancels
///   the continuous scan loop and, by dropping the broadcast sender it owns,
///   winds every per-peer task down. `ble_start` fires the *previous* one before
///   starting a fresh supervisor so nothing leaks across a restart.
#[derive(Default)]
struct BleState {
    outbound: Mutex<Option<broadcast::Sender<Frame>>>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
}

/// JSON-friendly rendering of [`LinkError`] for the `ble://error` event.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase", tag = "kind", content = "message")]
enum LinkErrorPayload {
    Io(String),
    Disconnected,
}

impl From<LinkError> for LinkErrorPayload {
    fn from(e: LinkError) -> Self {
        match e {
            LinkError::Io(m) => LinkErrorPayload::Io(m),
            LinkError::Disconnected => LinkErrorPayload::Disconnected,
        }
    }
}

/// Counter snapshot emitted on `ble://stats` (mirrors [`TransportEvent::Stats`]).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StatsPayload {
    protocol_violations: u64,
    impersonation_rejections: u64,
    incompatible_proto: u64,
}

/// Bridges a [`TransportEvent`] to its `ble://…` webview emit.
fn emit_event(app: &AppHandle, ev: TransportEvent) {
    match ev {
        TransportEvent::Connected => {
            let _ = app.emit("ble://connected", ());
        }
        // `Frame` derives Serialize with the same camelCase wire shape the UI
        // already understands, so it is emitted as-is.
        TransportEvent::Frame(frame) => {
            let _ = app.emit("ble://frame", frame);
        }
        TransportEvent::Stats {
            protocol_violations,
            impersonation_rejections,
            incompatible_proto,
        } => {
            let _ = app.emit(
                "ble://stats",
                StatsPayload {
                    protocol_violations,
                    impersonation_rejections,
                    incompatible_proto,
                },
            );
        }
        TransportEvent::LinkError(e) => {
            let _ = app.emit("ble://error", LinkErrorPayload::from(e));
        }
        TransportEvent::Disconnected => {
            let _ = app.emit("ble://disconnected", ());
        }
    }
}

/// Starts the multi-peer BLE supervisor (Room Model A): scan continuously,
/// auto-connect to every LiqMesh peer, keep all links alive, and bridge their
/// merged events to `ble://…` emits.
///
/// A previous supervisor (if any) is **explicitly torn down first**: its stored
/// `shutdown` sender is fired, cancelling its scan loop and (by dropping the old
/// broadcast sender) every per-peer task. Only then are the new outbound
/// (broadcast) / shutdown handles installed. Returns once the background tasks
/// are spawned; connection progress arrives via events. Idempotent to call on
/// every UI mount.
#[tauri::command]
async fn ble_start(
    app: AppHandle,
    state: State<'_, BleState>,
    my_id: String,
    my_name: String,
) -> Result<(), String> {
    if my_id.trim().is_empty() {
        return Err("myId must not be empty".into());
    }

    let (out_tx, _out_rx0) = broadcast::channel::<Frame>(OUTBOUND_CAPACITY);
    let (ev_tx, mut ev_rx) = mpsc::channel::<TransportEvent>(EVENTS_CAPACITY);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // Stop any prior supervisor (cancels its scan + per-peer tasks), then install
    // the new handles. The broadcast sender is cloned into the supervisor; the
    // copy stored here is what `ble_send` publishes to.
    {
        let mut shutdown_guard = state
            .shutdown
            .lock()
            .map_err(|_| "BLE state lock poisoned".to_string())?;
        if let Some(old) = shutdown_guard.take() {
            let _ = old.send(());
        }
        *shutdown_guard = Some(shutdown_tx);

        let mut out_guard = state
            .outbound
            .lock()
            .map_err(|_| "BLE state lock poisoned".to_string())?;
        *out_guard = Some(out_tx.clone());
    }

    // Bridge merged per-peer events → webview, AND multi-hop flood-relay.
    //
    // Relay (Room Model A, group only): on a NEW inbound `msg` we re-broadcast the
    // EXACT same frame (original id/senderId/senderName/roomId/body/createdAt — no
    // re-stamp) to every connected peer, so a message reaches the whole mesh even
    // without full connectivity. Dedup by `msg.id` in a per-session seen-set: a
    // msg we've already handled is neither displayed again (the store also dedups)
    // nor relayed again — that finite-per-id rule is what prevents relay loops, so
    // no TTL/hop field is needed (and none exists on the wire, keeping byte-compat
    // with iOS/Android). The arrival link gets the echo too, but its remote peer
    // dedups by the same id, so re-sending to all (not "all-but-arrival") is safe.
    let app_for_events = app.clone();
    let relay_tx = out_tx.clone();
    tokio::spawn(async move {
        let mut seen_msg_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        while let Some(ev) = ev_rx.recv().await {
            if let TransportEvent::Frame(Frame::Msg { id, .. }) = &ev {
                // First time we see this id → relay it; otherwise drop (already
                // displayed + relayed). New ids are inserted; repeats short-circuit.
                if seen_msg_ids.insert(id.clone()) {
                    if let TransportEvent::Frame(frame) = &ev {
                        // broadcast::send errors only with zero receivers (no peers) — fine.
                        let _ = relay_tx.send(frame.clone());
                    }
                } else {
                    // Duplicate arrival: don't re-display or re-relay.
                    continue;
                }
            }
            emit_event(&app_for_events, ev);
        }
    });

    // Run the supervisor. It owns `out_tx` (for per-peer subscriptions), `ev_tx`,
    // and `shutdown_rx` for its lifetime.
    tokio::spawn(connect_and_run_multi(my_id, my_name, ev_tx, out_tx, shutdown_rx));

    Ok(())
}

/// Stops the current BLE session, if any.
///
/// Fires the stored `shutdown` signal (cancelling the running `Driver::run` and
/// tearing down the GATT link + helper tasks) and clears the outbound sender so
/// subsequent `ble_send` calls fail fast. Idempotent: a no-op when nothing is
/// connected.
#[tauri::command]
async fn ble_stop(state: State<'_, BleState>) -> Result<(), String> {
    {
        let mut shutdown_guard = state
            .shutdown
            .lock()
            .map_err(|_| "BLE state lock poisoned".to_string())?;
        if let Some(tx) = shutdown_guard.take() {
            let _ = tx.send(());
        }
    }
    {
        let mut out_guard = state
            .outbound
            .lock()
            .map_err(|_| "BLE state lock poisoned".to_string())?;
        *out_guard = None;
    }
    Ok(())
}

/// Broadcasts one frame (parsed strictly from JSON) to every connected peer
/// (Room Model A group fan-out).
///
/// Fails if the JSON is not a valid [`Frame`] or if the supervisor is not
/// running. A successful publish with **no peers yet connected** is not an error
/// (the optimistic local echo already showed the message); it simply reaches
/// zero links.
#[tauri::command]
async fn ble_send(state: State<'_, BleState>, frame_json: String) -> Result<(), String> {
    let frame: Frame =
        serde_json::from_str(&frame_json).map_err(|e| format!("invalid frame JSON: {e}"))?;

    let sender = {
        let guard = state
            .outbound
            .lock()
            .map_err(|_| "BLE state lock poisoned".to_string())?;
        guard.clone()
    };
    match sender {
        // `broadcast::send` errors only when there are no receivers. With Room
        // Model A that just means no peers are connected yet — not a failure.
        Some(tx) => {
            let _ = tx.send(frame);
            Ok(())
        }
        None => Err("BLE supervisor not running".into()),
    }
}

/// Result of the launch-time Bluetooth precheck (`ble_available`).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BleAvailability {
    /// True when a usable Bluetooth adapter is present.
    available: bool,
    /// Human-readable reason when `available` is false (else `None`).
    reason: Option<String>,
}

/// Probe whether Bluetooth is usable, so the UI can prompt the user to enable it
/// at launch instead of letting a later scan silently fail. Never errors — the
/// outcome is carried in the returned struct.
#[tauri::command]
async fn ble_available() -> BleAvailability {
    match adapter_available().await {
        Ok(()) => BleAvailability { available: true, reason: None },
        Err(reason) => BleAvailability { available: false, reason: Some(reason) },
    }
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(BleState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            ble_start,
            ble_send,
            ble_stop,
            ble_available,
            ai::ai_status,
            ai::ai_download,
            ai::ai_ask,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
