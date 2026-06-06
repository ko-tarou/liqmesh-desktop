# LiqMesh BLE Interop Contract v1

> canonical — do not diverge; changes go through the architect session

目的: オンライン(Supabase)断時、Android/iOS/Windows-Desktop が直接 BLE で P2P チャットする。

## 役割

- 全クライアントは dual-role（OS が許す範囲で advertise(peripheral)＋scan/connect(central)）。
- iOS/Android = 完全な対等ピア（両役可）。**Windows Desktop = central 専用**（advertise 不可、相手へ接続して参加）。
- 接続後は GATT 接続を永続セッションとして保持（WebSocket 的・全二重）。範囲外で切断は許容。
- P2P ペア: iOS↔Android / Android↔Desktop / Desktop↔iOS すべて成立（Desktop は常に central）。
- v1 = 直接リンクのみ。マルチホップ中継は v2。

## GATT（全プラットフォーム固定）

- Service UUID:  `B1E5C0DE-1A2B-4C3D-8E9F-000000000001`
- TX (Write / WriteWithoutResponse, central→peripheral):  `B1E5C0DE-1A2B-4C3D-8E9F-000000000002`
- RX (Notify, peripheral→central):                        `B1E5C0DE-1A2B-4C3D-8E9F-000000000003`
- Advertise localName: `"LQM-"` + deviceId 先頭 4 文字
- Central は Service UUID でスキャン→接続→RX 購読→TX へ書込。

## 接続 / MTU / チャンク

- 接続時 MTU 247 を要求（失敗時 23 で動作）。
- 1 論理メッセージ(UTF-8 JSON)を分割。各パケット:
  `[msgId: 4 bytes big-endian][seq: 1 byte][total: 1 byte][payload...]`
  payload 上限 = negotiatedMTU - 3(ATT) - 6(header)。受信は msgId 単位で seq/total から再構成。total=1 は無分割。

## ペイロード（既存アプリ wire と同一 JSON。未知 type は無視＝前方互換）

- hello   `{type:"hello", senderId, senderName, protoVer:1}`   ← 接続直後に双方向で必須
- msg     `{type:"msg", id, senderId, senderName, body, createdAt, roomId, replyToId?}`
- reaction `{type:"reaction", messageId, senderId, emoji, op}`
- delete  `{type:"delete", messageId, senderId}`
- read    `{type:"read", roomId, upToMessageId, senderId}`
- (任意) backfill: 直近 N 件を hello 後に送る

## 整合 / セキュリティ

- senderId は接続単位に束縛(trust-on-first-use)＝なりすまし防止。
- 暗号化/ペアリング(標準 BLE 以上)は v1 スコープ外。

## 相互運用テスト合格条件

- 2 台ペアで hello 交換→双方向 msg→reaction/delete/read が往復→チャンク分割される長文も無損失。
