/**
 * Frontend bridge to the on-device LFM (P3).
 *
 * Wraps the `ai_*` Tauri commands and the `ai://download | token | done` events
 * into a small state machine for the AI tab: check/download the model, then ask
 * a question and watch the answer stream in token-by-token.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AIPhase = "checking" | "needs-download" | "downloading" | "ready" | "generating";

type DownloadProgress = { received: number; total: number; percent: number };

export type UseAI = {
  phase: AIPhase;
  /** 0..100 while downloading, or -1 if the size is unknown. */
  downloadPercent: number;
  /** The answer text accumulated so far (streams during "generating"). */
  answer: string;
  error: string | null;
  download: () => Promise<void>;
  ask: (question: string, history: string) => Promise<void>;
};

export function useAI(): UseAI {
  const [phase, setPhase] = useState<AIPhase>("checking");
  const [downloadPercent, setDownloadPercent] = useState(0);
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState<string | null>(null);
  // Accumulate streamed tokens in a ref, mirror to state, so rapid token events
  // don't drop characters between renders.
  const answerRef = useRef("");

  // Probe whether the model is already on disk at mount.
  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const have = await invoke<boolean>("ai_status");
        if (alive) setPhase(have ? "ready" : "needs-download");
      } catch (e) {
        if (alive) {
          setPhase("needs-download");
          setError(String(e));
        }
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  // Subscribe to streaming events for the hook's lifetime.
  useEffect(() => {
    const subs: Promise<UnlistenFn>[] = [
      listen<DownloadProgress>("ai://download", (e) => {
        setDownloadPercent(e.payload.percent);
        if (e.payload.percent >= 100) setPhase("ready");
      }),
      listen<string>("ai://token", (e) => {
        answerRef.current += e.payload;
        setAnswer(answerRef.current);
      }),
      listen<string>("ai://done", (e) => {
        answerRef.current = e.payload;
        setAnswer(e.payload);
        setPhase("ready");
      }),
    ];
    return () => {
      subs.forEach((p) => p.then((un) => un()));
    };
  }, []);

  const download = useCallback(async () => {
    setError(null);
    setPhase("downloading");
    setDownloadPercent(0);
    try {
      await invoke("ai_download");
      setPhase("ready");
    } catch (e) {
      setError(String(e));
      setPhase("needs-download");
    }
  }, []);

  const ask = useCallback(async (question: string, history: string) => {
    setError(null);
    answerRef.current = "";
    setAnswer("");
    setPhase("generating");
    try {
      await invoke("ai_ask", { question, history });
      setPhase("ready");
    } catch (e) {
      setError(String(e));
      setPhase("ready");
    }
  }, []);

  return { phase, downloadPercent, answer, error, download, ask };
}
