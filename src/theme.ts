import { createTheme } from "@mui/material/styles";

/**
 * LiqMesh Material theme — light mode, white base, green primary.
 *
 * The brand seed is `#1E8E5A` (the ONE brand colour shared with iOS
 * `AccentColor` and Android `LiqMeshGreen`). Matching it exactly keeps the
 * screen-shared Desktop demo visually consistent with the phones.
 */
export const theme = createTheme({
  palette: {
    mode: "light",
    primary: {
      main: "#1E8E5A", // LiqMesh brand green (iOS AccentColor / Android LiqMeshGreen)
      contrastText: "#ffffff",
    },
    success: { main: "#1E8E5A" },
    background: {
      default: "#ffffff",
      paper: "#ffffff",
    },
    text: {
      primary: "#181d19", // Android Neutral10 (on-background)
      secondary: "#424842", // Android onSurfaceVariant
    },
    divider: "#e2e3de", // Android Neutral90
  },
  shape: { borderRadius: 12 },
  typography: {
    fontFamily:
      'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
  },
});
