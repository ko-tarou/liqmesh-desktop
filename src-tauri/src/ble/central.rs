//! Concrete btleplug-backed [`GattLink`] + connection manager (PR-B2b-2).
//!
//! This is the only module that touches real OS BLE. It implements the
//! `central` half of the Contract (`docs/BLE_CONTRACT.md`): Windows Desktop is
//! **central-only** — it scans for the fixed Service UUID, connects, subscribes
//! to the RX notify characteristic, and writes the TX characteristic. The pure
//! codec/session/driver layers below it are unchanged and fully unit-tested; this
//! module only wires them to btleplug and a monotonic clock.
//!
//! ## Flow ([`connect_and_run`])
//! 1. `Manager::new()` → first `adapter`.
//! 2. `start_scan(ScanFilter{ services: [SERVICE_UUID] })`; watch the central
//!    event stream + poll `peripherals()` until a peer advertising the service
//!    (or a `"LQM-"` local name) appears, or a timeout fires.
//! 3. `connect()` → `discover_services()` → resolve TX (`…0002`) / RX (`…0003`).
//! 4. `subscribe(RX)` + spawn a task pumping `notifications()` into the driver's
//!    inbound channel.
//! 5. Build a [`Session`], a [`BtleLink`], a monotonic `Instant` clock, and a
//!    `tokio::time::interval` tick task, then hand everything to [`Driver::run`].
//!
//! No `unwrap`/`expect`/`panic` on any runtime path: every failure is surfaced to
//! the caller as a [`TransportEvent::LinkError`] over the `events` channel and
//! the function returns cleanly.

use std::time::{Duration, Instant};

