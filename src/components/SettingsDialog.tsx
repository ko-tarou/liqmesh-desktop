type Props = {
  myName: string;
  onNameChange: (name: string) => void;
  onClearChat: () => void;
  onClose: () => void;
};

/**
 * 設定 sheet (opened from the top-left ⚙️). Minimal parity with the mobile
 * settings: edit the display name and clear local chat history.
 */
export function SettingsDialog({ myName, onNameChange, onClearChat, onClose }: Props) {
  return (
    <div className="sheet-overlay" role="dialog" aria-modal="true" aria-label="設定">
      <div className="sheet">
        <div className="sheet-head">
          <h2 className="sheet-title">設定</h2>
          <button className="sheet-close" aria-label="閉じる" onClick={onClose}>
            ✕
          </button>
        </div>

        <label className="sheet-field">
          <span className="sheet-label">表示名</span>
          <input
            className="conn-name"
            value={myName}
            placeholder="表示名"
            onChange={(e) => onNameChange(e.currentTarget.value)}
          />
        </label>

        <button
          className="btn-secondary sheet-danger"
          onClick={() => {
            if (confirm("チャット履歴を消去しますか？")) onClearChat();
          }}
        >
          チャット履歴を消去
        </button>
      </div>
    </div>
  );
}
