#!/usr/bin/env bash
# Construit les paquets Linux de NovaTerm dans dist/ :
#   .deb (Debian, Ubuntu, Linux Mint), .AppImage (toutes distributions),
#   .tar.gz (archive portable) et .rpm (Fedora, openSUSE) si rpmbuild est installé.
#
# Usage : ./packaging/build-linux.sh [deb|appimage|tar|rpm|all]   (défaut : all)
set -euo pipefail
cd "$(dirname "$0")/.."

WHAT="${1:-all}"
NAME=novaterm
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH_RUST=$(uname -m)                                   # x86_64, aarch64
ARCH_DEB=$(dpkg --print-architecture 2>/dev/null || echo amd64)
DIST="$PWD/dist"
WORK="$PWD/target/package"
MAINTAINER="${MAINTAINER:-$(git config user.name 2>/dev/null || echo NovaTerm) <$(git config user.email 2>/dev/null || echo noreply@localhost)>}"

step() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

step "Compilation release ($VERSION, $ARCH_RUST)"
cargo build --release
BIN="$PWD/target/release/$NAME"

rm -rf "$WORK" && mkdir -p "$WORK" "$DIST"
step "Icônes"
"$BIN" --export-icon "$WORK/icons" >/dev/null

# Arborescence /usr commune (deb, rpm, AppImage).
make_usr() {
  local root="$1"
  install -Dm755 "$BIN" "$root/usr/bin/$NAME"
  install -Dm644 assets/novaterm.desktop "$root/usr/share/applications/$NAME.desktop"
  install -Dm644 "$WORK/icons/novaterm.svg" "$root/usr/share/icons/hicolor/scalable/apps/$NAME.svg"
  for s in 16 24 32 48 64 128 256 512; do
    install -Dm644 "$WORK/icons/novaterm-$s.png" "$root/usr/share/icons/hicolor/${s}x${s}/apps/$NAME.png"
  done
  install -Dm644 README.md "$root/usr/share/doc/$NAME/README.md"
  install -Dm644 assets/fonts/OFL.txt "$root/usr/share/doc/$NAME/OFL-JetBrainsMono.txt"
}

build_tar() {
  step "Archive .tar.gz"
  local dir="$WORK/$NAME-$VERSION-linux-$ARCH_RUST"
  mkdir -p "$dir"
  install -m755 "$BIN" "$dir/$NAME"
  cp -r "$WORK/icons" "$dir/icons"
  cp assets/novaterm.desktop README.md uninstall.sh "$dir/"
  cat > "$dir/install.sh" <<'EOF'
#!/usr/bin/env bash
# Installation pour l'utilisateur courant (sans droits administrateur).
set -e
cd "$(dirname "$0")"
install -Dm755 novaterm "$HOME/.local/bin/novaterm"
install -Dm644 icons/novaterm.svg "$HOME/.local/share/icons/hicolor/scalable/apps/novaterm.svg"
install -Dm644 novaterm.desktop "$HOME/.local/share/applications/novaterm.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$HOME/.local/share/applications" || true
echo "NovaTerm installé dans ~/.local/bin"
EOF
  chmod +x "$dir/install.sh"
  tar -C "$WORK" -czf "$DIST/$NAME-$VERSION-linux-$ARCH_RUST.tar.gz" "$(basename "$dir")"
  echo "  → dist/$NAME-$VERSION-linux-$ARCH_RUST.tar.gz"
}

