import type { Message } from "../chat/store";

/** Small, fixed palette of quick-react emojis (kept tiny for the demo). */
const QUICK_EMOJIS = ["👍", "❤️", "😂", "🎉"] as const;

type Props = {
  message: Message;
  mine: boolean;
  /** My senderId — used to compute which reactions are mine (toggle state). */
  myId: string;
  /** Toggle a reaction on this message. Absent/disabled hides the affordance. */
  onReact?: (messageId: string, emoji: string, op: "add" | "remove") => void;
};

/** Compact wall-clock time for a message's createdAt (falls back to raw). */
function formatTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  return new Date(t).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function MessageBubble({ message, mine, myId, onReact }: Props) {
  const reactionEntries = Object.entries(message.reactions);
  // Reactions are meaningless on a tombstone; hide the affordance there too.
  const canReact = !!onReact && !message.deleted;

  /** Toggle my reaction for `emoji`: remove if I already reacted, else add. */
  function toggle(emoji: string) {
    if (!onReact) return;
    const reacted = (message.reactions[emoji] ?? []).includes(myId);
    onReact(message.id, emoji, reacted ? "remove" : "add");
  }

  return (
    <div className={`msg-row ${mine ? "msg-mine" : "msg-theirs"}`}>
      <div className="msg-bubble">
        {!mine && <div className="msg-sender">{message.senderName || "unknown"}</div>}

        {message.replyToId && (
          <div className="msg-reply" title={`reply to ${message.replyToId}`}>
            ↩ replying to {message.replyToId.slice(0, 8)}
          </div>
        )}

        {message.deleted ? (
          <div className="msg-body msg-deleted">メッセージは削除されました</div>
        ) : (
          <div className="msg-body">{message.body}</div>
        )}

        <div className="msg-meta">
          <time dateTime={message.createdAt}>{formatTime(message.createdAt)}</time>
        </div>

        {reactionEntries.length > 0 && (
          <div className="msg-reactions">
            {reactionEntries.map(([emoji, senders]) => {
              const mineReaction = senders.includes(myId);
              return (
                <button
                  key={emoji}
                  type="button"
                  className={`reaction-chip${mineReaction ? " reaction-chip-mine" : ""}`}
                  aria-pressed={mineReaction}
                  disabled={!canReact}
                  onClick={() => toggle(emoji)}
                >
                  {emoji} {senders.length}
                </button>
              );
            })}
          </div>
        )}

        {canReact && (
          <div className="msg-react-bar" aria-label="add reaction">
            {QUICK_EMOJIS.map((emoji) => (
              <button
                key={emoji}
                type="button"
                className="react-add"
                title={`react ${emoji}`}
                onClick={() => toggle(emoji)}
              >
                {emoji}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
