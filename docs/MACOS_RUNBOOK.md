# LiqMesh Desktop — macOS BLE Central: Build & Run Runbook

For running the desktop app **on a Mac** (development + bundled `.app`) and
BLE-testing it against a phone. See also `docs/WINDOWS_RUNBOOK.md` (same app, same
wire contract) and `docs/BLE_CONTRACT.md` for the protocol.

**Role:** the desktop app is **BLE central only** (it scans for and connects to a
phone; it does not advertise). The phone (**iOS or Android**) must be
**advertising** the LiqMesh GATT service. Start the phone advertiser first.

The BLE stack (`btleplug` on CoreBluetooth) is fully portable — no Windows-only
gating — so the same code that runs on Windows scans/connects on macOS once
Bluetooth permission is granted (§3).

## 0. Hardware / OS prereqs

- macOS 11 (Big Sur) or later with built-in Bluetooth LE (any modern Mac).
- System Settings → Bluetooth: **ON**.

## 1. Install toolchain (once)

1. **Node.js LTS** (20.x or 22.x): <https://nodejs.org> → verify `node -v`, `npm -v`.
2. **Rust (stable)**: <https://rustup.rs> → verify `rustc --version`, `cargo --version`.
3. **Xcode Command Line Tools**: `xcode-select --install` (provides the linker/SDK
   Tauri needs). No full Xcode required.

## 2. Get the code & install deps (fresh clone)

```bash
git clone https://github.com/ko-tarou/liqmesh-desktop.git
cd liqmesh-desktop
npm install
```

(First run only) the Rust crates compile on the first `tauri` command — allow a
few minutes.

## 3. Run in dev mode (PREFERRED for the test)

```bash
npm run tauri dev
```

- A native window titled **"liqmesh-desktop"** opens (dark UI: top connection
  bar, room list, message pane, debug console at the bottom).
- **Run it from Terminal.app or iTerm** — see the Bluetooth-permission note below.

### Bluetooth permission in dev — IMPORTANT

`npm run tauri dev` launches the **bare** binary
(`src-tauri/target/debug/liqmesh-desktop`), which is *not* bundled and therefore
does **not** carry the app's `Info.plist`. On macOS, CoreBluetooth then attributes
the Bluetooth permission to the **parent terminal**, not to the app.

- The **first BLE scan** (click **Connect**) triggers a macOS prompt asking to let
  **your terminal** ("Terminal" or "iTerm") use Bluetooth → click **OK**.
- If you dismissed it or it never showed: System Settings → **Privacy & Security →
  Bluetooth** → enable your terminal app, then relaunch `npm run tauri dev`.
- The launch-time precheck dialog ("enable Bluetooth") only fires when *no adapter*
  is present; a permission that is merely un-granted surfaces as a scan that finds
  nothing until you approve the prompt above.

## 3b. Produce a standalone .app (FALLBACK)

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/macos/liqmesh-desktop.app` (and a `.dmg`).

The **bundled** `.app` carries `src-tauri/Info.plist`
(`NSBluetoothAlwaysUsageDescription`), so on first BLE use macOS prompts for **the
app itself** (not the terminal). Approve it, or grant it later under System
Settings → Privacy & Security → Bluetooth.

## 4. Connect / what the app scans for

Same as Windows — in the app's top bar:

- **`id <8 chars>`** is auto-generated per install (stable UUID; nothing to type).
- Type a **display name** (e.g. `Mac`).
- Click **Connect** → starts a **20-second** BLE scan.

The central scans for GATT **Service UUID**
`B1E5C0DE-1A2B-4C3D-8E9F-000000000001` and advertised **localName** `LQM-<id4>`,
then connects, subscribes to **RX** (`…0003`), writes to **TX** (`…0002`), and
sends a `hello`. **Success:** the status pill flips `offline` → `connecting…` →
**`connected`** (green) and shows the peer's name once their `hello` arrives.

For the full bidirectional msg / reaction / delete / read / long-message PASS
checklist, follow §5 of `docs/WINDOWS_RUNBOOK.md` (identical).

## 5. Troubleshooting

- **Black / blank window:** the WebView inspector is enabled (`devtools` feature) —
  right-click in the window → **Inspect Element** → read the **Console** tab for
  the real error. Any uncaught startup error is also painted directly into the
  window (red text) instead of a blank page, so it should never be a silent void.
- **Scan never finds the peer:** confirm Bluetooth is ON, the terminal/app has the
  Bluetooth permission (§3), the phone is actively advertising the LiqMesh service
  and within ~5–10 m, and no other app holds an exclusive connection to it.
- **VMs / screen-sharing-only sessions** may block BLE; run logged in locally on
  the physical Mac.