use btleplug::api::{
    Central, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use super::frame::Frame;
use super::session::Session;
use super::transport::{Driver, GattLink, LinkError, TransportEvent};

/// GATT Service UUID (Contract, fixed): `B1E5C0DE-1A2B-4C3D-8E9F-000000000001`.
pub const SERVICE_UUID: Uuid = Uuid::from_u128(0xB1E5C0DE_1A2B_4C3D_8E9F_000000000001);
/// TX (Write, central→peripheral): `…0002`.
pub const TX_CHAR_UUID: Uuid = Uuid::from_u128(0xB1E5C0DE_1A2B_4C3D_8E9F_000000000002);
/// RX (Notify, peripheral→central): `…0003`.
pub const RX_CHAR_UUID: Uuid = Uuid::from_u128(0xB1E5C0DE_1A2B_4C3D_8E9F_000000000003);

/// Contract advertise localName prefix used as a secondary discovery signal when
/// the advertised service UUID is not visible (some stacks omit it).
const LOCAL_NAME_PREFIX: &str = "LQM-";

/// How long to scan for a matching peripheral before giving up.
const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
/// Poll cadence while waiting for a discovered peripheral to match.
const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(300);
/// Maintenance-tick cadence handed to [`Driver::run`] (drives stale-reassembly
/// eviction + a `Stats` snapshot).
const TICK_INTERVAL: Duration = Duration::from_secs(5);
/// Bound on the internal RX/tick channels. Generous enough that a burst of
/// chunked notifications never blocks the OS notification pump.
const CHANNEL_CAPACITY: usize = 256;

/// Whether a usable Bluetooth adapter is present, for a launch-time precheck.
///
/// Returns `Ok(())` when `Manager::new()` succeeds and at least one adapter is
/// reported, mirroring the acquisition `connect_and_run` does. On error it
/// returns a short human-readable reason for the UI to surface.
///
/// Caveat (btleplug 0.12 / WinRT): a *present* adapter is detected reliably, but
/// a radio that is merely toggled **off** in Windows settings may still be
/// listed here — that case surfaces later as a scan that finds nothing. We
/// detect "no adapter / no BLE stack", which is the common owner failure.
pub async fn adapter_available() -> Result<(), String> {
    let manager = Manager::new()
        .await
        .map_err(|e| format!("Bluetooth stack unavailable: {e}"))?;
    match manager.adapters().await {
        Ok(a) if !a.is_empty() => Ok(()),
        Ok(_) => Err("no Bluetooth adapter found".into()),
        Err(e) => Err(format!("could not query Bluetooth adapters: {e}")),
    }
}

/// Concrete [`GattLink`] over a connected btleplug [`Peripheral`].
///
/// Holds the peripheral handle and the resolved TX characteristic; every
/// [`GattLink::write`] issues a *WriteWithResponse* to TX, mapping any btleplug
/// error to [`LinkError::Io`] so the driver can tear the link down gracefully.
pub struct BtleLink {
    peripheral: Peripheral,
    tx_char: btleplug::api::Characteristic,
}

impl GattLink for BtleLink {
    async fn write(&self, packet: Vec<u8>) -> Result<(), LinkError> {
        self.peripheral
            .write(&self.tx_char, &packet, WriteType::WithResponse)
            .await
            .map_err(|e| LinkError::Io(e.to_string()))
    }
}

/// Sends a [`TransportEvent::LinkError`] with an `Io` message, ignoring a closed
/// receiver (the caller has already gone away — nothing more we can do).
async fn report_io(events: &mpsc::Sender<TransportEvent>, msg: impl Into<String>) {
    let _ = events
        .send(TransportEvent::LinkError(LinkError::Io(msg.into())))
        .await;
}

/// True if a discovered peripheral matches our Contract target: it advertises the
/// Service UUID, or carries a `"LQM-"` local name (secondary signal).
async fn peripheral_matches(p: &Peripheral) -> bool {
    match p.properties().await {
        Ok(Some(props)) => {
            props.services.contains(&SERVICE_UUID)
                || props
                    .local_name
                    .as_deref()
                    .is_some_and(|n| n.starts_with(LOCAL_NAME_PREFIX))
        }
        // No properties yet (or a transient read error): treat as "not a match
        // yet" and let the scan loop poll again.
        _ => false,
    }
}

/// Scans `adapter` until a matching peripheral appears or [`SCAN_TIMEOUT`]
/// elapses. Returns the matched [`Peripheral`] on success.
async fn scan_for_peer(adapter: &Adapter) -> Result<Peripheral, String> {
    adapter
        .start_scan(ScanFilter {
            services: vec![SERVICE_UUID],
        })
        .await
        .map_err(|e| format!("start_scan failed: {e}"))?;

    let deadline = Instant::now() + SCAN_TIMEOUT;
    loop {
        // Poll the discovered set; some platforms surface matches here even when
        // the event stream lags.
        match adapter.peripherals().await {
            Ok(found) => {
                for p in found {
                    if peripheral_matches(&p).await {
                        let _ = adapter.stop_scan().await;
                        return Ok(p);
                    }
                }
            }
            Err(e) => {
                let _ = adapter.stop_scan().await;
                return Err(format!("peripherals() failed: {e}"));
            }
        }

        if Instant::now() >= deadline {
            let _ = adapter.stop_scan().await;
            return Err(format!(
                "no LiqMesh peripheral found within {}s",
                SCAN_TIMEOUT.as_secs()
            ));
        }
        tokio::time::sleep(SCAN_POLL_INTERVAL).await;
    }
}

/// How often the multi-peer supervisor re-polls the adapter for newly-arrived
/// peripherals. The scan is left running continuously; this only bounds how
/// quickly a freshly-advertised peer is noticed.
const MULTI_SCAN_POLL_INTERVAL: Duration = Duration::from_millis(800);

/// Continuous multi-peer supervisor (Room Model A): scan forever and
/// auto-connect to EVERY discovered LiqMesh peer, keeping all links alive at
/// once. Each connected peer runs its own [`run_peer`] task; inbound frames from
/// all peers fan into the single shared `events` stream (so the UI sees one
/// merged feed), and outbound frames are broadcast to every live link via
/// `outbound_tx`.
///
/// Unlike [`connect_and_run`] (single peer, stops after one connect), this never
/// stops scanning, so peers that appear later are picked up automatically and
/// peers that drop are re-connected on their next advertisement. `shutdown`
/// cancels the whole supervisor (and, by dropping the broadcast sender, every
/// per-peer task).
pub async fn connect_and_run_multi(
    my_id: String,
    my_name: String,
    events: mpsc::Sender<TransportEvent>,
    outbound_tx: broadcast::Sender<Frame>,
    shutdown: oneshot::Receiver<()>,
) {
    let manager = match Manager::new().await {
        Ok(m) => m,
        Err(e) => return report_io(&events, format!("Manager::new failed: {e}")).await,
    };
    let adapter = match manager.adapters().await {
        Ok(mut a) if !a.is_empty() => a.remove(0),
        Ok(_) => return report_io(&events, "no Bluetooth adapter found").await,
        Err(e) => return report_io(&events, format!("adapters() failed: {e}")).await,
    };
    // Empty scan filter (NOT a service-UUID filter). On macOS/CoreBluetooth a
    // service-filtered scan misses peers whose advertised service UUID lands in
    // the "overflow" area (notably backgrounded iOS apps) — that silently capped
    // discovery. We scan everything and filter in `peripheral_matches` (which
    // checks both the service UUID and the `LQM-` local name).
    if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
        return report_io(&events, format!("start_scan failed: {e}")).await;
    }

    // Peripheral ids we currently have a live (or in-flight) task for, so the
    // poll loop never double-connects the same peer. Shared with each `run_peer`
    // so a peer that DROPS removes itself here and can be re-connected on its
    // next advertisement (otherwise a single disconnect would permanently shrink
    // the mesh — a cause of the "inconsistent peer counts").
    let connected: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            // External stop: end the supervisor. Dropping `outbound_tx` (held
            // only here) closes every per-peer broadcast receiver, winding the
            // spawned tasks down; the scan is stopped on the way out.
            _ = &mut shutdown => break,
            _ = tokio::time::sleep(MULTI_SCAN_POLL_INTERVAL) => {
                let found = match adapter.peripherals().await {
                    Ok(f) => f,
                    Err(_) => continue, // transient; try again next tick
                };
                let mut discovered = 0usize;
                for p in found {
                    let id = p.id().to_string();
                    // Cheap dedup check first (short lock, never held across await).
                    if connected.lock().unwrap().contains(&id) {
                        continue;
                    }
                    // `peripheral_matches` awaits a properties read — do it WITHOUT
                    // holding the lock (a std Mutex across .await would be unsound).
                    if !peripheral_matches(&p).await {
                        continue;
                    }
                    // Re-check + claim under the lock (another tick may have raced).
                    {
                        let mut set = connected.lock().unwrap();
                        if !set.insert(id.clone()) {
                            continue; // already claimed between the checks
                        }
                        discovered += 1;
                        eprintln!(
                            "LIQMESH desktop connecting to peer id={id} connected={} peers={:?}",
                            set.len(),
                            *set
                        );
                    }
                    // Spawn an independent transport task for this peer. It owns
                    // its own broadcast receiver (outbound fan-out) and shares
                    // the single events sender (inbound fan-in). On exit it
                    // removes its id from `connected` so a re-advertised peer
                    // reconnects.
                    let ev = events.clone();
                    let out_rx = outbound_tx.subscribe();
                    let (pid, pname) = (my_id.clone(), my_name.clone());
                    let set_handle = connected.clone();
                    tokio::spawn(async move {
                        run_peer(p, pid, pname, ev, out_rx).await;
                        set_handle.lock().unwrap().remove(&id);
                    });
                }
                if discovered > 0 {
                    let set = connected.lock().unwrap();
                    eprintln!(
                        "LIQMESH desktop discovered+={discovered} connected={} peers={:?}",
                        set.len(),
                        *set
                    );
                }
            }
        }
    }

    let _ = adapter.stop_scan().await;
}

