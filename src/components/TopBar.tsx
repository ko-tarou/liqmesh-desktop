import { Settings, Search } from "lucide-react";

type Props = {
  /** Center title = the current tab's name. */
  title: string;
  onSettings: () => void;
  onSearch: () => void;
};

/**
 * Persistent top bar: left ⚙ 設定, centered title, right 🔍 検索. Green primary
 * background, white icons/title (themed via App.css `.top-bar`). Icons are
 * lucide-react (crisp, lightweight) replacing the old emoji.
 */
export function TopBar({ title, onSettings, onSearch }: Props) {
  return (
    <header className="top-bar">
      <button className="top-icon-btn" aria-label="設定" onClick={onSettings}>
        <Settings size={20} aria-hidden />
      </button>
      <span className="top-title">{title}</span>
      <button className="top-icon-btn" aria-label="検索" onClick={onSearch}>
        <Search size={20} aria-hidden />
      </button>
    </header>
  );
}
