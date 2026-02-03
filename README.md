# Claw Screenshot

A small Rust utility to take desktop screenshots via the FreeDesktop Screenshot portal and save them to `~/clawd/screenshots`.

## Features
- Requests a screenshot using `org.freedesktop.portal.Screenshot` (works with GNOME / Wayland portals).
- Waits for the portal response and copies the temporary file into `~/clawd/screenshots`.

## Installation
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
# SAVED:~/clawd/screenshots/Screenshot-2026-02-03-1030.png
```

Important: on first run the system will prompt the user to allow screenshot access for the application (portal). You must accept this prompt for screenshots to be created. If you deny access, the portal will not return a file.

## Notes
- The utility uses the session D-Bus to talk to `org.freedesktop.portal.Desktop`. It works best on Wayland sessions that implement the portal interfaces.
- The saved directory is `~/clawd/screenshots` by default. Change `dst_dir` in `src/main.rs` if you want another location.

## License
MIT — Copyright (c) 2026 Sergey Armodin
