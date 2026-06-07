import { useState } from "react";
import { Sparkles, FileText, ListOrdered } from "lucide-react";
import { useAI } from "../chat/useAI";
import type { Message } from "../chat/store";

type Props = {
  /** Local chat history used to ground the answer (Model A "general" room). */
  messages: Message[];
};

/** Modes of the AI tab: free-form chat, or a one-tap analysis preset. */
type AIMode = "chat" | "summary" | "triage";

/** Preset analysis prompts (disaster-mesh operator framing). The history is
 *  appended by `runPreset`; these are the instruction prefix. */
const PRESET_PROMPT: Record<Exclude<AIMode, "chat">, string> = {
  summary:
    "以下の会話を日本語で要約し、主な話題・決定事項・未対応のアクションを箇条書きで出して。",
  triage:
    "以下のメッセージを緊急度・重要度で優先順位付けし、対応すべき順に理由付きで並べて（災害時オペレーター視点）。",
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
 * The AI tab: on-device LFM over the local chat history (llama.cpp + LFM2-350M,
 * fully offline). A segmented control switches between:
 *  - チャット: free-form Q&A (type a question).
 *  - 要約 / 優先順位: one-tap analysis presets (会話要約 / トリアージ) that run a
 *    fixed prompt over the recent general-room history.
 * All three reuse the same `ai_ask` command + streaming answer area (useAI).
 */
export function AITab({ messages }: Props) {
  const { phase, downloadPercent, answer, error, download, ask } = useAI();
  const [mode, setMode] = useState<AIMode>("chat");
  const [question, setQuestion] = useState("");

  const busy = phase === "downloading" || phase === "generating";

  function submitChat() {
    const q = question.trim();
    if (!q || busy) return;
    void ask(q, buildHistory(messages));
  }

  function runPreset(m: Exclude<AIMode, "chat">) {
    if (busy) return;
    void ask(PRESET_PROMPT[m], buildHistory(messages));
  }

  return (
    <div className="ai-tab-full">
      <div className="ai-head">
        <Sparkles className="ai-icon" color="var(--brand)" aria-hidden />
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
          {/* Segmented control: チャット / 要約 / 優先順位 */}
          <div className="ai-modes" role="tablist" aria-label="AI モード">
            <button
              role="tab"
              aria-selected={mode === "chat"}
              className={`ai-mode${mode === "chat" ? " ai-mode-active" : ""}`}
              onClick={() => setMode("chat")}
            >
              <Sparkles size={16} aria-hidden /> チャット
            </button>
            <button
              role="tab"
              aria-selected={mode === "summary"}
              className={`ai-mode${mode === "summary" ? " ai-mode-active" : ""}`}
              onClick={() => setMode("summary")}
            >
              <FileText size={16} aria-hidden /> 要約
            </button>
            <button
              role="tab"
              aria-selected={mode === "triage"}
              className={`ai-mode${mode === "triage" ? " ai-mode-active" : ""}`}
              onClick={() => setMode("triage")}
            >
              <ListOrdered size={16} aria-hidden /> 優先順位
            </button>
          </div>

          <div className="ai-answer">
            {answer ? (
              <p className="ai-answer-text">{answer}</p>
            ) : (
              <p className="ai-answer-empty">
                {mode === "chat"
                  ? "質問を入力してください。"
                  : "「実行」を押すと、最近の会話を分析します。"}
              </p>
            )}
            {phase === "generating" && <span className="ai-cursor">生成中…</span>}
          </div>

          {mode === "chat" ? (
            <div className="ai-composer">
              <input
                className="conn-name ai-input"
                placeholder="例: 今の話題を要約して"
                value={question}
                disabled={busy}
                onChange={(e) => setQuestion(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") submitChat();
                }}
              />
              <button
                className="btn-primary"
                onClick={submitChat}
                disabled={busy || !question.trim()}
              >
                質問
              </button>
            </div>
          ) : (
            <div className="ai-composer">
              <button
                className="btn-primary ai-run"
                onClick={() => runPreset(mode)}
                disabled={busy}
              >
                {mode === "summary" ? "会話を要約する" : "優先順位をつける"}
              </button>
            </div>
          )}
        </>
      )}

      {error && <p className="ai-error">エラー: {error}</p>}
    </div>
  );
}
