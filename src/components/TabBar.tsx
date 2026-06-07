import BottomNavigation from "@mui/material/BottomNavigation";
import BottomNavigationAction from "@mui/material/BottomNavigationAction";
import Badge from "@mui/material/Badge";
import Paper from "@mui/material/Paper";
import ChatIcon from "@mui/icons-material/Chat";
import AutoAwesomeIcon from "@mui/icons-material/AutoAwesome";
import PersonIcon from "@mui/icons-material/Person";

export type RootTab = "chat" | "ai" | "profile";

type Props = {
  active: RootTab;
  onSelect: (tab: RootTab) => void;
  /** Unread count shown as a badge on the チャット tab (0 = hidden). */
  chatUnread?: number;
};

/**
 * Bottom tab bar (Material) matching the iOS/Android scaffold:
 * チャット / AI / プロフィール, with a Material Badge on チャット for unread.
 */
export function TabBar({ active, onSelect, chatUnread = 0 }: Props) {
  return (
    <Paper elevation={3} square sx={{ flex: "0 0 auto" }}>
      <BottomNavigation
        showLabels
        value={active}
        onChange={(_, v) => onSelect(v as RootTab)}
      >
        <BottomNavigationAction
          value="chat"
          label="チャット"
          icon={
            <Badge badgeContent={chatUnread} color="error" max={99}>
              <ChatIcon />
            </Badge>
          }
        />
        <BottomNavigationAction value="ai" label="AI" icon={<AutoAwesomeIcon />} />
        <BottomNavigationAction value="profile" label="プロフィール" icon={<PersonIcon />} />
      </BottomNavigation>
    </Paper>
  );
}
