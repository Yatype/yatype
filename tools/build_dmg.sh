#!/bin/bash
# Собирает универсальную программу (Apple Silicon + Intel) и установщик DMG
# в папку dist/.
#
#   tools/build_dmg.sh
#
# Подпись пока «для себя» (ad-hoc): macOS покажет предупреждение о
# неизвестном разработчике на чужих компьютерах. Для раздачи нужна подпись
# Developer ID и нотаризация.

set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="Yatype"
EXECUTABLE="Yatype"
BUNDLE_ID="com.yatype.app"
MIN_MACOS="10.15"
VERSION=$(grep -m1 '^version' crates/switcher/Cargo.toml | cut -d'"' -f2)

OUT="dist"
APP="$OUT/$APP_NAME.app"
DMG="$OUT/$EXECUTABLE-$VERSION.dmg"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

export PATH="$HOME/.cargo/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"

echo "→ Сборка под Apple Silicon и Intel"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --quiet -p switcher --bin switcher --target "$target"
done

echo "→ Пакет программы"
rm -rf "$OUT"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/$EXECUTABLE" \
    target/aarch64-apple-darwin/release/switcher \
    target/x86_64-apple-darwin/release/switcher
cp data/LICENSES.txt tools/Credits.html "$APP/Contents/Resources/"

echo "→ Иконка"
swift tools/make_icon.swift "$TMP/icon.png"
mkdir "$TMP/AppIcon.iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size "$TMP/icon.png" --out "$TMP/AppIcon.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$TMP/icon.png" --out "$TMP/AppIcon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns -o "$APP/Contents/Resources/AppIcon.icns" "$TMP/AppIcon.iconset"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$EXECUTABLE</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleName</key><string>$APP_NAME</string>
    <key>CFBundleDisplayName</key><string>$APP_NAME</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleDevelopmentRegion</key><string>ru</string>
    <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
    <key>LSUIElement</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>© 2026</string>
</dict>
</plist>
PLIST

echo "→ Подпись (ad-hoc)"
codesign --force --sign - "$APP"

echo "→ Установщик DMG"
STAGE="$TMP/dmg"
mkdir "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG"

echo
echo "Готово:"
lipo -archs "$APP/Contents/MacOS/$EXECUTABLE" | sed 's/^/  архитектуры: /'
du -sh "$APP" "$DMG" | sed 's/^/  /'