/// Runs one already-discovered peer to completion: connect, resolve chars,
/// subscribe to RX, and drive the [`Driver`] until the link drops. Outbound
/// frames arrive over a [`broadcast::Receiver`] shared with every other peer
/// (Room Model A: one group message goes to all connected peers); they are
/// forwarded into the driver's mpsc `outbound`. A `broadcast` lag (slow peer)
/// is skipped rather than killing the link.
async fn run_peer(
    peripheral: Peripheral,
    my_id: String,
    my_name: String,
    events: mpsc::Sender<TransportEvent>,
    mut outbound_rx: broadcast::Receiver<Frame>,
) {
    let peer_id = peripheral.id().to_string();
    let (tx_char, rx_char) = match connect_and_resolve_chars(&peripheral).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("LIQMESH desktop connect FAILED id={peer_id}: {e}");
            return report_io(&events, e).await;
        }
    };
    eprintln!("LIQMESH desktop CONNECTED id={peer_id}");

    let mut notif_stream = match peripheral.notifications().await {
        Ok(s) => s,
        Err(e) => return report_io(&events, format!("notifications() failed: {e}")).await,
    };
    if let Err(e) = peripheral.subscribe(&rx_char).await {
        return report_io(&events, format!("subscribe(RX) failed: {e}")).await;
    }

    let (inbound_tx, inbound_rx) = mpsc::channel::<Vec<u8>>(CHANNEL_CAPACITY);
    let rx_uuid = rx_char.uuid;
    let notif_task = tokio::spawn(async move {
        while let Some(n) = notif_stream.next().await {
            if n.uuid == rx_uuid && inbound_tx.send(n.value).await.is_err() {
                break;
            }
        }
    });

    // Adapt the shared broadcast receiver to the driver's mpsc `outbound`.
    let (out_tx, out_rx) = mpsc::channel::<Frame>(CHANNEL_CAPACITY);
    let fanout_task = tokio::spawn(async move {
        loop {
            match outbound_rx.recv().await {
                Ok(frame) => {
                    if out_tx.send(frame).await.is_err() {
                        break; // driver gone
                    }
                }
                // Lagged behind a burst: skip the missed frames and keep going.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                // All senders dropped (supervisor shut down): end the task.
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let start = Instant::now();
    let clock = move || start.elapsed().as_millis() as u64;
    let (tick_tx, tick_rx) = mpsc::channel::<()>(CHANNEL_CAPACITY);
    let tick_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            if tick_tx.send(()).await.is_err() {
                break;
            }
        }
    });

    let session = Session::new(my_id, my_name);
    let link = BtleLink {
        peripheral: peripheral.clone(),
        tx_char,
    };
    let driver = Driver::new(session, link, clock);
    driver.run(inbound_rx, out_rx, tick_rx, events).await;

    eprintln!("LIQMESH desktop DISCONNECTED id={peer_id}");
    notif_task.abort();
    fanout_task.abort();
    tick_task.abort();
    let _ = peripheral.disconnect().await;
}

