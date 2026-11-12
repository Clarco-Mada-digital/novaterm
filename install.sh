#!/usr/bin/env bash
# Installe NovaTerm pour l'utilisateur courant : binaire, icônes et entrée de menu.
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release
BIN="$HOME/.local/bin"
ICONS="$HOME/.local/share/icons/hicolor"
APPS="$HOME/.local/share/applications"

mkdir -p "$BIN" "$APPS" "$ICONS/scalable/apps"
install -m 755 target/release/novaterm "$BIN/novaterm"
install -m 644 assets/icon/novaterm.svg "$ICONS/scalable/apps/novaterm.svg"
for size in 16 24 32 48 64 128 256 512; do
  mkdir -p "$ICONS/${size}x${size}/apps"
  install -m 644 "assets/icon/novaterm-$size.png" "$ICONS/${size}x${size}/apps/novaterm.png"
done
install -m 644 assets/novaterm.desktop "$APPS/novaterm.desktop"

command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" || true
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true

echo "NovaTerm installé : $BIN/novaterm (menu : Système → NovaTerm)"
echo "Pour désinstaller : ./uninstall.sh  (--purge pour effacer aussi les réglages)"
case ":$PATH:" in *":$BIN:"*) ;; *) echo "⚠ Ajoute $BIN à ton PATH." ;; esac
