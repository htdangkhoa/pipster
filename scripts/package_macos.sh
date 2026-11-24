#!/usr/bin/env bash
# Build Pipster.app with Finder "Open With" document types.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="${PIPSTER_VERSION:-}"
if [[ -z "$VERSION" ]]; then
  VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
fi
VERSION="${VERSION:-0.1.0}"

cargo build -p pipster --release

APP="$ROOT/dist/Pipster.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RES="$CONTENTS/Resources"

rm -rf "$APP"
mkdir -p "$MACOS" "$RES"

cp "$ROOT/target/release/pipster" "$MACOS/Pipster"
chmod +x "$MACOS/Pipster"

# Wrapper so Open With / Dock launches find Homebrew mpv.
cat > "$MACOS/PipsterLauncher" <<'SH'
#!/bin/bash
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$DIR/Pipster" "$@"
SH
chmod +x "$MACOS/PipsterLauncher"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Pipster</string>
  <key>CFBundleDisplayName</key><string>Pipster</string>
  <key>CFBundleIdentifier</key><string>app.pipster.player</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key><string>PipsterLauncher</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Movie</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Default</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>mp4</string>
        <string>mov</string>
        <string>mkv</string>
        <string>webm</string>
        <string>avi</string>
        <string>m4v</string>
        <string>wmv</string>
        <string>flv</string>
        <string>mpeg</string>
        <string>mpg</string>
        <string>ts</string>
        <string>m2ts</string>
        <string>ogv</string>
      </array>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.movie</string>
        <string>public.mpeg-4</string>
        <string>public.avi</string>
        <string>com.apple.quicktime-movie</string>
        <string>org.matroska.mkv</string>
        <string>org.webmproject.webm</string>
      </array>
    </dict>
  </array>
  <key>UTImportedTypeDeclarations</key>
  <array>
    <dict>
      <key>UTTypeIdentifier</key><string>org.matroska.mkv</string>
      <key>UTTypeDescription</key><string>Matroska Video</string>
      <key>UTTypeConformsTo</key>
      <array><string>public.movie</string></array>
      <key>UTTypeTagSpecification</key>
      <dict>
        <key>public.filename-extension</key>
        <array><string>mkv</string></array>
      </dict>
    </dict>
  </array>
</dict>
</plist>
PLIST

cp "$ROOT/NOTICE" "$RES/NOTICE" 2>/dev/null || true

# Refresh Launch Services so Finder "Open With" picks up the app.
if command -v lsregister >/dev/null 2>&1; then
  lsregister -f "$APP" || true
elif [[ -x /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister ]]; then
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" || true
fi

echo "Built $APP (v${VERSION})"
echo "Open With: right-click a video → Open With → Pipster (may need one relaunch of Finder)."
echo "Requires Homebrew mpv on PATH (brew install mpv)."
