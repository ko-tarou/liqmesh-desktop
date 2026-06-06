import "./App.css";
import { DEFAULT_ROOM_ID } from "./chat/frames";
import { useChatStore } from "./chat/useChatStore";
import { useIdentity } from "./chat/useIdentity";
import { useBle } from "./chat/useBle";
import { ConnectionBar } from "./components/ConnectionBar";
import { MessageList } from "./components/MessageList";
import { Composer } from "./components/Composer";
import { DebugPanel } from "./components/DebugPanel";

/** PR-C1b ships a single room; multiple rooms / presence land in C2. */
const ROOM_ID = DEFAULT_ROOM_ID;

function App() {
  const { myId, myName, setMyName } = useIdentity();
  const { status, stats, error, connect, disconnect, sendMessage } = useBle();

  // Subscribe to just this room's messages; re-renders only on relevant changes.
  const messages = useChatStore((s) => s.messagesByRoom[ROOM_ID] ?? []);

  const connected = status === "connected";

  return (
    <main className="app">
      <ConnectionBar
        myId={myId}
        myName={myName}
        status={status}
        onNameChange={setMyName}
        onConnect={() => connect(myId, myName)}
        onDisconnect={disconnect}
      />

      <section className="room">
        <div className="room-header">
          <span className="room-name"># {ROOM_ID}</span>
        </div>

        <MessageList messages={messages} myId={myId} />

        <Composer
          disabled={!connected}
          onSend={(body) => sendMessage(body, ROOM_ID, myId, myName)}
        />
      </section>

      <DebugPanel
        myId={myId}
        roomId={ROOM_ID}
        connected={connected}
        stats={stats}
        error={error}
      />
    </main>
  );
}

export default App;
