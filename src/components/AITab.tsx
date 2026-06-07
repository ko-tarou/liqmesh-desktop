/**
 * The AI tab (unified mobile-UI spec): on-device LFM Q&A over local chat history.
 *
 * Placeholder until P3 wires the on-device model (llama-cpp-2 + LFM2.5-350M). The
 * tab exists now so the 3-tab scaffold matches the phones; the run screen
 * (streamed tokens, 生成中…/DL中 NN%) lands in P3.
 */
export function AITab() {
  return (
    <div className="ai-tab">
      <div className="ai-placeholder">
        <span className="ai-icon" aria-hidden>
          ✨
        </span>
        <p className="ai-title">オンデバイス AI</p>
        <p className="ai-sub">
          端末内の LFM がチャット履歴に答えます。準備中です。
        </p>
      </div>
    </div>
  );
}
