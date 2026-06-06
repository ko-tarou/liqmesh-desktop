import type { BleStatus } from "../chat/useBle";

type Props = {
  myId: string;
  myName: string;
  status: BleStatus;
  onNameChange: (name: string) => void;
  onConnect: () => void;
  onDisconnect: () => void;
};

const STATUS_LABEL: Record<BleStatus, string> = {
  offline: "offline",
  connecting: "connecting…",
  connected: "connected",
};

/** Short, human-scannable form of the device id. */
function shortId(id: string): string {
  return id ? id.slice(0, 8) : "—";
}

export function ConnectionBar({
  myId,
  myName,
  status,
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
        {isOffline ? (
          <button className="btn-primary" onClick={onConnect}>
            Connect
          </button>
        ) : (
          <button className="btn-secondary" onClick={onDisconnect}>
            Disconnect
          </button>
        )}
        <span className={`conn-status status-${status}`}>
          <span className="status-dot" aria-hidden="true" />
          {STATUS_LABEL[status]}
        </span>
      </div>
    </header>
  );
}
