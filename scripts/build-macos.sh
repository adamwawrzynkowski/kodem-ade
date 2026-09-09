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
install_with_sudo=0
install_file() {
    if [[ "$install_with_sudo" == 1 ]]; then sudo "$@"; else "$@"; fi
}
cleanup() {
    if [[ -n "$install_stage" && -d "$install_stage" ]]; then install_file rm -rf "$install_stage"; fi
    rm -rf "$work_dir" "$build_lock"
}
trap cleanup EXIT
source "$KODEM_ROOT/scripts/dependencies.sh"
kodem_dependencies
cargo fmt --all -- --check
if [[ "$configuration" == release ]]; then cargo build --workspace --release --locked; else cargo build --workspace --locked; fi
app_name=Kodem
bundle_id=dev.kodem.ade
if [[ "$configuration" == debug ]]; then app_name='Kodem Debug'; bundle_id=dev.kodem.ade.debug; fi
bundle="$work_dir/$app_name.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/licenses"
cp "target/$configuration/kodem" "$bundle/Contents/MacOS/kodem"
cp "target/$configuration/kodem-host" "$bundle/Contents/MacOS/kodem-host"
chmod 755 "$bundle/Contents/MacOS/kodem" "$bundle/Contents/MacOS/kodem-host"
cp assets/licenses/* "$bundle/Contents/Resources/licenses/"
cp assets/THIRD_PARTY.md "$bundle/Contents/Resources/THIRD_PARTY.md"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>$app_name</string>
<key>CFBundleDisplayName</key><string>$app_name</string>
<key>CFBundleIdentifier</key><string>$bundle_id</string>
<key>CFBundleExecutable</key><string>kodem</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PLIST
/usr/bin/plutil -lint "$bundle/Contents/Info.plist"
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
if [[ "$configuration" == release ]]; then
    destination=/Applications/Kodem.app
    if [[ -e "$destination" ]]; then
        existing_id=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$destination/Contents/Info.plist" 2>/dev/null || true)
        [[ "$existing_id" == dev.kodem.ade ]] || { echo "Refusing to replace an unrelated application at $destination" >&2; exit 1; }
        if pgrep -f '^/Applications/Kodem.app/Contents/MacOS/(kodem|kodem-host)( |$)' >/dev/null; then
            echo "Kodem or its background host is still running. Finish active tasks, use Stop idle host and quit, then rerun this command." >&2
            exit 1
        fi
    fi
    # Stage on the same filesystem, verify, then move the previous installation to a backup.
    stage_candidate="/Applications/.Kodem-install-$$.app"
    [[ ! -e "$stage_candidate" ]] || { echo "Installation staging path already exists: $stage_candidate" >&2; exit 1; }
    install_stage="$stage_candidate"
    backup="/Applications/Kodem.previous.$(date +%Y%m%d-%H%M%S).app"
    install_with_sudo=0
    if [[ ! -w /Applications ]]; then echo "Administrator access is needed to install in /Applications."; sudo -v; install_with_sudo=1; fi
    install_file /usr/bin/ditto "$output" "$install_stage"
    if ! /usr/bin/codesign --verify --deep --strict "$install_stage"; then install_file rm -rf "$install_stage"; exit 1; fi
    if [[ -e "$destination" ]]; then install_file mv "$destination" "$backup"; fi
    if ! install_file mv "$install_stage" "$destination"; then
        if [[ -e "$backup" ]]; then install_file mv "$backup" "$destination"; fi
        install_file rm -rf "$install_stage"
        exit 1
    fi
    echo "Installed: $destination"
    if [[ -e "$backup" ]]; then echo "Previous version: $backup"; fi
fi
echo "Built and verified: $output"
echo "The application was not launched."
