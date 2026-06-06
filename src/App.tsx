import "./App.css";
import { DEFAULT_ROOM_ID } from "./chat/frames";
import { useChatStore } from "./chat/useChatStore";
import { useIdentity } from "./chat/useIdentity";
import { useBle } from "./chat/useBle";
import { ConnectionBar } from "./components/ConnectionBar";
import { RoomList } from "./components/RoomList";
import { MessageList } from "./components/MessageList";
import { Composer } from "./components/Composer";
import { DebugPanel } from "./components/DebugPanel";

function App() {
  const { myId, myName, setMyName } = useIdentity();
  const { status, stats, error, peerId, connect, disconnect, sendMessage, sendReaction } =
    useBle();

  // Room state (persisted) drives which conversation is shown. Subscribing to
  // `rooms` re-renders the switcher when a new room is discovered/added.
  const activeRoomId = useChatStore((s) => s.activeRoomId);
  const rooms = useChatStore((s) => s.rooms);
  const setActiveRoom = useChatStore((s) => s.setActiveRoom);
  const addRoom = useChatStore((s) => s.addRoom);
  const peerNameOf = useChatStore((s) => s.peerName);

  // Subscribe to just the active room's messages; re-render only on changes.
  const messages = useChatStore((s) => s.messagesByRoom[activeRoomId] ?? []);

  // Always surface the default room, even if `rooms` somehow lacks it.
  const roomList = rooms.includes(DEFAULT_ROOM_ID) ? rooms : [DEFAULT_ROOM_ID, ...rooms];

  const connected = status === "connected";
  const peerName = peerId ? peerNameOf(peerId) : undefined;

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
            onReact={
              connected
                ? (messageId, emoji, op) => sendReaction(messageId, emoji, op, myId)
                : undefined
            }
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
