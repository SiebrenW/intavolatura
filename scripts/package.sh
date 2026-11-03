#!/bin/sh
# Builds the app in release mode and packs it for one platform:
#   scripts/package.sh <macos|linux|windows> <version> [out-dir]
# macos:   a universal (arm64 + x86_64) Intavolatura.app in intavolatura-<version>-macos.zip
# linux:   intavolatura-<version>-linux-x86_64.tar.gz
# windows: intavolatura-<version>-windows-x86_64.zip
# Bravura's licence (OFL) goes along, as the font is embedded. Used by
# .github/workflows/release.yml; runs locally too.
set -eu
platform=$1
version=$2
out=${3:-dist}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
mkdir -p "$out"
out=$(cd "$out" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

case "$platform" in
macos)
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --release --locked -p intavolatura --target "$target"
    done
    app="$stage/Intavolatura.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    lipo -create -output "$app/Contents/MacOS/intavolatura" \
        target/aarch64-apple-darwin/release/intavolatura \
        target/x86_64-apple-darwin/release/intavolatura
    cp crates/intavolatura/fonts/OFL.txt "$app/Contents/Resources/"
    cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Intavolatura</string>
  <key>CFBundleDisplayName</key><string>Intavolatura</string>
  <key>CFBundleIdentifier</key><string>io.github.siebrenw.intavolatura</string>
  <key>CFBundleExecutable</key><string>intavolatura</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
    ditto -c -k --norsrc --noextattr --keepParent "$app" "$out/intavolatura-$version-macos.zip"
    ;;
linux)
    cargo build --release --locked -p intavolatura
    dir="$stage/intavolatura-$version"
    mkdir -p "$dir"
    cp target/release/intavolatura crates/intavolatura/fonts/OFL.txt "$dir/"
    tar -C "$stage" -czf "$out/intavolatura-$version-linux-x86_64.tar.gz" "intavolatura-$version"
    ;;
windows)
    cargo build --release --locked -p intavolatura
    dir="$stage/intavolatura-$version"
    mkdir -p "$dir"
    cp target/release/intavolatura.exe crates/intavolatura/fonts/OFL.txt "$dir/"
    (cd "$stage" && 7z a -tzip "$out/intavolatura-$version-windows-x86_64.zip" "intavolatura-$version" >/dev/null)
    ;;
*)
    echo "unknown platform: $platform (macos, linux or windows)" >&2
    exit 2
    ;;
esac
ls "$out"
