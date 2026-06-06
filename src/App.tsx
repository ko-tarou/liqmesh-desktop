import "./App.css";
import { DEFAULT_ROOM_ID } from "./chat/frames";
import { useChatStore } from "./chat/useChatStore";
import { useIdentity } from "./chat/useIdentity";
import { useBle } from "./chat/useBle";
import { useReadReceipts } from "./chat/useReadReceipts";
import { ConnectionBar } from "./components/ConnectionBar";
import { RoomList } from "./components/RoomList";
import { MessageList } from "./components/MessageList";
import { Composer } from "./components/Composer";
import { DebugPanel } from "./components/DebugPanel";

function App() {
  const { myId, myName, setMyName } = useIdentity();
  const {
    status,
    stats,
    error,
    peerId,
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
  const messages = useChatStore((s) => s.messagesByRoom[activeRoomId] ?? []);
  // Subscribe to the active room's reads so the "seen" marker re-renders when a
  // peer's read high-water-mark advances.
  const roomReads = useChatStore((s) => s.reads[activeRoomId]);

  // Always surface the default room, even if `rooms` somehow lacks it.
  const roomList = rooms.includes(DEFAULT_ROOM_ID) ? rooms : [DEFAULT_ROOM_ID, ...rooms];

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
