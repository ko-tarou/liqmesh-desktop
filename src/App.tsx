import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./App.css";

/** One entry in the on-screen BLE event log. */
type LogEntry = {
  time: string;
  kind: string;
  detail: string;
};

/** The `ble://…` events the Rust transport emits. */
const BLE_EVENTS = [
  "ble://connected",
  "ble://frame",
  "ble://stats",
  "ble://disconnected",
  "ble://error",
] as const;

function App() {
  const [myId, setMyId] = useState("");
  const [myName, setMyName] = useState("");
  const [body, setBody] = useState("");
  const [roomId, setRoomId] = useState("general");
  const [connecting, setConnecting] = useState(false);
  const [connected, setConnected] = useState(false);
  const [log, setLog] = useState<LogEntry[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);

  function append(kind: string, detail: string) {
    setLog((prev) => [
      ...prev,
      { time: new Date().toLocaleTimeString(), kind, detail },
    ]);
  }

  // Subscribe to every ble:// event for the component's lifetime.
  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = BLE_EVENTS.map((name) =>
      listen(name, (event) => {
        const short = name.replace("ble://", "");
        if (short === "connected") setConnected(true);
        if (short === "disconnected" || short === "error") setConnected(false);
        if (short === "connected") setConnecting(false);
        const payload =
          event.payload === null || event.payload === undefined
            ? ""
            : JSON.stringify(event.payload);
        append(short, payload);
      }),
    );
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, []);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [log]);

  async function connect() {
    setConnecting(true);
    append("action", `connect as ${myId} (${myName})`);
    try {
      await invoke("ble_start", { myId, myName });
    } catch (e) {
      setConnecting(false);
      append("error", String(e));
    }
  }

  async function send() {
    const frame = {
      type: "msg",
      id: crypto.randomUUID(),
      senderId: myId,
      senderName: myName,
      body,
      createdAt: new Date().toISOString(),
      roomId: roomId || "general",
    };
    append("action", `send msg "${body}"`);
    try {
      await invoke("ble_send", { frameJson: JSON.stringify(frame) });
      setBody("");
    } catch (e) {
      append("error", String(e));
    }
  }

  const canConnect = myId.trim() !== "" && !connecting;
  const canSend = connected && body.trim() !== "";

  return (
    <main className="container">
      <header className="header">
        <h1>LiqMesh</h1>
        <p className="tagline">
          Off-grid BLE chat · Windows central · disaster-resilient P2P
        </p>
        <span className={`status ${connected ? "online" : "offline"}`}>
          {connected ? "connected" : connecting ? "connecting…" : "offline"}
        </span>
      </header>

      <section className="panel">
        <h2>Connect</h2>
        <div className="field-row">
          <input
            placeholder="myId (device id)"
            value={myId}
            onChange={(e) => setMyId(e.currentTarget.value)}
          />
          <input
            placeholder="myName (display name)"
            value={myName}
            onChange={(e) => setMyName(e.currentTarget.value)}
          />
          <button onClick={connect} disabled={!canConnect}>
            {connecting ? "Connecting…" : "Connect"}
          </button>
        </div>
      </section>

      <section className="panel">
        <h2>Send message</h2>
        <div className="field-row">
          <input
            placeholder="roomId"
            value={roomId}
            onChange={(e) => setRoomId(e.currentTarget.value)}
          />
          <input
            placeholder="message body"
            value={body}
            onChange={(e) => setBody(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && canSend) send();
            }}
          />
          <button onClick={send} disabled={!canSend}>
            Send
          </button>
        </div>
      </section>

      <section className="panel log-panel">
        <h2>Event log</h2>
        <div className="log">
          {log.length === 0 ? (
            <p className="empty">
              No events yet. Connect to a nearby LiqMesh peer to begin.
            </p>
          ) : (
            log.map((entry, i) => (
              <div key={i} className={`log-line kind-${entry.kind}`}>
                <span className="log-time">{entry.time}</span>
                <span className="log-kind">{entry.kind}</span>
                <span className="log-detail">{entry.detail}</span>
              </div>
            ))
          )}
          <div ref={logEndRef} />
        </div>
      </section>
    </main>
  );
}

export default App;
