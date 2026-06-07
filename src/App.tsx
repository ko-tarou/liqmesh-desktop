import "./App.css";
import { DEFAULT_ROOM_ID } from "./chat/frames";
import { useChatStore } from "./chat/useChatStore";
import { unreadCount } from "./chat/store";
import { useIdentity } from "./chat/useIdentity";
import { useBle } from "./chat/useBle";
import { useReadReceipts } from "./chat/useReadReceipts";
import { useBleAvailability } from "./chat/useBleAvailability";
import { ConnectionBar } from "./components/ConnectionBar";
import { RoomList } from "./components/RoomList";
import { MessageList } from "./components/MessageList";
import { Composer } from "./components/Composer";
import { DebugPanel } from "./components/DebugPanel";
import { BluetoothDialog } from "./components/BluetoothDialog";
import { NearbyPeerBanner } from "./components/NearbyPeerBanner";
import type { Message } from "./chat/store";

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
    connect,
    disconnect,
    sendMessage,
    sendReaction,
    sendDelete,
    sendRead,
  } = useBle();

  // Room state (persisted) drives which conversation is shown. Subscribing to
  // `rooms` re-renders the switcher when a new room is discovered/added.
  const activeRoomId = useChatStore((s) => s.activeRoomId);
  const rooms = useChatStore((s) => s.rooms);
  const setActiveRoom = useChatStore((s) => s.setActiveRoom);
  const addRoom = useChatStore((s) => s.addRoom);
  const peerNameOf = useChatStore((s) => s.peerName);

  // Subscribe to just the active room's messages; re-render only on changes.
  const messages = useChatStore((s) => s.messagesByRoom[activeRoomId] ?? EMPTY_MESSAGES);
  // Subscribe to the active room's reads so the "seen" marker re-renders when a
  // peer's read high-water-mark advances.
  const roomReads = useChatStore((s) => s.reads[activeRoomId]);
  // Subscribe to all messages + reads to drive per-room unread badges.
  const messagesByRoom = useChatStore((s) => s.messagesByRoom);
  const reads = useChatStore((s) => s.reads);

  // Always surface the default room, even if `rooms` somehow lacks it.
  const roomList = rooms.includes(DEFAULT_ROOM_ID) ? rooms : [DEFAULT_ROOM_ID, ...rooms];

  // Unread badge per room (derived; recomputes when messages/reads change). The
  // active room is excluded — viewing it is what marks it read.
  const unreadState = { messagesByRoom, reads, rooms, peers: {} };
  const unreadByRoom: Record<string, number> = {};
  for (const room of roomList) {
    if (room === activeRoomId) continue;
    unreadByRoom[room] = unreadCount(unreadState, room, myId);
  }

  const connected = status === "connected";
  const peerName = peerId ? peerNameOf(peerId) : undefined;

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

  function handleAddRoom(roomId: string) {
    addRoom(roomId);
    setActiveRoom(roomId);
  }

  return (
    <main className="app">
      {bleAvailable === false && (
        <BluetoothDialog reason={bleReason} onRetry={recheckBle} />
      )}

      <NearbyPeerBanner peer={nearbyPeer} />

      <ConnectionBar
        myId={myId}
        myName={myName}
        status={status}
        peerName={peerName}
        onNameChange={setMyName}
        onConnect={() => connect(myId, myName)}
        onDisconnect={disconnect}
      />

      <div className="app-body">
        <RoomList
          rooms={roomList}
          activeRoomId={activeRoomId}
          onSelect={setActiveRoom}
          onAdd={handleAddRoom}
          unreadByRoom={unreadByRoom}
        />

        <section className="room">
          <div className="room-header">
            <span className="room-name"># {activeRoomId}</span>
          </div>

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
      </div>

      <DebugPanel
        myId={myId}
        roomId={activeRoomId}
        connected={connected}
        stats={stats}
        error={error}
      />
    </main>
  );
}

export default App;
