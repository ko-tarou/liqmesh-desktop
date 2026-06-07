//! Async BLE transport actor driving the sans-IO [`Session`].
//!
//! [`Session`] is a pure state machine: it turns outgoing [`Frame`]s into TX
//! packets and folds incoming RX packets back into [`Frame`]s, but performs no
//! I/O and reads no clock. This module is the thin async layer that wires that
//! state machine to the real world over a handful of [`tokio::sync::mpsc`]
//! channels, keeping the concrete BLE I/O behind the [`GattLink`] trait so the
//! whole driver is unit-testable with a mock link and an injected clock.
//!
//! ## Channel wiring ([`Driver::run`])
//!
//! - `inbound`  — RX-notification packets received from the peer. Each is fed to
//!   [`Session::on_packet`]; completed frames are emitted as
//!   [`TransportEvent::Frame`].
//! - `outbound` — frames the local app wants to send. Each is encoded via
//!   [`Session::encode_frame`] and written to the TX characteristic through
//!   [`GattLink::write`].
//! - `ticks`    — a periodic pulse driving [`Session::evict_expired`] (reaping
//!   stale reassemblies) and a [`TransportEvent::Stats`] snapshot.
//! - `events`   — driver → upper-layer output (connect / frame / stats / error /
//!   disconnect).
//!
//! ## PR-B2b-2 wiring (concrete I/O, deferred)
//!
//! B2b-2 supplies a btleplug-backed [`GattLink`]: scan for Service
//! `…0001` → connect → subscribe to RX notify `…0003` (feeding `inbound`) →
//! write TX `…0002` as WriteWithResponse. A `tokio::time::interval` drives
//! `ticks`; a Tauri command channel feeds `outbound`; and `events` are bridged
//! to Tauri `ble://connected|frame|stats|disconnected|error` emits. The
//! injected clock becomes a monotonic [`std::time::Instant`]-derived closure.

use super::frame::Frame;
use super::session::Session;
use tokio::sync::mpsc;

/// A failure writing to the TX GATT characteristic.
///
/// [`LinkError::Io`] carries an implementation-specific message (e.g. the
/// btleplug error rendered to a string); [`LinkError::Disconnected`] marks the
/// link as gone. Kept `Clone`/`Eq` so it can travel inside [`TransportEvent`]
/// and be asserted on in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// A transient or fatal write failure, with a human-readable cause.
    Io(String),
    /// The underlying GATT link is disconnected; no further writes can succeed.
    Disconnected,
}

/// Abstraction over the TX-write side of a GATT connection.
///
/// Implemented with an `async fn` in trait (RPITIT, stable since Rust 1.75), so
/// the [`Driver`] stays generic over `L: GattLink` and needs no `dyn`
/// indirection or the `async-trait` crate. The concrete btleplug impl lands in
/// PR-B2b-2; tests use an in-memory mock.
pub trait GattLink {
    /// Writes one packet to the TX characteristic.
    ///
    /// The implementation must guarantee delivery semantics equivalent to
    /// BLE *WriteWithResponse* (the byte sequence reaches the peer or the call
    /// fails), so the driver can treat a successful return as "sent".
    fn write(
        &self,
        packet: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<(), LinkError>> + Send;
}

/// An event surfaced by the [`Driver`] to the upper layer (PR-B2b-2 / Tauri).
///
/// These map 1:1 to the `ble://…` events the desktop UI consumes.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportEvent {
    /// The driver started and successfully wrote its `hello`; the link is live.
    Connected,
    /// A received frame that passed TOFU / proto-version / normalization checks.
    Frame(Frame),
    /// A periodic snapshot of the session's robustness counters, emitted on each
    /// tick so the otherwise-invisible drops are observable.
    Stats {
        /// Malformed *known* frames dropped (see [`Session::protocol_violations`]).
        protocol_violations: u64,
        /// Frames dropped for a mismatched `senderId`
        /// (see [`Session::impersonation_rejections`]).
        impersonation_rejections: u64,
        /// `hello`s rejected for an incompatible `protoVer`
        /// (see [`Session::incompatible_proto`]).
        incompatible_proto: u64,
    },
    /// A TX write failed; the driver tears the connection down after emitting it.
    LinkError(LinkError),
    /// The driver's run loop has ended (all channels closed, or a write failed).
    Disconnected,
}

