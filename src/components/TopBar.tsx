import AppBar from "@mui/material/AppBar";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import IconButton from "@mui/material/IconButton";
import SettingsIcon from "@mui/icons-material/Settings";
import SearchIcon from "@mui/icons-material/Search";

type Props = {
  /** Center title = the current tab's name. */
  title: string;
  onSettings: () => void;
  onSearch: () => void;
};

/**
 * Persistent top bar (Material AppBar): left ⚙ 設定, centered title, right 🔍 検索.
 * Green primary background, white icons/title — matches the mobile scaffold.
 */
export function TopBar({ title, onSettings, onSearch }: Props) {
  return (
    <AppBar position="static" color="primary" elevation={1}>
      <Toolbar variant="dense">
        <IconButton edge="start" color="inherit" aria-label="設定" onClick={onSettings}>
          <SettingsIcon />
        </IconButton>
        <Typography
          variant="h6"
          component="h1"
          sx={{ flex: 1, textAlign: "center", fontWeight: 600 }}
        >
          {title}
        </Typography>
        <IconButton edge="end" color="inherit" aria-label="検索" onClick={onSearch}>
          <SearchIcon />
        </IconButton>
      </Toolbar>
    </AppBar>
  );
}
