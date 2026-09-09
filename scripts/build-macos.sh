#!/bin/bash
# Compiles, bundles and verifies Kodem without launching either executable.
set -euo pipefail
KODEM_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$KODEM_ROOT"
configuration=${1:-debug}
[[ "$configuration" == debug || "$configuration" == release ]] || { echo "Usage: $0 debug|release" >&2; exit 2; }
mkdir -p target
build_lock="$KODEM_ROOT/target/.kodem-build-lock"
if ! mkdir "$build_lock" 2>/dev/null; then
    echo "Another Kodem build may be running. See $build_lock/pid. If that process has ended, remove only this lock directory and retry." >&2
    exit 1
fi
echo "$$" > "$build_lock/pid"
work_dir=$(mktemp -d -t kodem-build)
install_stage=""
destination=""
backup=""
install_pending=0
install_with_sudo=0
install_file() {
    if [[ "$install_with_sudo" == 1 ]]; then sudo "$@"; else "$@"; fi
}
cleanup() {
    if [[ "$install_pending" == 1 && ! -e "$destination" && -d "$backup" ]]; then
        install_file mv "$backup" "$destination" || echo "Restore the previous app from $backup" >&2
    fi
    if [[ -n "$install_stage" && -d "$install_stage" ]]; then install_file rm -rf "$install_stage"; fi
    rm -rf "$work_dir" "$build_lock"
}
trap cleanup EXIT
source "$KODEM_ROOT/scripts/dependencies.sh"
kodem_dependencies
echo "Compiling $configuration from $KODEM_ROOT"
cargo fmt --all -- --check
# Pin both the output root and target. Shell/global Cargo overrides must never
# make packaging read an older executable from another output directory.
build_target=aarch64-apple-darwin
if [[ "$configuration" == release ]]; then
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
if [[ "$configuration" == debug ]]; then app_name='Kodem Debug'; bundle_id=dev.kodem.ade.debug; fi
bundle="$work_dir/$app_name.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/licenses"
cp "$executables/kodem" "$bundle/Contents/MacOS/kodem"
cp "$executables/kodem-host" "$bundle/Contents/MacOS/kodem-host"
chmod 755 "$bundle/Contents/MacOS/kodem" "$bundle/Contents/MacOS/kodem-host"
cp assets/licenses/* "$bundle/Contents/Resources/licenses/"
cp assets/THIRD_PARTY.md "$bundle/Contents/Resources/THIRD_PARTY.md"
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
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>$build_date</string>
<key>KodemBuildID</key><string>$build_id</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PLIST
/usr/bin/plutil -lint "$bundle/Contents/Info.plist"
cat > "$bundle/Contents/Resources/BuildInfo.txt" <<INFO
Build: $build_id
Configuration: $configuration
Target: $build_target
Source: $KODEM_ROOT
INFO
/usr/bin/codesign --force --sign - "$bundle/Contents/MacOS/kodem-host"
/usr/bin/codesign --force --sign - "$bundle"
/usr/bin/codesign --verify --deep --strict "$bundle"
/usr/bin/file "$bundle/Contents/MacOS/kodem" "$bundle/Contents/MacOS/kodem-host"
# Keep a local app bundle for both configurations.
output="$KODEM_ROOT/target/$configuration/$app_name.app"
if [[ -e "$output" ]]; then
    existing_id=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$output/Contents/Info.plist" 2>/dev/null || true)
    [[ "$existing_id" == "$bundle_id" ]] || { echo "Refusing to replace an unrelated bundle: $output" >&2; exit 1; }
    rm -rf "$output"
fi
/usr/bin/ditto "$bundle" "$output"
/usr/bin/codesign --verify --deep --strict "$output"
echo "BUILD SUCCEEDED: $output"
echo "Build ID: $build_id"
if [[ "$configuration" == release ]]; then
    destination=/Applications/Kodem.app
    if [[ -e "$destination" ]]; then
        existing_id=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$destination/Contents/Info.plist" 2>/dev/null || true)
        [[ "$existing_id" == dev.kodem.ade ]] || { echo "Refusing to replace an unrelated application at $destination" >&2; exit 1; }
    fi
    # Keep rollback only until the replacement has passed verification.
    stage_candidate="/Applications/.Kodem-install-$$.app"
    [[ ! -e "$stage_candidate" ]] || { echo "Installation staging path already exists: $stage_candidate" >&2; exit 1; }
    install_stage="$stage_candidate"
    backup="/Applications/.Kodem-rollback-$$.app"
    [[ ! -e "$backup" ]] || { echo "Backup path already exists: $backup" >&2; exit 1; }
    install_with_sudo=0
    if [[ ! -w /Applications ]]; then echo "Administrator access is needed to install in /Applications."; sudo -v; install_with_sudo=1; fi
    install_file /usr/bin/ditto "$output" "$install_stage"
    if ! /usr/bin/codesign --verify --deep --strict "$install_stage"; then install_file rm -rf "$install_stage"; exit 1; fi
    install_pending=1
    if [[ -e "$destination" ]]; then install_file mv "$destination" "$backup"; fi
    if ! install_file mv "$install_stage" "$destination"; then
        if [[ -e "$backup" ]]; then install_file mv "$backup" "$destination"; fi
        install_file rm -rf "$install_stage"
        exit 1
    fi
    # The old bundle is moved intact, including executables used by live processes.
    # Installation does not terminate tasks or pretend a running process was refreshed.
    if ! /usr/bin/codesign --verify --deep --strict "$destination" \
        || ! /usr/bin/cmp -s "$output/Contents/MacOS/kodem" "$destination/Contents/MacOS/kodem" \
        || ! /usr/bin/cmp -s "$output/Contents/MacOS/kodem-host" "$destination/Contents/MacOS/kodem-host" \
        || ! /usr/bin/cmp -s "$output/Contents/Resources/AppIcon.icns" "$destination/Contents/Resources/AppIcon.icns" \
        || ! /usr/bin/cmp -s "$output/Contents/Info.plist" "$destination/Contents/Info.plist"; then
        install_file mv "$destination" "$install_stage"
        if [[ -e "$backup" ]]; then install_file mv "$backup" "$destination"; fi
        echo "Installation verification failed; the previous app was restored." >&2
        exit 1
    fi
    install_pending=0
    if [[ -d "$backup" ]]; then install_file rm -rf "$backup"; fi
    # Remove only legacy backups generated by this installer, never unrelated apps.
    for previous in /Applications/Kodem.previous.*.app; do
        [[ -d "$previous" && ! -L "$previous" ]] || continue
        [[ "${previous##*/}" =~ ^Kodem\.previous\.[0-9]{8}-[0-9]{6}-[0-9]+\.app$ ]] || continue
        previous_id=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$previous/Contents/Info.plist" 2>/dev/null || true)
        [[ "$previous_id" == dev.kodem.ade ]] || continue
        install_file rm -rf "$previous"
    done
    echo "INSTALL SUCCEEDED: $destination"
    echo "Verified build: $build_id"
    if pgrep -f '^/Applications/Kodem(\.app|\.previous\.[^/]+\.app)/Contents/MacOS/(kodem|kodem-host)( |$)' >/dev/null; then
        echo "RESTART REQUIRED: the app or host is still running its loaded build."
        echo "Finish active work, quit the old app, then open /Applications/Kodem.app. An idle older host upgrades automatically."
        echo "The new build is already installed. You do not need to build again."
    fi
fi
echo "Built and verified: $output"
echo "The application was not launched."
