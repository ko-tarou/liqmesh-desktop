//! On-device LFM inference for the AI tab (P3).
//!
//! Runs LiquidAI's LFM2-350M (Q4_K_M GGUF, ~229 MB) entirely on-device via
//! llama.cpp (Metal backend on Apple Silicon). The model is downloaded once on
//! first use into the app cache dir; subsequent runs load it from disk. There is
//! **no network at inference time** — fitting LiqMesh's offline-first story.
//!
//! Three Tauri commands drive the UI:
//!  - [`ai_status`]  — is the model already downloaded?
//!  - [`ai_download`] — stream the GGUF, emitting `ai://download` progress.
//!  - [`ai_ask`]      — answer a question over the local chat history, emitting
//!                      `ai://token` per generated piece and `ai://done` at end.
//!
//! The heavy model handle is **not** held in global state (a single 350M model
//! reload per question is fast enough for a demo and keeps the lifetime simple);
//! if latency matters later, cache the `LlamaModel` behind a `Mutex<Option<…>>`.

use std::num::NonZeroU32;
use std::path::PathBuf;

use futures::StreamExt;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// HuggingFace direct-download URL for the LFM2-350M Q4_K_M GGUF (~229 MB).
const MODEL_URL: &str =
    "https://huggingface.co/LiquidAI/LFM2-350M-GGUF/resolve/main/LFM2-350M-Q4_K_M.gguf";
/// On-disk filename under the app cache dir.
const MODEL_FILE: &str = "LFM2-350M-Q4_K_M.gguf";
/// Context window for the demo. 350M handles this comfortably on-device.
const N_CTX: u32 = 4096;
/// Hard cap on generated tokens so a runaway never hangs the UI.
const MAX_TOKENS: usize = 512;

/// Resolved path to the model file under the app cache dir (created if missing).
fn model_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("no app cache dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir cache: {e}"))?;
    Ok(dir.join(MODEL_FILE))
}

/// Whether the GGUF is already on disk (so the UI can skip the download prompt).
#[tauri::command]
pub async fn ai_status(app: AppHandle) -> Result<bool, String> {
    Ok(model_path(&app)?.exists())
}

/// Progress payload for `ai://download` (camelCase to match the JS side).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    received: u64,
    total: u64,
    /// 0..=100, or -1 when the server did not report a content length.
    percent: i32,
}

/// Downloads the GGUF to the cache dir, streaming `ai://download` progress.
///
/// Idempotent: returns immediately if the file already exists. Writes to a
/// `.part` file and renames on success so a cancelled/failed download never
/// leaves a truncated model that would crash the loader.
#[tauri::command]
pub async fn ai_download(app: AppHandle) -> Result<(), String> {
    let dest = model_path(&app)?;
    if dest.exists() {
        let _ = app.emit("ai://download", DownloadProgress { received: 1, total: 1, percent: 100 });
        return Ok(());
    }
    let part = dest.with_extension("part");

    let resp = reqwest::get(MODEL_URL)
        .await
        .map_err(|e| format!("download request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&part).map_err(|e| format!("create .part: {e}"))?;
    let mut received: u64 = 0;
    let mut stream = resp.bytes_stream();
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download stream error: {e}"))?;
        use std::io::Write;
        file.write_all(&chunk).map_err(|e| format!("write .part: {e}"))?;
        received += chunk.len() as u64;
        // Throttle events to ~every 2 MB so we don't flood the webview.
        if received - last_emit >= 2_000_000 || (total > 0 && received >= total) {
            last_emit = received;
            let percent = if total > 0 { ((received * 100) / total) as i32 } else { -1 };
            let _ = app.emit("ai://download", DownloadProgress { received, total, percent });
        }
    }
    drop(file);
    std::fs::rename(&part, &dest).map_err(|e| format!("finalize model: {e}"))?;
    let _ = app.emit("ai://download", DownloadProgress { received, total, percent: 100 });
    Ok(())
}

/// Answers `question` grounded in `history` (the local chat transcript), emitting
/// each generated piece over `ai://token` and `ai://done` (with the full answer)
/// at the end. Runs on a blocking thread because llama.cpp inference is CPU/GPU
/// heavy and synchronous.
#[tauri::command]
pub async fn ai_ask(app: AppHandle, question: String, history: String) -> Result<(), String> {
    let path = model_path(&app)?;
    if !path.exists() {
        return Err("model not downloaded".into());
    }

    // llama.cpp work is blocking; keep it off the async runtime's worker.
    tauri::async_runtime::spawn_blocking(move || generate(&app, &path, &question, &history))
        .await
        .map_err(|e| format!("inference task panicked: {e}"))?
}

/// Blocking inference: load model → build prompt → greedy-decode a streamed
/// answer. Emits `ai://token` per piece and `ai://done` at the end.
///
/// `token_to_str` / `Special` are deprecated in favour of the `token_to_piece`
/// + `encoding_rs::Decoder` API, but the simple form is correct for ASCII/UTF-8
/// pieces and keeps the demo path small; allow the deprecation locally.
#[allow(deprecated)]
fn generate(app: &AppHandle, path: &PathBuf, question: &str, history: &str) -> Result<(), String> {
    let backend = LlamaBackend::init().map_err(|e| format!("backend init: {e}"))?;

    // Offload all layers to Metal on Apple Silicon (no-op where unavailable).
    let model_params = Box::pin(LlamaModelParams::default().with_n_gpu_layers(1000));
    let model = LlamaModel::load_from_file(&backend, path, &model_params)
        .map_err(|e| format!("model load: {e}"))?;

    let ctx_params =
        LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
    let mut ctx = model
        .new_context(&backend, ctx_params)
        .map_err(|e| format!("context: {e}"))?;

    // A compact instruction prompt: ground the answer in the local history.
    let prompt = format!(
        "あなたは LiqMesh のオフライン AI アシスタントです。以下の近くのチャット履歴をふまえて、\
         ユーザーの質問に日本語で簡潔に答えてください。\n\n\
         === チャット履歴 ===\n{history}\n=== 質問 ===\n{question}\n=== 回答 ===\n"
    );

    let tokens = model
        .str_to_token(&prompt, AddBos::Always)
        .map_err(|e| format!("tokenize: {e}"))?;

    let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
    let last = tokens.len() as i32 - 1;
    for (i, tok) in tokens.iter().enumerate() {
        // Only the final prompt token needs logits (that's where generation starts).
        batch
            .add(*tok, i as i32, &[0], i as i32 == last)
            .map_err(|e| format!("batch add: {e}"))?;
    }
    ctx.decode(&mut batch).map_err(|e| format!("decode prompt: {e}"))?;

    let mut sampler = LlamaSampler::greedy();
    let mut n_cur = batch.n_tokens();
    let mut answer = String::new();

    for _ in 0..MAX_TOKENS {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }
        let piece = model
            .token_to_str(token, Special::Tokenize)
            .unwrap_or_default();
        answer.push_str(&piece);
        let _ = app.emit("ai://token", piece);

        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|e| format!("batch add gen: {e}"))?;
        n_cur += 1;
        ctx.decode(&mut batch).map_err(|e| format!("decode gen: {e}"))?;
    }

    let _ = app.emit("ai://done", answer);
    Ok(())
}
