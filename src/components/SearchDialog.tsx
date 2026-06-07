import { useMemo, useState } from "react";
import type { Message } from "../chat/store";

type Props = {
  /** All messages across rooms to search (caller flattens). */
  messages: Message[];
  onClose: () => void;
};

/**
 * 検索 sheet (opened from the top-right 🔍). A simple case-insensitive substring
 * search over message bodies + sender names — enough to find a past message on
 * the shared screen. Tombstoned (deleted) messages are excluded.
 */
export function SearchDialog({ messages, onClose }: Props) {
  const [query, setQuery] = useState("");

  const results = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return messages
      .filter((m) => !m.deleted)
      .filter(
        (m) =>
          m.body.toLowerCase().includes(q) ||
          m.senderName.toLowerCase().includes(q),
      )
      .slice(0, 50);
  }, [query, messages]);

  return (
    <div className="sheet-overlay" role="dialog" aria-modal="true" aria-label="検索">
      <div className="sheet">
        <div className="sheet-head">
          <h2 className="sheet-title">検索</h2>
          <button className="sheet-close" aria-label="閉じる" onClick={onClose}>
            ✕
          </button>
        </div>

        <input
          className="conn-name"
          autoFocus
          placeholder="メッセージを検索"
          value={query}
          onChange={(e) => setQuery(e.currentTarget.value)}
        />

        <div className="search-results">
          {query.trim() && results.length === 0 && (
            <p className="profile-empty">該当するメッセージはありません</p>
          )}
          {results.map((m) => (
            <div key={m.id} className="search-result">
              <span className="search-sender">{m.senderName}</span>
              <span className="search-body">{m.body}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
