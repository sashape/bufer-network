# BuferNet

Clipboard and file transfer between computers on the same local network.
Lives in the system tray and discovers other computers automatically —
no IP addresses, no configuration.

Written in Rust with a hand-drawn Direct2D interface — no UI frameworks,
a single exe under 0.5 MB. Fully protocol-compatible with the Python
version (v1.x): old and new instances see each other and exchange data.

## Features

- 📋 Send clipboard text — it lands straight in the other computer's clipboard
- 📁 Send files — saved to a folder of your choice (`Downloads\BuferNet` by default)
- 🔍 Automatic peer discovery on the LAN (UDP broadcast)
- 🖥 Windows 11 style UI: light/dark theme, HiDPI aware
- 🌍 Languages: English, Русский, Français, Deutsch, Español
- 🔔 Tray notifications for received clipboard and files
- 🚀 Network-wide updates: one click rolls a new version out to every computer

## Install

Download `BuferNet.exe` from [Releases](../../releases) and run it on each
computer. Allow access to **private networks** when the Windows Firewall asks.

From source:

```
cargo run --release
```

## Notes

- Computers must be on the same subnet; discovery does not cross routers or VPNs.
- Theme, language and save folder are in the ⚙ menu.
- To update the whole network: run a newer exe on one computer, then
  ⚙ → *Roll out update to network*.
- Security model: BuferNet trusts your LAN — there is no encryption or
  authentication. Don't run it on networks you don't trust.

## Build

```
cargo build --release
```

The exe lands in `target/release/bufernet.exe` (~480 KB, no dependencies).
Pushing a `v*` tag builds the exe and publishes a GitHub Release automatically.

## Architecture

- `src/ui/` — bare Win32 window, all widgets drawn by hand with
  Direct2D/DirectWrite (system components, nothing bundled into the exe)
- `src/discovery.rs` — UDP broadcast peer discovery (port 48765)
- `src/transfer.rs` — TCP transfer of clipboard/files/updates (port 48766)
- `src/json.rs` — tiny JSON parser for the wire protocol, no serde

## License

[MIT](LICENSE)
