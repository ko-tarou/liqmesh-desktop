import "./App.css";
import { useEffect, useState } from "react";
import { DEFAULT_ROOM_ID } from "./chat/frames";
import { useChatStore } from "./chat/useChatStore";
import { unreadCount } from "./chat/store";
import { buildDemoSeed } from "./chat/seedDemo";
import { useIdentity } from "./chat/useIdentity";
import { useBle } from "./chat/useBle";
import { useReadReceipts } from "./chat/useReadReceipts";
import { useBleAvailability } from "./chat/useBleAvailability";
import { ConnectionBar } from "./components/ConnectionBar";
import { MessageList } from "./components/MessageList";
import { Composer } from "./components/Composer";
import { DebugPanel } from "./components/DebugPanel";
import { BluetoothDialog } from "./components/BluetoothDialog";
import { NearbyPeerBanner } from "./components/NearbyPeerBanner";
import { TopBar } from "./components/TopBar";
import { TabBar, type RootTab } from "./components/TabBar";
import { ProfileTab } from "./components/ProfileTab";
import { AITab } from "./components/AITab";
import { SettingsDialog } from "./components/SettingsDialog";
import { SearchDialog } from "./components/SearchDialog";
import type { Message } from "./chat/store";

/** Center title shown in the persistent top bar per tab. */
const TAB_TITLE: Record<RootTab, string> = {
  chat: "チャット",
  ai: "AI",
  profile: "プロフィール",
};

// Stable empty reference for the "no messages yet" case. The selector below runs
// through React's useSyncExternalStore (Zustand v5), which re-renders whenever
// consecutive snapshots are not `Object.is`-equal. Returning a fresh `[]` each
// call would never settle → "Maximum update depth exceeded" (React #185) on a
// fresh install where the active room has no messages. A shared constant keeps
// the reference identical across renders so the loop never starts.
const EMPTY_MESSAGES: Message[] = [];

