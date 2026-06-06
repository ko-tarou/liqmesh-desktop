# LiqMesh Desktop — Windows BLE Central: Build & Run Runbook

For the **phone-advertiser ↔ Windows-central** device test. See also the README
"BLE interop test" section and `docs/BLE_CONTRACT.md` (v1.5) for the wire contract.

**Role:** the Windows desktop app is **BLE central only** (Windows cannot
advertise). It **scans for and connects to** the phone (**iOS or Android** — both
advertise the same LiqMesh GATT service, so the steps below are identical for
either), which must be **advertising** the LiqMesh GATT service. Start the phone
advertiser first.

## 0. Hardware / OS prereqs

- A real **Windows 10 (1809+) or Windows 11** PC with a working **Bluetooth LE**
  adapter (built-in laptop BT or a BT 4.0+ USB dongle). A VM usually will **not**
  pass BLE through — use bare metal.
- Settings → Bluetooth & devices: **Bluetooth ON**.
- Windows 11: WebView2 is preinstalled. Windows 10: if the app window is blank,
  install the **Microsoft Edge WebView2 Runtime** (Evergreen) from Microsoft.

## 1. Install toolchain (once)

1. **Node.js LTS** (20.x or 22.x): <https://nodejs.org> → verify `node -v`, `npm -v`.
2. **Rust (stable, MSVC)**: <https://rustup.rs> → run `rustup-init.exe`, accept
   defaults → verify `rustc --version`, `cargo --version`.
3. **Visual Studio Build Tools (MSVC)** — required by Rust on Windows: install the
   **"Desktop development with C++"** workload from the VS Build Tools installer
   (<https://visualstudio.microsoft.com/downloads/> → Tools for Visual Studio →
   Build Tools). Reboot afterwards. (Tauri v2 needs no extra Rust prereqs beyond
   MSVC + WebView2.)

## 2. Get the code & install deps (fresh clone)

```bash
git clone https://github.com/ko-tarou/liqmesh-desktop.git
cd liqmesh-desktop
npm install
```

(First run only) the Rust crates compile on the first `tauri` command — allow a
few minutes.

## 3a. Run in dev mode (PREFERRED for the test)

```bash
npm run tauri dev
```

- First launch builds the Rust side (btleplug etc.); subsequent launches are fast.
- A native window titled **"liqmesh-desktop"** opens. If a Windows Firewall or
  Bluetooth permission prompt appears, **Allow** it.

## 3b. Produce a standalone .exe (FALLBACK)

```bash
npm run tauri build
```

Output:

- Installer (`.msi` / NSIS `.exe`): `src-tauri\target\release\bundle\`
- Raw executable: `src-tauri\target\release\liqmesh-desktop.exe`

Double-click the `.exe` (or install the `.msi`) to launch.

## 4. Connect / what the app scans for

In the app's top bar:

- **`id <8 chars>`** is auto-generated per install (stable UUID; nothing to type).
- Type a **display name** in the "display name" field (e.g. `Win`).
- Click **Connect**. This starts a **20-second** BLE scan.

The central scans for:

- GATT **Service UUID** `B1E5C0DE-1A2B-4C3D-8E9F-000000000001` (primary match)
- Advertised **localName** `LQM-<first 4 chars of the phone's id>` (secondary signal)

On finding the phone it connects, subscribes to **RX** (`…0003`), writes to
**TX** (`…0002`), and sends a `hello`.

**Success looks like:** the status pill flips `offline` → `connecting…` →
**`connected`** (green). Once the phone's `hello` arrives, the pill also shows
the peer's name (`· <phone name>`). If 20s elapse with no peer, it returns to
`offline` with an error — re-check the phone is advertising and in range, then
click **Connect** again.

(Optional) open the **"Debug · interop console"** at the bottom to watch protocol
counters and send raw `reaction`/`delete`/`read` frames for the round-trip checks.

## 5. PASS checklist

All must hold (ref `docs/BLE_CONTRACT.md` v1.5 "相互運用テスト合格条件"):

- [ ] 1. **Scan → connect:** status reaches `connected` within 20s of clicking Connect.
- [ ] 2. **Hello exchange:** the peer name appears in the status pill (proves the
      phone's `hello` was received & parsed).
- [ ] 3. **Bidirectional msg:** Send from Windows → appears on the phone; send from
      the phone → appears in the Windows message list. Both directions.
- [ ] 4. **Reaction round-trip:** react (👍) on one side → the chip + count updates
      on the other. Toggle off → updates on both.
- [ ] 5. **Delete round-trip:** delete your own message on one side → shows as
      "メッセージは削除されました" (tombstone) on both. A delete of the **other**
      party's message must be **rejected** (author-only).
- [ ] 6. **Read round-trip:** exercise a `read` frame (Debug console "Read" template
      or the iOS read affordance) → no errors, applied on both sides.
- [ ] 7. **Long multi-chunk message:** send a long message (e.g. paste 1–2 KB of
      text) → arrives complete and uncorrupted on the other side (verifies chunk
      split/reassembly; MTU 247 negotiated, falls back to 23).

## 6. Known Windows BLE caveats

- btleplug uses the **WinRT** backend on Windows. The PC adapter is
  central-capable but **cannot advertise** — that's expected; the phone (iOS or
  Android) is the advertiser.
- **VMs / RDP** sessions typically block BLE; run on the physical machine, logged
  in locally.
- **Scan never finds the peer:** confirm Bluetooth is ON in Windows Settings, the
  adapter is BLE-capable (not BT Classic only), the phone is actively advertising
  the LiqMesh service and within ~5–10 m, and no other app is holding an exclusive
  connection to it. (On Android, also confirm the app has the runtime BT/location
  permissions it needs to advertise.)
- **Blank window on Win10:** install the WebView2 Runtime (§0) and relaunch.
- Antivirus/Firewall may prompt on first launch of the `.exe` — allow it.
