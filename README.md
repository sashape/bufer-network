# BuferNet

Clipboard and file transfer between computers on the same local network.
Lives in the system tray and discovers other computers automatically —
no IP addresses, no configuration.

Written in Rust with a hand-drawn Direct2D interface — no UI frameworks,
a single exe under 0.5 MB. Fully protocol-compatible with the Python
version (v1.x): old and new instances see each other and exchange data.

## Features

- 📋 Send clipboard text or images — they land straight in the other computer's clipboard
- 📁 Send files — saved to a folder of your choice (`Downloads\BuferNet` by default);
  clicking the notification opens the folder with the file selected
- 🔍 Automatic peer discovery on the LAN (UDP broadcast)
- 🖥 Windows 11 style UI: light/dark theme, HiDPI aware
- 🌍 Languages: English, Русский, Français, Deutsch, Español
- 🔔 Tray notifications for received clipboard and files
- ⌨️ Global hotkeys for quick send: `Ctrl+Alt+B` — clipboard, `Ctrl+Alt+F` — files
  (change or disable them in the ⚙ menu)
- ☝️ Single instance: launching a second copy just brings up the running window
- 🚀 Network-wide updates: one click rolls a new version out to every computer

## Install

Download from [Releases](../../releases) — either one works on each computer:

- `BuferNet.msi` — per-user install (no admin rights): Start Menu shortcut
  and **start with Windows** enabled out of the box
- `BuferNet.exe` — portable, just run it

Allow access to **private networks** when the Windows Firewall asks.
Autostart can be toggled any time in the ⚙ menu.

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

MSI (needs the [WiX](https://wixtoolset.org) CLI, `dotnet tool install --global wix --version 5.0.2`,
plus the UI and Util extensions: `wix extension add -g WixToolset.UI.wixext/5.0.2 WixToolset.Util.wixext/5.0.2`):

```
wix build wix/bufernet.wxs -d ProductVersion=2.2.2 -arch x64 -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext -o BuferNet.msi
```

Pushing a `v*` tag builds both and publishes a GitHub Release automatically.

## Architecture

- `src/ui/` — bare Win32 window, all widgets drawn by hand with
  Direct2D/DirectWrite (system components, nothing bundled into the exe)
- `src/discovery.rs` — UDP broadcast peer discovery (port 48765)
- `src/transfer.rs` — TCP transfer of clipboard/files/updates (port 48766)
- `src/json.rs` — tiny JSON parser for the wire protocol, no serde

## License

[MIT](LICENSE)
