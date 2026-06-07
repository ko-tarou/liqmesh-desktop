/**
 * One-time demo seed for the general room.
 *
 * On first launch, if the general room has NO messages, seed a realistic
 * disaster-mesh conversation so the app looks in-use and the AI 要約/優先順位
 * demos have material. Idempotent: guarded by both an emptiness check AND a
 * persisted `SEED_FLAG`, so it never duplicates or overwrites real messages
 * (e.g. it won't re-run after the user clears chat, and won't fire once real
 * BLE messages exist).
 *
 * Messages are inserted via the store's `addLocalMessage` (same path as a local
 * send), so they sort by `createdAt` (epoch ms) and dedupe by id like any other.
 */

import { DEFAULT_ROOM_ID } from "./frames";
import type { Message } from "./store";

/** localStorage flag so the seed runs at most once per install. */
const SEED_FLAG = "liqmesh-demo-seeded";

type Seed = { sender: string; body: string; minsAgo: number };

/** Senders (5-6 distinct), stable ids derived from the name. */
function senderId(name: string): string {
  return `seed-${name}`;
}

/**
 * The scripted conversation: 安否確認 / 避難所 / 物資不足 / 救助要請(高緊急) /
 * 危険箇所 / 連絡・雑談. `minsAgo` spreads timestamps over the last ~4 hours,
 * roughly ascending so the transcript reads in order.
 */
const SCRIPT: Seed[] = [
  { sender: "田中", body: "地震大きかったですね…みなさん無事ですか？安否確認お願いします。", minsAgo: 240 },
  { sender: "佐藤", body: "佐藤家、全員無事です。家屋に少しヒビが入った程度。", minsAgo: 236 },
  { sender: "山本", body: "こちらも無事。ただ停電と断水が続いています。", minsAgo: 233 },
  { sender: "避難所A", body: "市民体育館を避難所として開設しました。現在約120名を受け入れています。", minsAgo: 228 },
  { sender: "田中", body: "避難所Aさん、まだ受け入れ余裕ありますか？", minsAgo: 224 },
  { sender: "避難所A", body: "あと80名ほどは可能です。毛布が足りていません。提供できる方いますか？", minsAgo: 221 },
  { sender: "消防団", body: "【緊急】西3丁目で高齢者が倒れています。意識はあるが動けない。救助に向かえる人いますか？", minsAgo: 210 },
  { sender: "山本", body: "消防団さん、私が西3丁目近くにいます。すぐ向かいます。", minsAgo: 208 },
  { sender: "消防団", body: "助かります。場所は西3丁目の青い屋根の家の前です。", minsAgo: 207 },
  { sender: "佐藤", body: "北側の橋、ひび割れていて通行は危険です。迂回してください。", minsAgo: 200 },
  { sender: "避難所A", body: "水が残り少なくなってきました。飲料水の支援を求めます。", minsAgo: 192 },
  { sender: "田中", body: "コンビニ前に給水車が来るという情報あり。未確認ですが共有します。", minsAgo: 188 },
  { sender: "山本", body: "西3丁目の高齢者、無事保護しました。避難所Aへ搬送します。", minsAgo: 181 },
  { sender: "避難所A", body: "山本さんありがとうございます。受け入れ準備します。", minsAgo: 180 },
  { sender: "消防団", body: "【緊急】東2丁目で建物倒壊、中に人がいる可能性。救助要請します。", minsAgo: 165 },
  { sender: "消防団", body: "東2丁目、消防団3名で向かいます。重機が必要かもしれません。", minsAgo: 163 },
  { sender: "佐藤", body: "うちに発電機があります。必要な避難所へ貸し出せます。", minsAgo: 150 },
  { sender: "避難所A", body: "佐藤さん、ぜひお願いします。照明用に使いたいです。", minsAgo: 148 },
  { sender: "田中", body: "子ども用のミルクとおむつが不足しています。お持ちの方いますか？", minsAgo: 140 },
  { sender: "山本", body: "おむつ少しなら提供できます。後で避難所Aに持っていきます。", minsAgo: 137 },
  { sender: "消防団", body: "東2丁目、1名救出。軽傷で意識あり。引き続き捜索中。", minsAgo: 120 },
  { sender: "避難所A", body: "夜間に向けて毛布がやはり足りません。30枚ほど不足しています。", minsAgo: 95 },
  { sender: "佐藤", body: "南倉庫に毛布の備蓄があるはず。鍵を持っている人を探しています。", minsAgo: 88 },
  { sender: "田中", body: "南倉庫の鍵は町内会長が持っています。連絡を試みます。", minsAgo: 84 },
  { sender: "山本", body: "余震が続いています。みなさん頭上に落下物がないか確認を。", minsAgo: 60 },
  { sender: "避難所A", body: "現在の受け入れ約160名。けが人5名は応急処置済み。医療支援を希望します。", minsAgo: 45 },
  { sender: "消防団", body: "東2丁目の捜索、本日は一旦終了。明朝再開します。みなさんお疲れさまでした。", minsAgo: 20 },
];

/**
 * The 27 demo messages stamped relative to `now` (epoch ms) — UNGATED. Used by
 * the on-demand "デモデータを投入" button in 設定 (the owner's general is rarely
 * empty after earlier runs, so the auto-seed won't fire). Stable ids mean
 * inserting twice is deduped by the store rather than duplicated.
 */
export function demoSeedMessages(now: number): Message[] {
  return SCRIPT.map((s, i) => ({
    id: `seed-${i}`,
    senderId: senderId(s.sender),
    senderName: s.sender,
    body: s.body,
    createdAt: now - s.minsAgo * 60_000,
    roomId: DEFAULT_ROOM_ID,
    deleted: false,
    reactions: {},
  }));
}

/**
 * Returns the seed messages stamped relative to `now` (epoch ms), or `[]` if the
 * seed has already run / should not run. Side-effect: sets the persisted flag.
 *
 * `roomHasMessages` lets the caller pass the current general-room length so we
 * never seed over real content. (Auto-seed path — first launch only.)
 */
export function buildDemoSeed(now: number, roomHasMessages: boolean): Message[] {
  if (roomHasMessages) return [];
  if (localStorage.getItem(SEED_FLAG)) return [];
  localStorage.setItem(SEED_FLAG, "1");
  return demoSeedMessages(now);
}
