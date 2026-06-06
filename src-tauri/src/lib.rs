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

mod ble;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot};

use ble::central::connect_and_run;
use ble::frame::Frame;
use ble::transport::{LinkError, TransportEvent};

/// Channel capacity for app→link outbound frames. Small: the UI sends one frame
/// per user action, so backpressure here is effectively never hit.
const OUTBOUND_CAPACITY: usize = 64;
/// Channel capacity for link→app events.
const EVENTS_CAPACITY: usize = 256;

/// Shared Tauri state: the live link handles of the *current* connection.
///
/// - `outbound` — `ble_send` reads it to enqueue a frame; `ble_start`/`ble_stop`
///   replace/clear it.
/// - `shutdown` — the one-shot stop signal for the current connection. Firing it
///   cancels the running `Driver::run` and forces a full teardown (see
///   [`ble::central::connect_and_run`]). `ble_start` fires the *previous* one
///   before creating a new connection so the old driver/notif/tick tasks and
///   GATT link are guaranteed to wind down — dropping the outbound sender alone
///   is not sufficient because the driver's tick interval never closes.
#[derive(Default)]
struct BleState {
    outbound: Mutex<Option<mpsc::Sender<Frame>>>,
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

/// Starts a BLE session: scans for a Contract peer, connects, and runs the
/// transport driver, bridging its events to `ble://…` emits.
///
/// A previous connection (if any) is **explicitly torn down first**: its stored
/// `shutdown` sender is fired, which cancels the old `Driver::run` and disconnects
/// the old GATT link + helper tasks (see [`ble::central::connect_and_run`]). Only
/// then are the new outbound/shutdown handles installed. Returns once the
/// background tasks are spawned; connection progress arrives via events.
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

    let (out_tx, out_rx) = mpsc::channel::<Frame>(OUTBOUND_CAPACITY);
    let (ev_tx, mut ev_rx) = mpsc::channel::<TransportEvent>(EVENTS_CAPACITY);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // Stop any prior connection (cancels its driver → full teardown), then
    // install the new link's handles.
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
        *out_guard = Some(out_tx);
    }

    // Bridge driver events → webview.
    let app_for_events = app.clone();
    tokio::spawn(async move {
        while let Some(ev) = ev_rx.recv().await {
            emit_event(&app_for_events, ev);
        }
    });

    // Run the connection. It owns `out_rx`/`ev_tx`/`shutdown_rx` for its lifetime.
    tokio::spawn(connect_and_run(my_id, my_name, ev_tx, out_rx, shutdown_rx));

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

/// Enqueues one frame (parsed strictly from JSON) onto the current connection.
///
/// Fails if the JSON is not a valid [`Frame`] or if no connection is active.
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
        Some(tx) => tx
            .send(frame)
            .await
            .map_err(|_| "no active BLE connection (link closed)".to_string()),
        None => Err("no active BLE connection".into()),
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
            greet, ble_start, ble_send, ble_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
