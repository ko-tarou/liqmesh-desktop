export type RootTab = "chat" | "ai" | "profile";

type Props = {
  active: RootTab;
  onSelect: (tab: RootTab) => void;
  /** Unread count shown as a badge on the チャット tab (0 = hidden). */
  chatUnread?: number;
};

/** The three tabs in the unified mobile-UI order: チャット / AI / プロフィール. */
const TABS: { id: RootTab; label: string; icon: string }[] = [
  { id: "chat", label: "チャット", icon: "💬" },
  { id: "ai", label: "AI", icon: "✨" },
  { id: "profile", label: "プロフィール", icon: "👤" },
];

/** Bottom tab bar matching the iOS/Android scaffold (チャット / AI / プロフィール). */
export function TabBar({ active, onSelect, chatUnread = 0 }: Props) {
  return (
    <nav className="tab-bar" role="tablist">
      {TABS.map((t) => (
        <button
          key={t.id}
          role="tab"
          aria-selected={active === t.id}
          className={`tab-item${active === t.id ? " tab-item-active" : ""}`}
          onClick={() => onSelect(t.id)}
        >
          <span className="tab-icon" aria-hidden>
            {t.icon}
            {t.id === "chat" && chatUnread > 0 && (
              <span className="tab-badge">{chatUnread}</span>
            )}
          </span>
          <span className="tab-label">{t.label}</span>
        </button>
      ))}
    </nav>
  );
}