build_deb() {
  step "Paquet .deb"
  local root="$WORK/deb"
  make_usr "$root"
  mkdir -p "$root/DEBIAN"
  local size
  size=$(du -sk "$root/usr" | cut -f1)
  cat > "$root/DEBIAN/control" <<EOF
Package: $NAME
Version: $VERSION
Section: x11
Priority: optional
Architecture: $ARCH_DEB
Installed-Size: $size
Depends: libc6, libgl1, libxkbcommon0, libx11-6, libxcursor1, libxrandr2, libxi6
Recommends: fontconfig, libnotify-bin
Provides: x-terminal-emulator
Maintainer: $MAINTAINER
Description: Terminal moderne : onglets, splits, suggestions et images
 NovaTerm est un émulateur de terminal rapide écrit en Rust : onglets,
 divisions d'écran, suggestions de commandes, blocs de commandes avec
 statut et durée, images dans le terminal, liens cliquables, sessions,
 profils de projet, terminal rapide (quake), thèmes et transparence.
EOF
  cat > "$root/DEBIAN/postinst" <<EOF
#!/bin/sh
set -e
if [ "\$1" = "configure" ]; then
  update-alternatives --install /usr/bin/x-terminal-emulator x-terminal-emulator /usr/bin/$NAME 20 || true
fi
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
command -v update-desktop-database >/dev/null && update-desktop-database -q /usr/share/applications || true
exit 0
EOF
  cat > "$root/DEBIAN/prerm" <<EOF
#!/bin/sh
set -e
if [ "\$1" = "remove" ]; then
  update-alternatives --remove x-terminal-emulator /usr/bin/$NAME || true
fi
exit 0
EOF
  chmod 755 "$root/DEBIAN/postinst" "$root/DEBIAN/prerm"
  dpkg-deb --root-owner-group --build "$root" "$DIST/${NAME}_${VERSION}_${ARCH_DEB}.deb" >/dev/null
  echo "  → dist/${NAME}_${VERSION}_${ARCH_DEB}.deb   (sudo apt install ./dist/${NAME}_${VERSION}_${ARCH_DEB}.deb)"
}

build_appimage() {
  step "AppImage"
  local appdir="$WORK/NovaTerm.AppDir"
  make_usr "$appdir"
  cp assets/novaterm.desktop "$appdir/$NAME.desktop"
  cp "$WORK/icons/novaterm-256.png" "$appdir/$NAME.png"
  cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/novaterm" "$@"
EOF
  chmod +x "$appdir/AppRun"
  local tool
  tool=$(command -v appimagetool || true)
  if [ -z "$tool" ]; then
    tool="$PWD/packaging/.tools/appimagetool-$ARCH_RUST.AppImage"
    if [ ! -x "$tool" ]; then
      echo "  Téléchargement d'appimagetool…"
      mkdir -p "$(dirname "$tool")"
      curl -fL --retry 3 -o "$tool" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH_RUST.AppImage"
      chmod +x "$tool"
    fi
  fi
  # --appimage-extract-and-run : fonctionne même sans FUSE (CI, conteneurs).
  ARCH="$ARCH_RUST" "$tool" --appimage-extract-and-run "$appdir" "$DIST/NovaTerm-$VERSION-$ARCH_RUST.AppImage" >/dev/null
  echo "  → dist/NovaTerm-$VERSION-$ARCH_RUST.AppImage   (chmod +x puis double-clic)"
}

build_rpm() {
  if ! command -v rpmbuild >/dev/null; then
    echo "  (rpmbuild absent : paquet .rpm ignoré — sudo apt install rpm pour l'activer)"
    return
  fi
  step "Paquet .rpm"
  local top="$WORK/rpm"
  mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS,BUILDROOT}
  make_usr "$top/staging"
  cat > "$top/SPECS/$NAME.spec" <<EOF
%global debug_package %{nil}
%global __os_install_post %{nil}
Name: $NAME
Version: $VERSION
Release: 1
Summary: Terminal moderne : onglets, splits, suggestions et images
License: MIT
Requires: mesa-libGL, libxkbcommon
%description
Émulateur de terminal moderne écrit en Rust.
%install
cp -a $top/staging/. %{buildroot}/
%files
/usr/bin/$NAME
/usr/share/applications/$NAME.desktop
/usr/share/icons/hicolor/*/apps/$NAME.*
/usr/share/doc/$NAME
EOF
  rpmbuild --define "_topdir $top" -bb "$top/SPECS/$NAME.spec" >/dev/null
  find "$top/RPMS" -name '*.rpm' -exec cp {} "$DIST/" \;
  echo "  → dist/$(basename "$(find "$top/RPMS" -name '*.rpm' | head -1)")"
}

case "$WHAT" in
  deb) build_deb ;;
  appimage) build_appimage ;;
  tar) build_tar ;;
  rpm) build_rpm ;;
  all) build_tar; build_deb; build_appimage; build_rpm ;;
  *) echo "Usage : $0 [deb|appimage|tar|rpm|all]"; exit 1 ;;
esac

step "Terminé"
ls -lh "$DIST"