/// Connects to `peripheral`, discovers its services, and resolves the TX/RX
/// characteristics. Returns `(tx_char, rx_char)` on success.
async fn connect_and_resolve_chars(
    peripheral: &Peripheral,
) -> Result<
    (
        btleplug::api::Characteristic,
        btleplug::api::Characteristic,
    ),
    String,
> {
    peripheral
        .connect()
        .await
        .map_err(|e| format!("connect failed: {e}"))?;
    peripheral
        .discover_services()
        .await
        .map_err(|e| format!("discover_services failed: {e}"))?;

    let chars = peripheral.characteristics();
    let tx_char = chars
        .iter()
        .find(|c| c.uuid == TX_CHAR_UUID)
        .cloned()
        .ok_or_else(|| "TX characteristic (…0002) not found".to_string())?;
    let rx_char = chars
        .iter()
        .find(|c| c.uuid == RX_CHAR_UUID)
        .cloned()
        .ok_or_else(|| "RX characteristic (…0003) not found".to_string())?;
    Ok((tx_char, rx_char))
}

/// Scans, connects, and runs the BLE transport [`Driver`] for one peer until the
/// link drops or the `outbound` channel closes.
///
/// NOTE: superseded by [`connect_and_run_multi`] (Room Model A, continuous
/// multi-peer). Retained as the single-peer reference implementation; the app no
/// longer wires it.
///
/// All inputs/outputs are channels so the Tauri layer never sees btleplug types:
/// - `events`   — driver/connection events out to Tauri (`ble://…`).
/// - `outbound` — frames the local UI wants to send.
///
/// Any setup failure is reported as a [`TransportEvent::LinkError`] and the
/// function returns; it never panics.
///
/// ## Teardown guarantee
/// `shutdown` is an external stop signal: firing it (`ble_stop`, or a reconnect
/// that supersedes this link) cancels the [`Driver::run`] future via `select!`
/// and proceeds to the same teardown path as a natural driver exit — the
/// notification/tick tasks are aborted and the GATT connection is disconnected.
/// This is the only reliable way to wind a connection down, because `Driver::run`
/// holds a `tokio::time::interval` tick source that never closes on its own;
/// dropping the outbound sender alone would leave the old driver (and its
/// notif/tick tasks + GATT link) running indefinitely.
pub async fn connect_and_run(
    my_id: String,
    my_name: String,
    events: mpsc::Sender<TransportEvent>,
    outbound: mpsc::Receiver<Frame>,
    shutdown: oneshot::Receiver<()>,
) {
    // 1. Adapter.
    let manager = match Manager::new().await {
        Ok(m) => m,
        Err(e) => return report_io(&events, format!("Manager::new failed: {e}")).await,
    };
    let adapter = match manager.adapters().await {
        Ok(mut a) if !a.is_empty() => a.remove(0),
        Ok(_) => return report_io(&events, "no Bluetooth adapter found").await,
        Err(e) => return report_io(&events, format!("adapters() failed: {e}")).await,
    };

    // 2. Scan for a Contract peer.
    let peripheral = match scan_for_peer(&adapter).await {
        Ok(p) => p,
        Err(e) => return report_io(&events, e).await,
    };

    // 3. Connect + resolve characteristics.
    let (tx_char, rx_char) = match connect_and_resolve_chars(&peripheral).await {
        Ok(pair) => pair,
        Err(e) => return report_io(&events, e).await,
    };

    // 4. Subscribe to RX and pump notifications into the inbound channel.
    let mut notif_stream = match peripheral.notifications().await {
        Ok(s) => s,
        Err(e) => return report_io(&events, format!("notifications() failed: {e}")).await,
    };
    if let Err(e) = peripheral.subscribe(&rx_char).await {
        return report_io(&events, format!("subscribe(RX) failed: {e}")).await;
    }

    let (inbound_tx, inbound_rx) = mpsc::channel::<Vec<u8>>(CHANNEL_CAPACITY);
    let rx_uuid = rx_char.uuid;
    let notif_task = tokio::spawn(async move {
        while let Some(n) = notif_stream.next().await {
            // Only forward notifications from our RX characteristic; ignore any
            // unrelated ones the OS may multiplex onto the same stream.
            if n.uuid == rx_uuid && inbound_tx.send(n.value).await.is_err() {
                break; // driver gone
            }
        }
    });

    // 5. Monotonic clock + maintenance ticks.
    let start = Instant::now();
    let clock = move || start.elapsed().as_millis() as u64;

    let (tick_tx, tick_rx) = mpsc::channel::<()>(CHANNEL_CAPACITY);
    let tick_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        // Skip the immediate first tick so the very first Stats arrives one
        // interval after connect, not instantly.
        interval.tick().await;
        loop {
            interval.tick().await;
            if tick_tx.send(()).await.is_err() {
                break; // driver gone
            }
        }
    });

    // 6. Run the driver to completion.
    let session = Session::new(my_id, my_name);
    let link = BtleLink {
        peripheral: peripheral.clone(),
        tx_char,
    };
    let driver = Driver::new(session, link, clock);
    // Run the driver, but allow an external `shutdown` to cancel it: placing
    // `driver.run(..)` as a `select!` branch means firing `shutdown` drops the
    // run future (cancelling it) and falls through to the shared teardown below.
    tokio::select! {
        _ = driver.run(inbound_rx, outbound, tick_rx, events) => {}
        _ = shutdown => {}
    }

    // Either path (driver exited, or shutdown requested): tear down the helper
    // tasks and the GATT connection so nothing leaks across a reconnect/stop.
    notif_task.abort();
    tick_task.abort();
    let _ = peripheral.disconnect().await;
}