function App() {
  const { myId, myName, setMyName } = useIdentity();
  const { available: bleAvailable, reason: bleReason, recheck: recheckBle } =
    useBleAvailability();
  const {
    status,
    stats,
    error,
    peerId,
    nearbyPeer,
    peerCount,
    start,
    connect,
    disconnect,
    sendMessage,
    sendReaction,
    sendDelete,
    sendRead,
  } = useBle();

  // Room Model A: start the continuous multi-peer scan/connect supervisor as
  // soon as we have an identity, so the app auto-discovers and connects to every
  // nearby LiqMesh peer without a manual "Connect" — matching the phones.
  useEffect(() => {
    if (myId) void start(myId, myName);
    // Re-run only when the identity changes; `start` is stable (useCallback).
  }, [myId, myName, start]);

  // Unified mobile-UI scaffold: bottom tabs + top settings/search sheets.
  const [tab, setTab] = useState<RootTab>("chat");
  const [showSettings, setShowSettings] = useState(false);
  const [showSearch, setShowSearch] = useState(false);

  const peers = useChatStore((s) => s.peers);
  const clearChat = useChatStore((s) => s.clear);
  const addLocalMessage = useChatStore((s) => s.addLocalMessage);

  // Demo seed (once per install, only when the general room is empty): populate a
  // realistic disaster-mesh conversation so the app looks in-use and the AI
  // 要約/優先順位 demos have material. Idempotent — buildDemoSeed checks the
  // persisted flag and never overwrites real messages.
  useEffect(() => {
    const general = useChatStore.getState().messagesByRoom[DEFAULT_ROOM_ID] ?? [];
    const seeds = buildDemoSeed(Date.now(), general.length > 0);
    for (const m of seeds) {
      const { deleted: _d, reactions: _r, ...rest } = m;
      addLocalMessage(rest);
    }
    // Run once on mount; addLocalMessage is a stable store action.
  }, [addLocalMessage]);

  // Room Model A: one shared "general" group room. No per-peer DMs, no room
  // switcher — the チャット tab always shows "general".
  const activeRoomId = DEFAULT_ROOM_ID;

  // Subscribe to just the general room's messages; re-render only on changes.
  const messages = useChatStore((s) => s.messagesByRoom[activeRoomId] ?? EMPTY_MESSAGES);
  // Subscribe to the room's reads so the "seen" marker re-renders when a peer's
  // read high-water-mark advances.
  const roomReads = useChatStore((s) => s.reads[activeRoomId]);
  // All messages (flattened) feed the 検索 sheet.
  const messagesByRoom = useChatStore((s) => s.messagesByRoom);
  const reads = useChatStore((s) => s.reads);

  const connected = status === "connected";

  // Unread count for the チャット tab badge: other-sender, non-deleted messages
  // after my read mark, computed only while another tab is showing (viewing the
  // chat tab is what marks it read).
  const chatUnread =
    tab === "chat"
      ? 0
      : unreadCount({ messagesByRoom, reads, rooms: [activeRoomId], peers: {} }, activeRoomId, myId);

  const newestMessageId = messages.length > 0 ? messages[messages.length - 1].id : undefined;

  // "seen": the id of MY latest message the peer has read up to. The peer's
  // read mark is an id in this sorted list; the seen marker sits on the newest
  // message of mine at or before that position.
  const peerMark = peerId ? roomReads?.[peerId] : undefined;
  const seenMessageId = (() => {
    if (!peerMark) return undefined;
    const peerIdx = messages.findIndex((m) => m.id === peerMark);
    if (peerIdx === -1) return undefined;
    for (let i = peerIdx; i >= 0; i--) {
      if (messages[i].senderId === myId) return messages[i].id;
    }
    return undefined;
  })();

  useReadReceipts({ roomId: activeRoomId, newestMessageId, myId, connected, sendRead });

  // Flatten every room's messages for the 検索 sheet (Model A: practically just
  // "general", but kept general so older multi-room history is searchable too).
  const allMessages = Object.values(messagesByRoom).flat();

  return (
    <main className="app">
      {bleAvailable === false && (
        <BluetoothDialog reason={bleReason} onRetry={recheckBle} />
      )}

      <TopBar
        title={TAB_TITLE[tab]}
        onSettings={() => setShowSettings(true)}
        onSearch={() => setShowSearch(true)}
      />

      <div className="tab-content">
        {tab === "chat" && (
          <>
            <ConnectionBar
              myId={myId}
              myName={myName}
              status={status}
              peerCount={peerCount}
              onNameChange={setMyName}
              onConnect={() => connect(myId, myName)}
              onDisconnect={disconnect}
            />
            <NearbyPeerBanner peer={nearbyPeer} />

            <section className="room">
              <MessageList
                messages={messages}
                myId={myId}
                seenMessageId={seenMessageId}
                onReact={
                  connected
                    ? (messageId, emoji, op) => sendReaction(messageId, emoji, op, myId)
                    : undefined
                }
                onDelete={connected ? (messageId) => sendDelete(messageId, myId) : undefined}
              />
              <Composer
                disabled={!connected}
                onSend={(body) => sendMessage(body, activeRoomId, myId, myName)}
              />
            </section>

            <DebugPanel
              myId={myId}
              roomId={activeRoomId}
              connected={connected}
              stats={stats}
              error={error}
            />
          </>
        )}

        {tab === "ai" && <AITab messages={messages} />}

        {tab === "profile" && (
          <ProfileTab
            myId={myId}
            myName={myName}
            onNameChange={setMyName}
            peers={peers}
            onOpenChat={() => setTab("chat")}
          />
        )}
      </div>

      <TabBar active={tab} onSelect={setTab} chatUnread={chatUnread} />

      {showSettings && (
        <SettingsDialog
          myName={myName}
          onNameChange={setMyName}
          onClearChat={() => {
            clearChat();
            setShowSettings(false);
          }}
          onClose={() => setShowSettings(false)}
        />
      )}
      {showSearch && (
        <SearchDialog messages={allMessages} onClose={() => setShowSearch(false)} />
      )}
    </main>
  );
}

export default App;
