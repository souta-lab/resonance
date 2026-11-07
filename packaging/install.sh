#!/usr/bin/env bash
# Install Resonance for the current user.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
DATA="$PREFIX/share/resonance"
UNIT="$HOME/.config/systemd/user"

echo "Building release binary…"
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p resonance-cli

echo "Building web UI…"
if [ ! -d "$ROOT/web/node_modules" ]; then
  (cd "$ROOT/web" && npm install --no-audit --no-fund --legacy-peer-deps)
fi
(cd "$ROOT/web" && npm run build)

echo "Installing to $PREFIX…"
mkdir -p "$BIN" "$DATA/web" "$UNIT"
install -m 755 "$ROOT/target/release/resonance" "$BIN/resonance"
rm -rf "$DATA/web"
cp -r "$ROOT/web/dist" "$DATA/web"
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
