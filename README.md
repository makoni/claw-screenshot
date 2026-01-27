# Claw Screenshot

Rust CLI to request screenshots via xdg-desktop-portal and save them to ~/clawd/screenshots.

Usage:
- Build: cargo build --release
- Install: cp target/release/claw-screenshot ~/.local/bin/
- Run once to allow portal: gtk-launch claw-screenshot

Notes:
- The project uses xdg-desktop-portal; first run will show a permission dialog in GNOME.
