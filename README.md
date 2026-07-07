# BuferNet

Clipboard and file transfer between computers on the same local network.
Lives in the system tray and discovers other computers automatically —
no IP addresses, no configuration.

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
computer — no Python required. Allow access to **private networks** when the
Windows Firewall asks.

From source:

```
pip install -r requirements.txt
pythonw main.py
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
pip install pyinstaller
pyinstaller --noconsole --onefile --name BuferNet --hidden-import pystray._win32 --collect-all sv_ttk --exclude-module numpy --exclude-module ssl --exclude-module _ssl --exclude-module _hashlib --exclude-module PIL.AvifImagePlugin --exclude-module PIL._avif --exclude-module PIL.WebPImagePlugin --exclude-module PIL._webp --exclude-module PIL._imagingft --exclude-module PIL.ImageCms --exclude-module PIL._imagingcms main.py
```

Pushing a `v*` tag builds the exe and publishes a GitHub Release automatically.

## License

[MIT](LICENSE)
