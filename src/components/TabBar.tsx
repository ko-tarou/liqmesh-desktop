import { MessageCircle, Sparkles, User } from "lucide-react";

export type RootTab = "chat" | "ai" | "profile";

type Props = {
  active: RootTab;
  onSelect: (tab: RootTab) => void;
  /** Unread count shown as a badge on the チャット tab (0 = hidden). */
  chatUnread?: number;
};

/** Tabs in the unified mobile-UI order: チャット / AI / プロフィール (lucide icons). */
const TABS: { id: RootTab; label: string; Icon: typeof MessageCircle }[] = [
  { id: "chat", label: "チャット", Icon: MessageCircle },
  { id: "ai", label: "AI", Icon: Sparkles },
  { id: "profile", label: "プロフィール", Icon: User },
];

/** Bottom tab bar (チャット / AI / プロフィール), themed via App.css `.tab-bar`. */
export function TabBar({ active, onSelect, chatUnread = 0 }: Props) {
  return (
    <nav className="tab-bar" role="tablist">
      {TABS.map(({ id, label, Icon }) => (
        <button
          key={id}
          role="tab"
          aria-selected={active === id}
          className={`tab-item${active === id ? " tab-item-active" : ""}`}
          onClick={() => onSelect(id)}
        >
          <span className="tab-icon" aria-hidden>
            <Icon size={22} />
            {id === "chat" && chatUnread > 0 && (
              <span className="tab-badge">{chatUnread > 99 ? "99+" : chatUnread}</span>
            )}
          </span>
          <span className="tab-label">{label}</span>
        </button>
      ))}
    </nav>
  );
}