/// The async actor that owns one [`Session`] and drives it over channels.
///
/// Generic over the link `L` and an injected monotonic-ms clock `C` so it can be
/// exercised deterministically in tests (closure clock + mock link) and backed
/// by real BLE + `Instant` in production.
pub struct Driver<L: GattLink, C: Fn() -> u64 + Send> {
    session: Session,
    link: L,
    clock: C,
}

impl<L: GattLink, C: Fn() -> u64 + Send> Driver<L, C> {
    /// Creates a driver around an existing [`Session`], a [`GattLink`], and a
    /// `clock` returning monotonic milliseconds.
    pub fn new(session: Session, link: L, clock: C) -> Self {
        Driver {
            session,
            link,
            clock,
        }
    }

    /// Encodes `frame` and writes every resulting packet to the link in order.
    ///
    /// `Ok(())` once all packets are written; `Err(LinkError)` on the first
    /// write failure. An `encode_frame` error (e.g. trying to send
    /// [`Frame::Unknown`]) is treated as a caller bug and silently dropped
    /// (`Ok(())`), never as a link failure — the frame simply is not sent.
    async fn send_frame(&mut self, frame: &Frame) -> Result<(), LinkError> {
        let packets = match self.session.encode_frame(frame) {
            Ok(p) => p,
            // Unencodable frame (e.g. Unknown) or chunk-cap overflow: a local
            // bug, not a transport failure. Drop it rather than killing the link.
            Err(_) => return Ok(()),
        };
        let pkt_count = packets.len();
        for pkt in packets {
            self.link.write(pkt).await?;
        }
        // Demo aid (visible in the `tauri dev` terminal): confirm the frame's
        // packets were written to the peer's TX characteristic.
        eprintln!("[ble] wrote {pkt_count} packet(s) to TX for {frame:?}");
        Ok(())
    }

