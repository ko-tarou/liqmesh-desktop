import { useEffect, useRef } from "react";
import type { Message } from "../chat/store";
import { MessageBubble } from "./MessageBubble";

type Props = {
  messages: Message[];
  myId: string;
  /** Toggle a reaction on a message (omitted/undefined disables reacting). */
  onReact?: (messageId: string, emoji: string, op: "add" | "remove") => void;
  /** Delete one of my own messages (omitted/undefined disables deleting). */
  onDelete?: (messageId: string) => void;
  /** Id of my latest message the peer has read; renders a "seen" marker under it. */
  seenMessageId?: string;
};

export function MessageList({ messages, myId, onReact, onDelete, seenMessageId }: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  // Keep the newest message in view as the conversation grows.
  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  if (messages.length === 0) {
    return (
      <div className="msg-list msg-list-empty">
        <p className="empty-title">まだメッセージはありません</p>
        <p className="empty-sub">
          近くの LiqMesh ピアに接続して、最初のメッセージを送りましょう。
        </p>
      </div>
    );
  }

  return (
    <div className="msg-list">
      {messages.map((m) => (
        <MessageBubble
          key={m.id}
          message={m}
          mine={m.senderId === myId}
          myId={myId}
          seen={m.id === seenMessageId}
          onReact={onReact}
          onDelete={onDelete}
        />
      ))}
      <div ref={endRef} />
    </div>
  );
}
