#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<EOF
Usage: install.sh <artifact-url> <sha256-hex>
Downloads artifact, verifies sha256, installs binary to ~/.local/bin and creates a .desktop entry.
EOF
}

if [ "$#" -ne 2 ]; then
  usage
  exit 2
fi

URL=$1
SHA_EXPECT=$2
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

ARCHIVE="$TMPDIR/art.tar.gz"

echo "Downloading $URL..."
curl -fL -o "$ARCHIVE" "$URL"

echo "Computing sha256..."
SHA_ACTUAL=$(sha256sum "$ARCHIVE" | awk '{print $1}')
if [ "$SHA_ACTUAL" != "$SHA_EXPECT" ]; then
  echo "Checksum mismatch! expected $SHA_EXPECT but got $SHA_ACTUAL" >&2
  exit 3
fi

echo "Checksum OK"

mkdir -p "$HOME/.local/bin"

# extract and find executable
tar -xzf "$ARCHIVE" -C "$TMPDIR"
BIN_PATH=$(find "$TMPDIR" -type f -name "claw-screenshot" -perm /111 | head -n1 || true)
if [ -z "$BIN_PATH" ]; then
  echo "claw-screenshot binary not found in archive" >&2
  exit 4
fi

install -m 0755 "$BIN_PATH" "$HOME/.local/bin/claw-screenshot"

# create .desktop
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/claw-screenshot.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Claw Screenshot Helper
Exec=$HOME/.local/bin/claw-screenshot
Icon=utilities-terminal
Terminal=false
Categories=Utility;
StartupNotify=true
DESK

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DIR" || true
fi

echo "Installed to $HOME/.local/bin/claw-screenshot"
echo "Desktop entry: $DESKTOP_DIR/claw-screenshot.desktop"

echo "Done."