    /// Runs the transport actor until every input channel is closed or a TX
    /// write fails.
    ///
    /// On start it sends `hello`; on failure it emits [`TransportEvent::LinkError`]
    /// and returns. Otherwise it emits [`TransportEvent::Connected`] and loops
    /// over the inputs, finally emitting [`TransportEvent::Disconnected`].
    ///
    /// See the module docs for the role of each channel and the PR-B2b-2 wiring.
    pub async fn run(
        mut self,
        mut inbound: mpsc::Receiver<Vec<u8>>,
        mut outbound: mpsc::Receiver<Frame>,
        mut ticks: mpsc::Receiver<()>,
        events: mpsc::Sender<TransportEvent>,
    ) {
        // Handshake: send our hello before anything else (Contract: hello is
        // exchanged in both directions immediately after connect).
        let hello = self.session.hello_frame();
        if let Err(e) = self.send_frame(&hello).await {
            let _ = events.send(TransportEvent::LinkError(e)).await;
            return;
        }
        let _ = events.send(TransportEvent::Connected).await;

        loop {
            tokio::select! {
                // RX notification packet from the peer.
                Some(pkt) = inbound.recv() => {
                    let now = (self.clock)();
                    match self.session.on_packet(&pkt, now) {
                        Ok(Some(frame)) => {
                            // Demo aid (tauri dev terminal): a packet reassembled
                            // into a complete frame and is being delivered to the
                            // UI. If a peer's msg never reaches here but its hello
                            // does, the loss is in decode/reassembly above.
                            eprintln!("[ble] recv frame → {frame:?}");
                            let _ = events.send(TransportEvent::Frame(frame)).await;
                        }
                        // Still reassembling, or the completed payload was
                        // dropped (unknown/malformed/impersonation/incompatible).
                        Ok(None) => {}
                        // Chunk-layer error (too short, bad total, …). Dropped
                        // for now; a future PR may surface it as an event.
                        Err(_) => {}
                    }
                }
                // A frame the local app wants to send.
                Some(frame) = outbound.recv() => {
                    if let Err(e) = self.send_frame(&frame).await {
                        let _ = events.send(TransportEvent::LinkError(e)).await;
                        break;
                    }
                }
                // Periodic maintenance pulse: reap stale reassemblies and
                // surface the current counters.
                Some(()) = ticks.recv() => {
                    let now = (self.clock)();
                    self.session.evict_expired(now);
                    let _ = events.send(TransportEvent::Stats {
                        protocol_violations: self.session.protocol_violations(),
                        impersonation_rejections: self.session.impersonation_rejections(),
                        incompatible_proto: self.session.incompatible_proto(),
                    }).await;
                }
                // All input channels closed: nothing left to do.
                else => break,
            }
        }

        let _ = events.send(TransportEvent::Disconnected).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    /// A [`GattLink`] that records every written packet and always succeeds.
    #[derive(Clone, Default)]
    struct MockLink {
        writes: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl MockLink {
        fn new() -> Self {
            MockLink::default()
        }
        /// Snapshot of every packet written so far.
        fn packets(&self) -> Vec<Vec<u8>> {
            self.writes.lock().unwrap().clone()
        }
    }

    impl GattLink for MockLink {
        async fn write(&self, packet: Vec<u8>) -> Result<(), LinkError> {
            self.writes.lock().unwrap().push(packet);
            Ok(())
        }
    }

    /// A [`GattLink`] whose every write fails — exercises the error path.
    #[derive(Clone, Default)]
    struct FailingLink;

    impl GattLink for FailingLink {
        async fn write(&self, _packet: Vec<u8>) -> Result<(), LinkError> {
            Err(LinkError::Io("boom".into()))
        }
    }

    /// A test clock backed by an [`AtomicU64`] so tests can advance time
    /// deterministically without touching a real wall/monotonic clock.
    #[derive(Clone)]
    struct TestClock(Arc<AtomicU64>);
    impl TestClock {
        fn new() -> Self {
            TestClock(Arc::new(AtomicU64::new(0)))
        }
        fn closure(&self) -> impl Fn() -> u64 + Send {
            let inner = self.0.clone();
            move || inner.load(Ordering::SeqCst)
        }
    }

    fn session(id: &str, name: &str) -> Session {
        Session::new(id.to_string(), name.to_string())
    }

    /// Reassembles a packet list with a *fresh* [`Session`] and returns the
    /// single completed frame (panics if none completes).
    fn reassemble(packets: &[Vec<u8>]) -> Frame {
        let mut rx = session("peer", "Peer");
        let mut out = None;
        for p in packets {
            if let Some(f) = rx.on_packet(p, 0).expect("on_packet") {
                out = Some(f);
            }
        }
        out.expect("a frame should complete")
    }

    /// Spawns a driver with the given link/clock and returns the input senders
    /// and the event receiver, so each test wires only what it needs.
    #[allow(clippy::type_complexity)]
    fn spawn_driver<L>(
        sess: Session,
        link: L,
        clock_fn: impl Fn() -> u64 + Send + 'static,
    ) -> (
        mpsc::Sender<Vec<u8>>,
        mpsc::Sender<Frame>,
        mpsc::Sender<()>,
        mpsc::Receiver<TransportEvent>,
        tokio::task::JoinHandle<()>,
    )
    where
        L: GattLink + Send + 'static,
    {
        let (in_tx, in_rx) = mpsc::channel(16);
        let (out_tx, out_rx) = mpsc::channel(16);
        let (tick_tx, tick_rx) = mpsc::channel(16);
        let (ev_tx, ev_rx) = mpsc::channel(16);
        let driver = Driver::new(sess, link, clock_fn);
        let handle = tokio::spawn(driver.run(in_rx, out_rx, tick_rx, ev_tx));
        (in_tx, out_tx, tick_tx, ev_rx, handle)
    }

    fn a_msg(sender_id: &str, body: &str) -> Frame {
        Frame::Msg {
            id: "m1".into(),
            sender_id: sender_id.into(),
            sender_name: "X".into(),
            body: body.into(),
            created_at: 1,
            room_id: "lobby".into(),
            reply_to_id: None,
        }
    }

    #[tokio::test]
    async fn sends_hello_on_connect_and_emits_connected() {
        let link = MockLink::new();
        let clock = TestClock::new();
        let (_in_tx, _out_tx, _tick_tx, mut ev_rx, _h) =
            spawn_driver(session("me", "Me"), link.clone(), clock.closure());

        // Connected must be the first event.
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Connected));

        // The hello packets must have been written and reassemble to our hello.
        let packets = link.packets();
        assert!(!packets.is_empty(), "hello must be written on connect");
        match reassemble(&packets) {
            Frame::Hello {
                sender_id,
                proto_ver,
                ..
            } => {
                assert_eq!(sender_id, "me");
                assert_eq!(proto_ver, 1);
            }
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatches_inbound_packets_as_frame_events() {
        let link = MockLink::new();
        let clock = TestClock::new();
        let (in_tx, _out_tx, _tick_tx, mut ev_rx, _h) =
            spawn_driver(session("me", "Me"), link, clock.closure());
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Connected));

        // Encode a msg on a separate sender session and feed its packets in.
        let mut peer = session("u1", "U1");
        let msg = a_msg("u1", "hello there");
        for pkt in peer.encode_frame(&msg).expect("encode") {
            in_tx.send(pkt).await.unwrap();
        }

        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Frame(msg)));
    }

    #[tokio::test]
    async fn outbound_frames_are_encoded_and_written() {
        let link = MockLink::new();
        let clock = TestClock::new();
        let (_in_tx, out_tx, _tick_tx, mut ev_rx, _h) =
            spawn_driver(session("me", "Me"), link.clone(), clock.closure());
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Connected));

        // Snapshot the hello packet count, then send a frame out.
        let hello_count = link.packets().len();
        let reaction = Frame::Reaction {
            message_id: "m1".into(),
            sender_id: "me".into(),
            emoji: "👍".into(),
            op: "add".into(),
        };
        out_tx.send(reaction.clone()).await.unwrap();

        // Poll until the outbound packets appear (write happens off-thread).
        let outbound_pkts = loop {
            let all = link.packets();
            if all.len() > hello_count {
                break all[hello_count..].to_vec();
            }
            tokio::task::yield_now().await;
        };
        assert_eq!(reassemble(&outbound_pkts), reaction);
    }

    #[tokio::test]
    async fn impersonating_sender_is_dropped() {
        let link = MockLink::new();
        let clock = TestClock::new();
        let (in_tx, _out_tx, _tick_tx, mut ev_rx, _h) =
            spawn_driver(session("me", "Me"), link, clock.closure());
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Connected));

        // Bind the peer to u1 via hello (msg no longer binds — it may be relayed).
        // The hello itself surfaces as a Frame event; consume it first.
        let mut u1 = session("u1", "U1");
        let hello = u1.hello_frame();
        for pkt in u1.encode_frame(&hello).expect("encode") {
            in_tx.send(pkt).await.unwrap();
        }
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Frame(hello)));

        // An impostor REACTION claiming u2 over the u1 connection. TOFU stays
        // strict for non-msg frames, so this must be dropped. (A msg would be
        // accepted now — that's the relay relaxation, covered in session tests.)
        let mut u2 = session("u2", "U2");
        let spoof = Frame::Reaction {
            message_id: "m1".into(),
            sender_id: "u2".into(),
            emoji: "👍".into(),
            op: "add".into(),
        };
        for pkt in u2.encode_frame(&spoof).expect("encode") {
            in_tx.send(pkt).await.unwrap();
        }
        // ...then a legitimate u1 msg. Inbound delivery is FIFO, so asserting the
        // next Frame is the legit u1 one proves the impostor never produced a
        // Frame event.
        let m2 = a_msg("u1", "second");
        for pkt in u1.encode_frame(&m2).expect("encode") {
            in_tx.send(pkt).await.unwrap();
        }
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Frame(m2)));

        // A tick now surfaces the counters; the impostor must be recorded.
        _tick_tx.send(()).await.unwrap();
        match ev_rx.recv().await {
            Some(TransportEvent::Stats {
                impersonation_rejections,
                ..
            }) => assert!(impersonation_rejections >= 1),
            other => panic!("expected Stats, got {other:?} (impostor leaked?)"),
        }
    }

    #[tokio::test]
    async fn tick_emits_stats() {
        let link = MockLink::new();
        let clock = TestClock::new();
        let (_in_tx, _out_tx, tick_tx, mut ev_rx, _h) =
            spawn_driver(session("me", "Me"), link, clock.closure());
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Connected));

        tick_tx.send(()).await.unwrap();
        match ev_rx.recv().await {
            Some(TransportEvent::Stats {
                protocol_violations,
                impersonation_rejections,
                incompatible_proto,
            }) => {
                assert_eq!(protocol_violations, 0);
                assert_eq!(impersonation_rejections, 0);
                assert_eq!(incompatible_proto, 0);
            }
            other => panic!("expected Stats, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_error_on_outbound_emits_link_error_and_ends() {
        // FailingLink fails the hello write, so the driver reports LinkError
        // during the handshake and returns before Connected. This exercises the
        // write-failure path and the run-loop teardown together.
        let clock = TestClock::new();
        let (_in_tx, _out_tx, _tick_tx, mut ev_rx, handle) =
            spawn_driver(session("me", "Me"), FailingLink, clock.closure());

        assert_eq!(
            ev_rx.recv().await,
            Some(TransportEvent::LinkError(LinkError::Io("boom".into())))
        );
        // The run loop returns (no Connected, no Disconnected after a failed
        // handshake); the task completes.
        handle.await.expect("driver task joins");
    }

    #[tokio::test]
    async fn write_error_after_connect_breaks_loop_with_disconnected() {
        // A link that lets the hello through but fails the next write. We model
        // it with a flag so the first N writes (hello) succeed and a later
        // outbound write fails, hitting the in-loop LinkError + break path.
        #[derive(Clone)]
        struct FlakyLink {
            writes: Arc<Mutex<Vec<Vec<u8>>>>,
            fail_after: Arc<AtomicU64>,
        }
        impl GattLink for FlakyLink {
            async fn write(&self, packet: Vec<u8>) -> Result<(), LinkError> {
                let n = self.fail_after.fetch_sub(1, Ordering::SeqCst);
                if n == 0 {
                    return Err(LinkError::Disconnected);
                }
                self.writes.lock().unwrap().push(packet);
                Ok(())
            }
        }

        let link = FlakyLink {
            writes: Arc::new(Mutex::new(Vec::new())),
            // hello is a single packet → allow exactly 1 write, fail the 2nd.
            fail_after: Arc::new(AtomicU64::new(1)),
        };
        let clock = TestClock::new();
        let (_in_tx, out_tx, _tick_tx, mut ev_rx, handle) =
            spawn_driver(session("me", "Me"), link, clock.closure());

        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Connected));

        // This outbound send triggers the failing write.
        out_tx.send(a_msg("me", "will fail")).await.unwrap();

        assert_eq!(
            ev_rx.recv().await,
            Some(TransportEvent::LinkError(LinkError::Disconnected))
        );
        // After the in-loop break, the driver emits Disconnected and ends.
        assert_eq!(ev_rx.recv().await, Some(TransportEvent::Disconnected));
        handle.await.expect("driver task joins");
    }
}
