import { useState } from "react";
import AutoAwesomeIcon from "@mui/icons-material/AutoAwesome";
import { useAI } from "../chat/useAI";
import type { Message } from "../chat/store";

type Props = {
  /** Local chat history used to ground the answer (Model A "general" room). */
  messages: Message[];
};

/** Flatten recent chat into a compact transcript the model can read. */
function buildHistory(messages: Message[]): string {
  return messages
    .filter((m) => !m.deleted)
    .slice(-40) // keep the prompt small on a 350M model
    .map((m) => `${m.senderName}: ${m.body}`)
    .join("\n");
}

/**
 * The AI tab (P3): on-device LFM Q&A over the local chat history via llama.cpp +
 * LFM2-350M. Downloads the GGUF on first use (with a live %), then streams the
 * answer token-by-token. Always-on, fully offline at inference time.
 */
export function AITab({ messages }: Props) {
  const { phase, downloadPercent, answer, error, download, ask } = useAI();
  const [question, setQuestion] = useState("");

  const busy = phase === "downloading" || phase === "generating";

  function submit() {
    const q = question.trim();
    if (!q || busy) return;
    void ask(q, buildHistory(messages));
  }

  return (
    <div className="ai-tab-full">
      <div className="ai-head">
        <AutoAwesomeIcon className="ai-icon" color="primary" aria-hidden />
        <div>
          <p className="ai-title">オンデバイス AI</p>
          <p className="ai-sub">端末内の LFM2-350M がチャット履歴に答えます（完全オフライン）。</p>
        </div>
      </div>

      {phase === "checking" && <p className="ai-status-line">モデルを確認中…</p>}

      {phase === "needs-download" && (
        <div className="ai-download">
          <p className="ai-status-line">初回のみモデル (約229MB) をダウンロードします。</p>
          <button className="btn-primary" onClick={() => void download()}>
            モデルをダウンロード
          </button>
        </div>
      )}

      {phase === "downloading" && (
        <div className="ai-download">
          <p className="ai-status-line">
            DL中 {downloadPercent >= 0 ? `${downloadPercent}%` : "…"}
          </p>
          <div className="ai-progress">
            <div
              className="ai-progress-fill"
              style={{ width: `${downloadPercent >= 0 ? downloadPercent : 30}%` }}
            />
          </div>
        </div>
      )}

      {(phase === "ready" || phase === "generating") && (
        <>
          <div className="ai-answer">
            {answer ? (
              <p className="ai-answer-text">{answer}</p>
            ) : (
              <p className="ai-answer-empty">質問を入力してください。</p>
            )}
            {phase === "generating" && <span className="ai-cursor">生成中…</span>}
          </div>

          <div className="ai-composer">
            <input
              className="conn-name ai-input"
              placeholder="例: 今の話題を要約して"
              value={question}
              disabled={busy}
              onChange={(e) => setQuestion(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") submit();
              }}
            />
            <button className="btn-primary" onClick={submit} disabled={busy || !question.trim()}>
              質問
            </button>
          </div>
        </>
      )}

      {error && <p className="ai-error">エラー: {error}</p>}
    </div>
  );
}
