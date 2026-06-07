import type { Peer } from "../chat/store";

type Props = {
  myId: string;
  myName: string;
  onNameChange: (name: string) => void;
  /** senderId -> peer presence (display name). The 知り合い directory. */
  peers: Record<string, Peer>;
  /** Tapping a 知り合い opens the shared "general" group (Room Model A). */
  onOpenChat: () => void;
};

/** Short, human-scannable form of the device id. */
function shortId(id: string): string {
  return id ? id.slice(0, 8) : "—";
}

/**
 * The プロフィール tab (unified mobile-UI spec): the local display name (editable),
 * a read-only short device ID, and the 知り合い directory.
 *
 * 知り合い is derived from the chat store's `peers` map, which records every peer
 * seen via a hello/msg handshake (latest-name-wins). Room Model A: tapping a name
 * opens the shared "general" group, not a DM.
 */
export function ProfileTab({ myId, myName, onNameChange, peers, onOpenChat }: Props) {
  const acquaintances = Object.entries(peers)
    .filter(([id]) => id !== myId) // never list yourself
    .map(([id, p]) => ({ id, name: p.senderName }))
    .sort((a, b) => a.name.localeCompare(b.name));

  return (
    <div className="profile-tab">
      <section className="profile-section">
        <h2 className="profile-heading">表示名</h2>
        <input
          className="conn-name profile-name"
          placeholder="表示名"
          aria-label="表示名"
          value={myName}
          onChange={(e) => onNameChange(e.currentTarget.value)}
        />
      </section>

      <section className="profile-section">
        <h2 className="profile-heading">ID</h2>
        <span className="profile-id" title={myId}>
          {shortId(myId)}
        </span>
      </section>

      <section className="profile-section">
        <h2 className="profile-heading">知り合い</h2>
        {acquaintances.length === 0 ? (
          <p className="profile-empty">まだ知り合いはいません</p>
        ) : (
          <ul className="acq-list">
            {acquaintances.map((a) => (
              <li key={a.id}>
                <button className="acq-row" onClick={onOpenChat}>
                  <span className="acq-avatar" aria-hidden>
                    {a.name.slice(0, 1) || "?"}
                  </span>
                  <span className="acq-name">{a.name}</span>
                  <span className="acq-chat" aria-hidden>
                    💬
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
