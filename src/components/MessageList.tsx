import { useEffect, useRef } from "react";
import type { Message } from "../chat/store";
import { MessageBubble } from "./MessageBubble";

type Props = {
  messages: Message[];
  myId: string;
};

export function MessageList({ messages, myId }: Props) {
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
        <MessageBubble key={m.id} message={m} mine={m.senderId === myId} />
      ))}
      <div ref={endRef} />
    </div>
  );
}
