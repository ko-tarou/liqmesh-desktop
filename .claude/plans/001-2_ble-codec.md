# 001-2 PR-A: BLE codec/chunk (strict TDD, pure logic)

ブランチ: `feature/ble-codec`（main から分岐 → main 向け PR）
正典: `docs/BLE_CONTRACT.md`（実装中に v1 → **v1.1** へ司令塔が更新。下記「並行更新の記録」参照）

## スコープ

実機 / btleplug を一切使わない純ロジックのみ。`src-tauri/src/ble/` に2モジュール:

- `chunk.rs` — パケット分割 / 再構成（`[msgId:4 BE][seq:1][total:1][payload]`）
- `frame.rs` — JSON フレーム enum（hello/msg/reaction/delete/read + Unknown）

`src-tauri/src/lib.rs` に `mod ble;` を追加してビルドに含める。

## strict TDD の進め方（実績）

1. `chunk` テストを先に書き red（12 fail）→ commit
2. `chunk` 実装 green（13 pass）→ commit
3. `frame` テストを先に書き red（コンパイル不能 = field 未定義）→ commit
4. `frame` 実装 green（13 pass）→ commit

合計 26 tests pass / 0 warning（`cargo check`）。

## 採用した API シグネチャ（最終形）

### chunk.rs
```rust
pub const HEADER_LEN: usize = 6;   // msgId(4)+seq(1)+total(1)
pub const ATT_OVERHEAD: usize = 3;
pub const MAX_CHUNKS: usize = 255; // total は 1 byte
pub const MAX_CONCURRENT_REASSEMBLIES: usize = 64; // 同時再構成上限

pub fn payload_limit(mtu: usize) -> usize; // mtu - 3 - 6, 下回ると 0 に飽和
pub fn split(msg_id: u32, payload: &[u8], max_payload: usize)
    -> Result<Vec<Vec<u8>>, ChunkError>;

pub struct Reassembler { /* msgId 単位の部分状態 */ }
impl Reassembler {
    pub fn new() -> Self;
    pub fn push(&mut self, packet: &[u8]) -> Result<Option<Vec<u8>>, ChunkError>;
}

pub enum ChunkError {
    PacketTooShort, InvalidTotal, SeqOutOfRange, TotalMismatch, TooManyChunks,
    TooManyConcurrent,
}
```

挙動メモ:
- 空 payload も 1 チャンク（total=1）。`max_payload == 0` は呼び出し側バグとして
  `Err(TooManyChunks)`（panic しない・空 payload でも同じエラー経路に統一）。
- `Reassembler` は順不同受信・複数 msgId 並行・重複 seq（冪等）に対応。
- 同時再構成数は `MAX_CONCURRENT_REASSEMBLIES = 64` で上限。到達後の **新規 msgId** は
  `Err(TooManyConcurrent)`、既存 msgId への追記は成功。timeout / eviction は
  接続セッション境界を持つ **PR-B** で実装予定。
- 完成時に該当 msgId の部分状態を破棄して `Some(payload)` を返す。

### frame.rs
```rust
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Frame {
    Hello   { sender_id, sender_name, proto_ver },
    Msg     { id, sender_id, sender_name, body, created_at, room_id,
              reply_to_id: Option<String> /* skip_serializing_if None */ },
    Reaction{ message_id, sender_id, emoji, op },
    Delete  { message_id, sender_id },
    Read    { room_id, up_to_message_id, sender_id },
    Unknown, // #[serde(skip)]
}
impl Frame {
    pub fn encode(&self) -> Vec<u8>;            // serde_json::to_vec
    pub fn decode(bytes: &[u8]) -> Option<Frame>; // 不正JSON=None / 未知type=Unknown
}
```

- wire キーは camelCase（enum レベル + 各 variant に `rename_all`）。
  → enum レベルの `rename_all` は **variant 名（tag 値）にしか効かない**ため、
    各 struct variant にも `#[serde(rename_all = "camelCase")]` が必須だった（red で発覚）。
- 未知 type / type 欠落 → `Frame::Unknown`（panic しない＝前方互換）。
  実装は「まず Value にパース（不正JSON は None）→ tagged enum へ変換、失敗時 Unknown」。

## ✅ INTEROP（RESOLVED: 0-based, confirmed by architect, matches iOS/Android）

> **seq / total は 0 始まりで確定。** `seq ∈ 0..total` / `total = チャンク総数` /
> `total == 1 は無分割`。architect セッションで確定し、既存 iOS/Android wire と
> 突き合わせ済み — **3 者すべて一致、いずれの側もコード変更不要**。

- 採用根拠: `total` を「総数」とした方が `received == total` の完成判定が自然。多くの BLE チャンク実装も 0 始まり。
- doc コメント（`chunk.rs` 冒頭）にも RESOLVED として反映済み。
- GitHub issue は作成しない（司令塔確認で確定済みのため不要）。

## 並行更新の記録（作業中に発生）

- 実装途中、**同一ワーキングディレクトリ上で司令塔セッションが `git checkout main` → `docs/BLE_CONTRACT.md` を v1.1 へ amend commit（37b2470）** した。
  これにより一時的に HEAD が main に移り、本ブランチの未コミット green 実装がディスク上に取り残された。
- 対応: `feature/ble-codec` に復帰 → `git rebase main` で v1.1 を取り込み → green 実装を再コミット。作業ロスなし。
- v1.1 の追加（"Transport semantics": transport は全 frame 種別を運ぶ）は **chunk wire / JSON payload 形状を変えない**ため、本 PR の実装はそのまま v1.1 準拠。
  むしろ「transport は msg だけでなく全 frame を運ぶ」方針は、本 PR が hello/reaction/delete/read を含む完全な `Frame` enum を提供していることと整合的。
- 教訓: 共有作業ディレクトリで並行セッションが checkout すると衝突する。worktree isolation 推奨（CLAUDE.md のルール通り）。

## 実機テスト

- 実機 / btleplug 結線は **PR-B（transport 層）以降**。本 PR は純ロジックのため、
  相互運用の最終確認はオーナー（iOS/Android 実機保有者）に Phase PR-B 完了後に依頼する。
