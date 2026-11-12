#!/usr/bin/env bash
# Désinstalle NovaTerm.
#
#   ./uninstall.sh            désinstalle le programme, garde vos réglages (choix proposé)
#   ./uninstall.sh --purge    supprime aussi réglages, session, cache
#   ./uninstall.sh --keep     garde réglages et session sans poser de question
#   ./uninstall.sh -y         ne pose aucune question (programme seulement, sauf --purge)
#
# Gère : installation utilisateur (install.sh, archive .tar.gz), paquet .deb / .rpm,
# raccourci « terminal rapide » du bureau. Une AppImage se supprime simplement en
# effaçant son fichier (rien d'autre n'est installé).
set -uo pipefail

PURGE=""
YES=0
for a in "$@"; do
  case "$a" in
    --purge) PURGE=1 ;;
    --keep) PURGE=0 ;;
    -y|--yes) YES=1 ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Option inconnue : $a (voir --help)"; exit 1 ;;
  esac
done

ask() { # ask "question" → 0 si oui
  [ "$YES" = 1 ] && return 1
  read -r -p "$1 [o/N] " r
  [[ "$r" =~ ^[oOyY]$ ]]
}

removed=0
rm_if() {
  for f in "$@"; do
    if [ -e "$f" ] || [ -L "$f" ]; then
      rm -rf -- "$f" && echo "  supprimé : $f" && removed=$((removed + 1))
    fi
  done
}

echo "==> Désinstallation de NovaTerm"

# 1. Installation utilisateur (install.sh / archive)
ICONS="$HOME/.local/share/icons/hicolor"
rm_if "$HOME/.local/bin/novaterm" "$HOME/.local/share/applications/novaterm.desktop" "$ICONS/scalable/apps/novaterm.svg"
for s in 16 24 32 48 64 128 256 512; do
  rm_if "$ICONS/${s}x${s}/apps/novaterm.png"
done
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
command -v update-desktop-database >/dev/null && update-desktop-database -q "$HOME/.local/share/applications" 2>/dev/null || true

# 2. Paquet système
if command -v dpkg >/dev/null && dpkg -s novaterm >/dev/null 2>&1; then
  echo "  Le paquet .deb « novaterm » est installé."
  if [ "$YES" = 1 ] || ask "  Le désinstaller (sudo apt remove novaterm) ?"; then
    sudo apt remove -y novaterm && removed=$((removed + 1))
  fi
elif command -v rpm >/dev/null && rpm -q novaterm >/dev/null 2>&1; then
  echo "  Le paquet .rpm « novaterm » est installé."
  if [ "$YES" = 1 ] || ask "  Le désinstaller (sudo dnf remove novaterm) ?"; then
    sudo dnf remove -y novaterm && removed=$((removed + 1))
  fi
fi

# 3. Raccourci global du terminal rapide (Cinnamon / GNOME)
if command -v gsettings >/dev/null; then
  for spec in "org.cinnamon.desktop.keybindings custom-list novaterm-quake" \
              "org.gnome.settings-daemon.plugins.media-keys custom-keybindings /org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/novaterm-quake/"; do
    read -r schema key item <<<"$spec"
    list=$(gsettings get "$schema" "$key" 2>/dev/null) || continue
    if [[ "$list" == *"$item"* ]]; then
      new=$(echo "$list" | sed -E "s#,? *'$item'##; s#\\[ *, *#[#")
      [ "$new" = "[]" ] && new="@as []"
      gsettings set "$schema" "$key" "$new" && echo "  raccourci du terminal rapide retiré" && removed=$((removed + 1))
    fi
  done
fi

# 3 bis. AppImage rangée dans ~/Applications (l'entrée de menu a été retirée ci-dessus)
for app in "$HOME"/Applications/NovaTerm*.AppImage; do
  [ -e "$app" ] || continue
  if [ "$YES" = 1 ] && [ "$PURGE" = 1 ] || ask "  Supprimer aussi le fichier $app ?"; then
    rm_if "$app"
  fi
done

# 4. Fichiers générés (toujours sans valeur : recréés au besoin)
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/novaterm"
rm_if "$CACHE" "${XDG_RUNTIME_DIR:-/nonexistent}/novaterm"

# 5. Réglages, session, profils
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/novaterm"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}/novaterm"
if [ -e "$CONFIG" ] || [ -e "$DATA" ]; then
  if [ -z "$PURGE" ]; then
    echo
    echo "  Vos réglages (thème, raccourcis, snippets, profils) et votre session sont conservés dans :"
    echo "    $CONFIG"
    echo "    $DATA"
    echo "  Ils resservent automatiquement si vous réinstallez (AppImage, .deb…)."
    ask "  Les supprimer aussi ?" && PURGE=1 || PURGE=0
  fi
  if [ "$PURGE" = 1 ]; then
    rm_if "$CONFIG" "$DATA"
  else
    echo "  réglages conservés"
  fi
fi

echo
if [ "$removed" -gt 0 ]; then
  echo "✔ NovaTerm désinstallé."
else
  echo "Rien à désinstaller (NovaTerm n'était pas installé par install.sh ni par paquet)."
fi
echo "  AppImage ailleurs que dans ~/Applications : supprimez simplement le fichier."
echo "  L'intégration shell ne modifie pas ~/.zshrc ni ~/.bashrc : rien à nettoyer."
