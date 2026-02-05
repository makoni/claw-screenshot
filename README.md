# Claw Screenshot

<div align="center">
  <img src="https://raw.githubusercontent.com/makoni/claw-screenshot/main/installer/icons/hicolor/512x512/apps/claw-screenshot.png">

  <p>
    A small Rust utility to take desktop screenshots via the FreeDesktop Screenshot portal and save them to ~/Pictures.
  </p>
</div>

![Claw Screenshot](https://raw.githubusercontent.com/makoni/claw-screenshot/main/screenshot.png)

## Features
- Requests a screenshot using `org.freedesktop.portal.Screenshot` (works with GNOME / Wayland portals).
- Waits for the portal response and saves the screenshot into `~/Pictures`.

## Installation
Installer script (recommended):

```bash
# fetch script, inspect it, then run
curl -fsSL https://raw.githubusercontent.com/makoni/claw-screenshot/main/installer/install.sh -o /tmp/install-claw.sh
chmod +x /tmp/install-claw.sh

# install latest release
/tmp/install-claw.sh

# install a specific version
/tmp/install-claw.sh --version 0.1.0

# force user-local install (skip sudo/packages)
/tmp/install-claw.sh --user
```

The script auto-detects arch/OS, prefers .deb/.rpm with sudo, and falls back to a tarball in ~/.local/bin. It also creates the .desktop entry.

Build from source (requires Rust toolchain):

```bash
cd claw-screenshot
cargo build --release
# install the binary
install -m 755 target/release/claw-screenshot ~/.local/bin/
```

Or copy the prebuilt binary to `~/.local/bin/claw-screenshot`.

## Usage
Run the binary to request and save a screenshot. The command prints the saved path on success.

```bash
~/.local/bin/claw-screenshot
# Example output:
# Saved screenshot: /home/you/Pictures/Screenshot-2026-02-03-1030.png
```

To change the output directory, set `CLAW_SCREENSHOT_DIR` (the file is moved into that directory):

```bash
CLAW_SCREENSHOT_DIR="$HOME/Screenshots" ~/.local/bin/claw-screenshot
```

Important: on first run the system will prompt the user to allow screenshot access for the application (portal). You must accept this prompt for screenshots to be created. If you deny access, the portal will not return a file.

Security note: always inspect the installer script before running it and ensure the release assets are from this repository.

## Allowing portal screenshots (XDG Desktop Portal)
Some desktop portals (xdg-desktop-portal / GNOME/KDE portal backends) will only grant screenshot / window access to applications that are launched via a recognized desktop session entry. If you run the CLI directly from a terminal, the portal may deny permission and the screenshot will fail.

To ensure the screenshot helper can obtain permission reliably, create a minimal .desktop launcher and place it in your local applications folder.

1) Create the .desktop file (example)
- Path: `~/.local/share/applications/claw-screenshot.desktop`
- Content:

```ini
[Desktop Entry]
Type=Application
Name=Claw Screenshot Helper
Exec=/home/<your-user>/.local/bin/claw-screenshot
Icon=claw-screenshot
Terminal=false
Categories=Utility;
StartupNotify=true
X-GNOME-Autostart-enabled=false
```

Adjust `Exec` to the installed path of the binary if different. Replace `<your-user>` with your username or use `~` in docs but a full path is recommended in the actual file.

2) Make it discoverable
- Update desktop database (optional):
  `update-desktop-database ~/.local/share/applications || true`

3) Launch the helper via the desktop entry (one of these):
- From application launcher/menu (search "Claw Screenshot Helper")
- Or:
  `gtk-launch claw-screenshot`
  (or: `xdg-open ~/.local/share/applications/claw-screenshot.desktop`)

4) Why this helps
- Portals often check the launching "desktop app id" or session information to decide whether to present a permission dialog or allow ephemeral access. Launching via a .desktop entry gives the portal a recognizable application id and allows it to appear in permission flows.

5) Security notes
- The .desktop file merely makes the binary visible to the desktop session. Do not add setuid or world-writable permissions to the binary.
- If you prefer not to leave a launcher, you can create it temporarily, trigger a screenshot once, then remove the .desktop file.

6) Troubleshooting
- If screenshots still fail:
  - Check portal logs: `journalctl --user -u xdg-desktop-portal -f`
  - Ensure the `Exec` path is correct and the binary is executable: `ls -l ~/.local/bin/claw-screenshot`
  - Try launching the helper from the desktop menu instead of terminal.

## Notes
- The utility uses the session D-Bus to talk to `org.freedesktop.portal.Desktop`. It works best on Wayland sessions that implement the portal interfaces.
- The saved directory is `~/Pictures` by default or `CLAW_SCREENSHOT_DIR` if set.

## License
MIT — Copyright (c) 2026 Sergey Armodin
