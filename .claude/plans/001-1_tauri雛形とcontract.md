# 001-1 Tauri 雛形 + BLE_CONTRACT 保存（Phase 1 実装記録）

## やったこと

1. **Tauri React-TS 雛形**（既に作業ツリーに生成済みのものを確定）
   - Tauri 2 + React 19 + TypeScript + Vite 7
   - `src-tauri/`（Rust: tauri 2, tauri-plugin-opener, serde, serde_json）
   - フロント: `src/`（App.tsx, main.tsx）, `index.html`, `vite.config.ts`, `tsconfig*.json`
   - 重い依存（llama.cpp / btleplug）は未導入（Phase 2 で追加）

2. **`.gitignore` 整備**
   - node_modules, dist, dist-ssr, target, src-tauri/target, src-tauri/gen/schemas, .DS_Store ほか
   - `git check-ignore` で node_modules / dist / src-tauri/target / target / .DS_Store が無視されることを確認

3. **`docs/BLE_CONTRACT.md` 正典保存**
   - 3 セッション共通の正典テキストを一字一句そのまま保存
   - 既存コミット `ffacfef` は英語版だったため、正典（日本語版）へ訂正してコミット

4. **計画ファイル**
   - `.claude/plans/001_全体計画.md`
   - `.claude/plans/001-1_tauri雛形とcontract.md`（本ファイル）

## ビルド結果

- `npm install`: 完了（node_modules 解決済み）
- `npm run build`（tsc && vite build）: **成功**
  - `dist/index.html` 0.47 kB / `dist/assets/index-*.js` 194.41 kB（gzip 61.13 kB）
  - 32 modules transformed, built in ~377ms
- `cargo check --manifest-path src-tauri/Cargo.toml`: **成功**
  - `Finished dev profile ... in 29.48s`（warning/error なし）

## コミット / push

- 空 repo の初期化のため main へ直接コミット → `git push origin main`
- リモート: https://github.com/ko-tarou/liqmesh-desktop.git

## Phase 2 への引き継ぎ

- BLE central 実装は `docs/BLE_CONTRACT.md` に厳密準拠（strict TDD）
- 詳細は `001_全体計画.md` の Phase 2 表を参照
