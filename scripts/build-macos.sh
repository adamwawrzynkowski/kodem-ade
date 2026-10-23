#!/bin/bash
# Compiles, bundles and verifies Kodem without launching either executable. Nightly and
# Production also write Kodem-macos-aarch64.zip, the archive that installed apps update from,
# and Kodem-macos-aarch64.dmg, the disk image for a first installation.
set -euo pipefail
KODEM_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$KODEM_ROOT"
channel=${1:-debug}
case "$channel" in
    debug) configuration=debug ;;
    nightly|production) configuration=release ;;
    *) echo "Usage: $0 debug|nightly|production" >&2; exit 2 ;;
esac
[[ "$(uname -s)" == Darwin ]] || { echo "Run this build on macOS. Use scripts/build-linux.sh or scripts/build-windows.sh elsewhere." >&2; exit 2; }
mkdir -p target
build_lock="$KODEM_ROOT/target/.kodem-build-lock"
if ! mkdir "$build_lock" 2>/dev/null; then
    echo "Another Kodem build may be running. See $build_lock/pid. If that process has ended, remove only this lock directory and retry." >&2
    exit 1
fi
echo "$$" > "$build_lock/pid"
work_dir=$(mktemp -d -t kodem-build)
cleanup() {
    rm -rf "$work_dir" "$build_lock"
}
trap cleanup EXIT
source "$KODEM_ROOT/scripts/dependencies.sh"
kodem_dependencies
source "$KODEM_ROOT/scripts/github-release.sh"
kodem_release_prepare "$channel"
export KODEM_RELEASE_CHANNEL="$channel" KODEM_APP_VERSION="$release_version" KODEM_RELEASE_PREVIOUS="$release_previous" KODEM_RELEASE_NOTES="$release_notes" KODEM_RELEASE_HIGHLIGHTS="$release_highlights"
echo "Compiling $channel $release_version from $KODEM_ROOT"
cargo fmt --all -- --check
# Pin both the output root and target. Shell/global Cargo overrides must never
# make packaging read an older executable from another output directory.
build_target=aarch64-apple-darwin
python3 scripts/licenses.py prepare --target "$build_target"
if [[ "$configuration" == release ]]; then
    kodem_release_rustflags
    cargo build --workspace --release --locked --target "$build_target" --target-dir "$KODEM_ROOT/target"
else
    cargo build --workspace --locked --target "$build_target" --target-dir "$KODEM_ROOT/target"
