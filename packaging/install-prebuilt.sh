#!/usr/bin/env bash
# Install a prebuilt Resonance release for the current user.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
DATA="$PREFIX/share/resonance"
UNIT="$HOME/.config/systemd/user"

if [ ! -x "$ROOT/resonance" ]; then
  echo "error: $ROOT/resonance not found; run this from the extracted release" >&2
  exit 1
fi

echo "Installing Resonance to $PREFIX…"
mkdir -p "$BIN" "$DATA/web" "$UNIT"
install -m 755 "$ROOT/resonance" "$BIN/resonance"
rm -rf "$DATA/web"
cp -r "$ROOT/web/." "$DATA/web/"
install -m 644 "$ROOT/packaging/systemd/resonance.service" "$UNIT/resonance.service"

echo
echo "Installed:"
echo "  $BIN/resonance"
echo "  $DATA/web"
echo "  $UNIT/resonance.service"
echo
echo "Enable and start:"
echo "  systemctl --user daemon-reload"
echo "  systemctl --user enable --now resonance.service"
echo "  xdg-open http://127.0.0.1:8737"
