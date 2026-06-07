import type { BleStatus } from "../chat/useBle";

type Props = {
  myId: string;
  myName: string;
  status: BleStatus;
  /** Number of distinct peers connected (Room Model A group). */
  peerCount: number;
  onNameChange: (name: string) => void;
  onConnect: () => void;
  onDisconnect: () => void;
};

const STATUS_LABEL: Record<BleStatus, string> = {
  offline: "オフライン",
  // Room Model A: we scan continuously, so "connecting" reads as "searching".
  connecting: "検索中…",
  connected: "接続中",
};

/** Short, human-scannable form of the device id. */
function shortId(id: string): string {
  return id ? id.slice(0, 8) : "—";
}

export function ConnectionBar({
  myId,
  myName,
  status,
  peerCount,
  onNameChange,
  onConnect,
  onDisconnect,
}: Props) {
  const isOffline = status === "offline";

  return (
    <header className="conn-bar">
      <div className="conn-brand">
        <h1>LiqMesh</h1>
        <span className="conn-id" title={myId}>
          id {shortId(myId)}
        </span>
      </div>

      <div className="conn-controls">
        <input
          className="conn-name"
          placeholder="display name"
          aria-label="display name"
          value={myName}
          onChange={(e) => onNameChange(e.currentTarget.value)}
        />
        {/* Room Model A auto-scans on launch; expose a manual re-scan only when
            offline (e.g. the supervisor errored), and a stop otherwise. */}
        {isOffline ? (
          <button className="btn-primary" onClick={onConnect}>
            再スキャン
          </button>
        ) : (
          <button className="btn-secondary" onClick={onDisconnect}>
            停止
          </button>
        )}
        <span className={`conn-status status-${status}`}>
          <span className="status-dot" aria-hidden="true" />
          {STATUS_LABEL[status]}
          {peerCount > 0 && <span className="conn-peer"> · {peerCount}人接続中</span>}
        </span>
      </div>
    </header>
  );
}
