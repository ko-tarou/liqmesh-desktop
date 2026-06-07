type Props = {
  /** Center title = the current tab's name. */
  title: string;
  onSettings: () => void;
  onSearch: () => void;
};

/**
 * Persistent top bar (unified mobile-UI spec): top-LEFT ⚙️設定, center title,
 * top-RIGHT 🔍検索. Shown across all tabs.
 */
export function TopBar({ title, onSettings, onSearch }: Props) {
  return (
    <header className="top-bar">
      <button className="top-icon-btn" aria-label="設定" onClick={onSettings}>
        ⚙️
      </button>
      <span className="top-title">{title}</span>
      <button className="top-icon-btn" aria-label="検索" onClick={onSearch}>
        🔍
      </button>
    </header>
  );
}
