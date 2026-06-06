import { useState } from "react";
import { normalizeRoomId } from "../chat/frames";

type Props = {
  rooms: string[];
  activeRoomId: string;
  onSelect: (roomId: string) => void;
  /** Register a new room and switch to it. */
  onAdd: (roomId: string) => void;
};

/**
 * Room switcher: lists known rooms, highlights the active one, and lets the
 * user create/join a room by name. Adding normalizes the input and switches to
 * it (delegated to `onAdd`). Unread counts are intentionally omitted in C2 —
 * see the PR handoff (precise unread tracking lands in C3).
 */
export function RoomList({ rooms, activeRoomId, onSelect, onAdd }: Props) {
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
