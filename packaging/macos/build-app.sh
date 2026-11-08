#!/usr/bin/env bash
# Builds RuneCraft.app: one universal binary for Apple Silicon and Intel Macs, with its icon,
# signed ad hoc so it runs locally. Run on a Mac: packaging/macos/build-app.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release -p runecraft -p runecraft-server --target aarch64-apple-darwin
cargo build --release -p runecraft -p runecraft-server --target x86_64-apple-darwin

APP=target/RuneCraft.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/runecraft" \
    target/aarch64-apple-darwin/release/runecraft target/x86_64-apple-darwin/release/runecraft
cp packaging/macos/Info.plist "$APP/Contents/Info.plist"

ICONSET=target/RuneCraft.iconset
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z $s $s packaging/macos/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) packaging/macos/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/RuneCraft.icns"

codesign --force --deep --sign - "$APP"

# The dedicated server, also universal, for hosting from a Mac.
lipo -create -output target/runecraft-server \
    target/aarch64-apple-darwin/release/runecraft-server target/x86_64-apple-darwin/release/runecraft-server
codesign --force --sign - target/runecraft-server

rm -f target/RuneCraft-macOS.zip
(cd target && ditto -c -k --keepParent RuneCraft.app RuneCraft-macOS.zip && zip -q RuneCraft-macOS.zip runecraft-server)
echo "Built target/RuneCraft.app and target/RuneCraft-macOS.zip"
