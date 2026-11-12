#!/usr/bin/env bash
# Construit NovaTerm pour macOS dans dist/ : NovaTerm.app puis NovaTerm-<version>-macos-<arch>.dmg
# À lancer sur un Mac (iconutil et hdiutil sont fournis par macOS).
# Usage : ./packaging/build-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# Binaire universel (Intel + Apple Silicon) si possible ; UNIVERSAL=0 pour l'architecture locale seule.
UNIVERSAL="${UNIVERSAL:-1}"
ARCH=$([ "$UNIVERSAL" = 1 ] && echo universal || uname -m)
DIST="$PWD/dist"
WORK="$PWD/target/package-macos"
APP="$WORK/NovaTerm.app"

echo "==> Compilation release ($VERSION, $ARCH)"
rm -rf "$WORK" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$DIST"
if [ "$UNIVERSAL" = 1 ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  lipo -create -output "$APP/Contents/MacOS/novaterm" \
    target/aarch64-apple-darwin/release/novaterm target/x86_64-apple-darwin/release/novaterm
  BIN="$PWD/target/$(uname -m | sed 's/arm64/aarch64/')-apple-darwin/release/novaterm"
else
  cargo build --release
  install -m755 target/release/novaterm "$APP/Contents/MacOS/novaterm"
  BIN="$PWD/target/release/novaterm"
fi

echo "==> Icône (.icns)"
"$BIN" --export-icon "$WORK/icons" >/dev/null
ICONSET="$WORK/novaterm.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  cp "$WORK/icons/novaterm-$s.png" "$ICONSET/icon_${s}x${s}.png"
done
cp "$WORK/icons/novaterm-32.png" "$ICONSET/icon_16x16@2x.png"
cp "$WORK/icons/novaterm-64.png" "$ICONSET/icon_32x32@2x.png"
cp "$WORK/icons/novaterm-256.png" "$ICONSET/icon_128x128@2x.png"
cp "$WORK/icons/novaterm-512.png" "$ICONSET/icon_256x256@2x.png"
cp "$WORK/icons/novaterm-512.png" "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/novaterm.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>NovaTerm</string>
  <key>CFBundleDisplayName</key><string>NovaTerm</string>
  <key>CFBundleIdentifier</key><string>app.novaterm.NovaTerm</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>novaterm</string>
  <key>CFBundleIconFile</key><string>novaterm</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
EOF

echo "==> Image disque (.dmg)"
DMG="$DIST/NovaTerm-$VERSION-macos-$ARCH.dmg"
rm -f "$DMG"
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "NovaTerm" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
echo "  → $DMG"
echo "Note : sans signature Apple, ouvrir la 1re fois avec clic droit → Ouvrir."
