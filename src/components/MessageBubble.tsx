import type { Message } from "../chat/store";

type Props = {
  message: Message;
  mine: boolean;
};

/** Compact wall-clock time for a message's createdAt (falls back to raw). */
function formatTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  return new Date(t).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function MessageBubble({ message, mine }: Props) {
  const reactionEntries = Object.entries(message.reactions);

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
            {reactionEntries.map(([emoji, senders]) => (
              <span key={emoji} className="reaction-chip">
                {emoji} {senders.length}
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
