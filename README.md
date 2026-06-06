# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## BLE interop test (owner, on a real BLE Windows PC)

The desktop app is a BLE **central** (Windows cannot advertise). End-to-end BLE
cannot run on macOS/CI, so the only thing CI guarantees is that the code builds
and the pure-logic tests pass:

```bash
cargo build  --manifest-path src-tauri/Cargo.toml   # btleplug compiles
cargo test   --manifest-path src-tauri/Cargo.toml   # 69 pure-logic tests
npm run build                                        # frontend (tsc + vite)
```

To verify the real link (`docs/BLE_CONTRACT.md` "相互運用テスト合格条件"):

1. On a BLE-capable **Windows** PC, run `npm run tauri dev` (or
   `npm run tauri build` and launch the `.exe`).
2. Prepare a peer phone (Android/iOS) **advertising** the LiqMesh GATT service
   `B1E5C0DE-1A2B-4C3D-8E9F-000000000001` (TX `…0002` write, RX `…0003` notify),
   localName `LQM-<id4>`.
3. In the desktop UI enter `myId` / `myName`, click **Connect**, and confirm a
   `ble://connected` event appears in the log.
4. Exchange messages both ways and confirm `ble://frame` events; exercise
   reaction / delete / read frames and a long (multi-chunk) message — all must
   arrive without loss.
