import { useState } from "react";

type Props = {
  /** Composer is only usable while connected. */
  disabled: boolean;
  onSend: (body: string) => void;
};

export function Composer({ disabled, onSend }: Props) {
  const [body, setBody] = useState("");

  const canSend = !disabled && body.trim() !== "";

  function submit() {
    if (!canSend) return;
    onSend(body.trim());
    setBody("");
  }

  return (
    <div className="composer">
      <input
        className="composer-input"
        placeholder={disabled ? "接続すると送信できます" : "メッセージを入力…"}
        aria-label="message body"
        value={body}
        disabled={disabled}
        onChange={(e) => setBody(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            submit();
          }
        }}
      />
      <button className="btn-primary composer-send" onClick={submit} disabled={!canSend}>
        Send
      </button>
    </div>
  );
}
