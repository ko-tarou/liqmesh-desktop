import React from "react";
import ReactDOM from "react-dom/client";
import { ThemeProvider } from "@mui/material/styles";
import CssBaseline from "@mui/material/CssBaseline";
import { theme } from "./theme";
import App from "./App";

/**
 * Paints a readable error into the page instead of leaving a black screen.
 *
 * A blank window is the hardest failure to debug — there is nothing to read. So
 * if the root render throws (or an early uncaught error/rejection fires before
 * React mounts), we surface the message + stack right in `#root`. The owner sees
 * *what* broke instead of a black void, even on a build where opening devtools
 * is awkward.
 */
function showFatal(label: string, err: unknown): void {
  const root = document.getElementById("root");
  if (!root) return;
  const detail = err instanceof Error ? `${err.message}\n\n${err.stack ?? ""}` : String(err);
  root.innerHTML = `
    <div style="padding:24px;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;
                color:#f87171;background:#0e1116;min-height:100vh;white-space:pre-wrap;
                line-height:1.5;font-size:13px;overflow:auto;">
      <strong style="color:#fbbf24;font-size:15px;">LiqMesh failed to start (${label})</strong>
      <hr style="border:none;border-top:1px solid #232a33;margin:12px 0;" />
      ${detail.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]!)}
    </div>`;
}

window.addEventListener("error", (e) => showFatal("uncaught error", e.error ?? e.message));
window.addEventListener("unhandledrejection", (e) => showFatal("unhandled rejection", e.reason));

try {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ThemeProvider theme={theme}>
        <CssBaseline />
        <App />
      </ThemeProvider>
    </React.StrictMode>,
  );
} catch (err) {
  showFatal("render", err);
}