fi
executables="$KODEM_ROOT/target/$build_target/$configuration"
build_date=$(date -u +%Y.%m.%d)
build_id=$(date -u +%Y%m%dT%H%M%SZ)
build_revision=$(git rev-parse --short HEAD)
if ! git diff --quiet HEAD -- . ':!*.DS_Store'; then build_revision="$build_revision-dirty"; fi
build_id="$build_id-$build_revision"
app_name=Kodem
bundle_id=dev.kodem.ade
if [[ "$channel" == debug ]]; then app_name='Kodem Debug'; bundle_id=dev.kodem.ade.debug; fi
if [[ "$channel" == nightly ]]; then app_name='Kodem Nightly'; bundle_id=dev.kodem.ade.nightly; fi
bundle="$work_dir/$app_name.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/licenses"
cp "$executables/kodem" "$bundle/Contents/MacOS/kodem"
cp "$executables/kodem-host" "$bundle/Contents/MacOS/kodem-host"
cp "$executables/kodem-update" "$bundle/Contents/MacOS/kodem-update"
chmod 755 "$bundle/Contents/MacOS/kodem" "$bundle/Contents/MacOS/kodem-host" "$bundle/Contents/MacOS/kodem-update"
cp LICENSE "$bundle/Contents/Resources/LICENSE"
cp assets/licenses/* "$bundle/Contents/Resources/licenses/"
cp target/licensing/assets/licenses/* "$bundle/Contents/Resources/licenses/"
cp assets/THIRD_PARTY.md "$bundle/Contents/Resources/THIRD_PARTY.md"
cp assets/sounds/kodem-*.wav "$bundle/Contents/Resources/"
icon_source="$KODEM_ROOT/assets/app/AppIcon_1024x1024.png"
[[ -f "$icon_source" ]] || { echo "Missing app icon: $icon_source" >&2; exit 1; }
iconset="$work_dir/AppIcon.iconset"
mkdir -p "$iconset"
for points in 16 32 128 256 512; do
    /usr/bin/sips -z "$points" "$points" "$icon_source" --out "$iconset/icon_${points}x${points}.png" >/dev/null
    pixels=$((points * 2))
    /usr/bin/sips -z "$pixels" "$pixels" "$icon_source" --out "$iconset/icon_${points}x${points}@2x.png" >/dev/null
done
/usr/bin/iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/AppIcon.icns"

cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>$app_name</string>
<key>CFBundleDisplayName</key><string>$app_name</string>
<key>CFBundleIdentifier</key><string>$bundle_id</string>
<key>CFBundleExecutable</key><string>kodem</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$release_version</string>
<key>CFBundleVersion</key><string>$build_date</string>
<key>KodemReleaseChannel</key><string>$channel</string>
<key>KodemReleaseRepository</key><string>$release_repository</string>
<key>KodemBuildID</key><string>$build_id</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSMicrophoneUsageDescription</key><string>Voice input records your prompt and transcribes it on this Mac.</string>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PLIST
/usr/bin/plutil -lint "$bundle/Contents/Info.plist"
cat > "$bundle/Contents/Resources/BuildInfo.txt" <<INFO
Build: $build_id
Configuration: $channel
Version: $release_version
Target: $build_target
INFO
# UserNotifications checks the sender's code identity against the requested bundle.
# An automatically generated helper identifier cannot post for the parent app.
/usr/bin/codesign --force --sign - --identifier "$bundle_id" "$bundle/Contents/MacOS/kodem-host"
/usr/bin/codesign --force --sign - "$bundle/Contents/MacOS/kodem-update"
/usr/bin/codesign --force --sign - "$bundle"
/usr/bin/codesign --verify --deep --strict "$bundle"
host_identity=$(/usr/bin/codesign -d --verbose=2 "$bundle/Contents/MacOS/kodem-host" 2>&1 | /usr/bin/sed -n 's/^Identifier=//p')
[[ "$host_identity" == "$bundle_id" ]] || { echo "Host notification identity does not match $bundle_id" >&2; exit 1; }
/usr/bin/file "$bundle/Contents/MacOS/kodem" "$bundle/Contents/MacOS/kodem-host" "$bundle/Contents/MacOS/kodem-update"
# Keep a local app bundle for each channel.
output="$KODEM_ROOT/target/$channel/$app_name.app"
if [[ -e "$output" ]]; then
    existing_id=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$output/Contents/Info.plist" 2>/dev/null || true)
    [[ "$existing_id" == "$bundle_id" ]] || { echo "Refusing to replace an unrelated bundle: $output" >&2; exit 1; }
    rm -rf "$output"
fi
mkdir -p "$(dirname "$output")"
/usr/bin/ditto "$bundle" "$output"
/usr/bin/codesign --verify --deep --strict "$output"
echo "BUILD SUCCEEDED: $output"
echo "Build ID: $build_id"
if [[ "$channel" != debug ]]; then
    archive="$KODEM_ROOT/target/$channel/Kodem-macos-aarch64.zip"
    disk_image="$KODEM_ROOT/target/$channel/Kodem-macos-aarch64.dmg"
    rm -f "$archive" "$archive.sha256" "$disk_image" "$disk_image.sha256"
    /usr/bin/ditto -c -k --sequesterRsrc --keepParent "$output" "$archive"
    # The disk image installs by drag and drop: the app beside a link to Applications.
    image_root="$work_dir/disk-image"
    mkdir "$image_root"
    /usr/bin/ditto "$output" "$image_root/$app_name.app"
    ln -s /Applications "$image_root/Applications"
    # hdiutil fails now and then while the system still uses a new volume. Retry a few times.
    for attempt in 1 2 3; do
        if /usr/bin/hdiutil create -volname "$app_name" -srcfolder "$image_root" -fs HFS+ -format UDZO -ov "$disk_image" >/dev/null; then break; fi
        [[ "$attempt" -lt 3 ]] || { echo "hdiutil could not create $disk_image" >&2; exit 1; }
        sleep 5
    done
    /usr/bin/hdiutil verify "$disk_image" >/dev/null
    kodem_write_sha256 "$archive"
    kodem_write_sha256 "$disk_image"
    if kodem_release_publishes "$channel"; then
        kodem_release_publish "$channel" "$archive" "$archive.sha256" "$disk_image" "$disk_image.sha256"
    else
        echo "PACKAGED: $archive and $disk_image (the release workflow publishes them)"
    fi
fi
echo "Built and verified: $output"
echo "The application was not launched."
