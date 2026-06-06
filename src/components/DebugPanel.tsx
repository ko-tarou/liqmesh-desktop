import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BleStats } from "../chat/useBle";

type Props = {
  myId: string;
  roomId: string;
  connected: boolean;
  stats: BleStats | null;
  error: string | null;
};

type RawKind = "reaction" | "delete" | "read";

/**
 * Collapsible interop console: lets a tester send raw reaction/delete/read
 * frames over the wire so cross-platform behaviour can be exercised without a
 * dedicated UI for every frame type (those land in C2). Kept out of the way.
 */
export function DebugPanel({ myId, roomId, connected, stats, error }: Props) {
  const [open, setOpen] = useState(false);
  const [raw, setRaw] = useState("");

  function template(kind: RawKind) {
    const tmpl =
      kind === "reaction"
        ? { type: "reaction", messageId: "", senderId: myId, emoji: "👍", op: "add" }
        : kind === "delete"
          ? { type: "delete", messageId: "", senderId: myId }
          : { type: "read", roomId, upToMessageId: "", senderId: myId };
    setRaw(JSON.stringify(tmpl, null, 2));
  }

  async function sendRaw() {
    try {
      await invoke("ble_send", { frameJson: raw });
    } catch (e) {
      setRaw((prev) => `// send failed: ${String(e)}\n${prev}`);
    }
  }

  return (
    <details className="debug" open={open} onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>Debug · interop console</summary>

      <div className="debug-stats">
        {error && <span className="debug-error">error: {error}</span>}
        {stats && (
          <span className="debug-counters">
            violations {stats.protocolViolations} · impersonation{" "}
            {stats.impersonationRejections} · proto {stats.incompatibleProto}
          </span>
        )}
      </div>

      <div className="debug-templates">
        <button className="btn-secondary" onClick={() => template("reaction")}>
          Reaction
        </button>
        <button className="btn-secondary" onClick={() => template("delete")}>
          Delete
        </button>
        <button className="btn-secondary" onClick={() => template("read")}>
          Read
        </button>
      </div>

      <textarea
        className="debug-raw"
        rows={6}
        placeholder='{"type":"reaction","messageId":"…","senderId":"…","emoji":"👍","op":"add"}'
        value={raw}
        onChange={(e) => setRaw(e.currentTarget.value)}
      />

      <button
        className="btn-primary"
        onClick={sendRaw}
        disabled={!connected || raw.trim() === ""}
      >
        Send raw frame
      </button>
    </details>
  );
}
