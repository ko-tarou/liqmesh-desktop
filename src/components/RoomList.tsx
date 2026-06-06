import { useState } from "react";
import { normalizeRoomId } from "../chat/frames";

type Props = {
  rooms: string[];
  activeRoomId: string;
  onSelect: (roomId: string) => void;
  /** Register a new room and switch to it. */
  onAdd: (roomId: string) => void;
  /** roomId -> unread count; rooms absent/0 show no badge. */
  unreadByRoom?: Record<string, number>;
};

/**
 * Room switcher: lists known rooms, highlights the active one, shows a per-room
 * unread badge, and lets the user create/join a room by name. Adding normalizes
 * the input and switches to it (delegated to `onAdd`).
 */
export function RoomList({ rooms, activeRoomId, onSelect, onAdd, unreadByRoom }: Props) {
  const [draft, setDraft] = useState("");

  function submit() {
    const name = draft.trim();
    if (name === "") return;
    onAdd(normalizeRoomId(name));
    setDraft("");
  }

  return (
    <nav className="room-list" aria-label="rooms">
      <div className="room-list-head">Rooms</div>

      <ul className="room-list-items">
        {rooms.map((room) => {
          const active = room === activeRoomId;
          const unread = unreadByRoom?.[room] ?? 0;
          return (
            <li key={room}>
              <button
                type="button"
                className={`room-list-item${active ? " room-list-item-active" : ""}`}
                aria-current={active ? "true" : undefined}
                onClick={() => onSelect(room)}
              >
                <span className="room-list-hash" aria-hidden="true">
                  #
                </span>
                <span className="room-list-name">{room}</span>
                {unread > 0 && (
                  <span className="room-unread" aria-label={`${unread} unread`}>
                    {unread > 99 ? "99+" : unread}
                  </span>
                )}
              </button>
            </li>
          );
        })}
      </ul>

      <div className="room-list-add">
        <input
          className="room-list-input"
          placeholder="new room…"
          aria-label="new room name"
          value={draft}
          onChange={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
        />
        <button
          type="button"
          className="btn-secondary room-list-add-btn"
          onClick={submit}
          disabled={draft.trim() === ""}
        >
          Add
        </button>
      </div>
    </nav>
  );
}
