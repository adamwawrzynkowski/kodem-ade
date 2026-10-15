#!/bin/bash
# Compiles, packages and verifies Kodem for x86_64 Linux without launching either executable.
# Build on the oldest distribution you support: the package needs that glibc or newer.
set -euo pipefail
KODEM_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$KODEM_ROOT"
channel=${1:-debug}
case "$channel" in
    debug) configuration=debug ;;
    nightly|production) configuration=release ;;
    *) echo "Usage: $0 debug|nightly|production" >&2; exit 2 ;;
esac
[[ "$(uname -s)" == Linux ]] || { echo "Run this build on Linux." >&2; exit 2; }
mkdir -p target
build_lock="$KODEM_ROOT/target/.kodem-build-lock"
if ! mkdir "$build_lock" 2>/dev/null; then
    echo "Another Kodem build may be running. See $build_lock/pid. If that process has ended, remove only this lock directory and retry." >&2
    exit 1
fi
echo "$$" > "$build_lock/pid"
work_dir=$(mktemp -d -t kodem-build.XXXXXX)
cleanup() {
    rm -rf "$work_dir" "$build_lock"
}
trap cleanup EXIT
source "$KODEM_ROOT/scripts/dependencies.sh"
kodem_dependencies
source "$KODEM_ROOT/scripts/github-release.sh"
kodem_release_prepare "$channel"
export KODEM_RELEASE_CHANNEL="$channel" KODEM_APP_VERSION="$release_version" KODEM_RELEASE_PREVIOUS="$release_previous" KODEM_RELEASE_NOTES="$release_notes"
echo "Compiling $channel $release_version from $KODEM_ROOT"
cargo fmt --all -- --check
# Pin both the output root and target. Shell/global Cargo overrides must never
# make packaging read an older executable from another output directory.
build_target=$(kodem_build_target)
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
# Names match updater::Channel::package_name and kodem_core::platform::app_id.
app_name=Kodem
package_name=kodem
app_id=dev.kodem.ade
if [[ "$channel" == debug ]]; then app_name='Kodem Debug'; package_name=kodem-debug; app_id=dev.kodem.ade.debug; fi
if [[ "$channel" == nightly ]]; then app_name='Kodem Nightly'; package_name=kodem-nightly; app_id=dev.kodem.ade.nightly; fi
package="$work_dir/$package_name"
mkdir -p "$package/licenses"
install -m 755 "$executables/kodem" "$package/kodem"
install -m 755 "$executables/kodem-host" "$package/kodem-host"
install -m 755 scripts/linux-install.sh "$package/install.sh"
install -m 644 assets/app/AppIcon_1024x1024.png "$package/kodem.png"
cp LICENSE "$package/LICENSE"
cp assets/THIRD_PARTY.md "$package/THIRD_PARTY.md"
# The tracked notices describe the macOS dependency graph. This target's notices replace them.
cp assets/licenses/* "$package/licenses/"
rm -f "$package/licenses/Rust-THIRD-PARTY.txt" "$package/licenses/Rust-standard-library.html" "$package/licenses/option-ext-0.2.0-source.tar.gz"
cp "target/licensing/$build_target/assets/licenses/"* "$package/licenses/"
glibc=$(objdump -T "$package/kodem" "$package/kodem-host" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
cat > "$package/release.json" <<JSON
{
  "name": "$app_name",
  "identifier": "$app_id",
  "version": "$release_version",
  "channel": "$channel",
  "repository": "$release_repository",
  "build": "$build_id"
}
JSON
cat > "$package/BuildInfo.txt" <<INFO
Build: $build_id
Configuration: $channel
Version: $release_version
Date: $build_date
Target: $build_target
Requires: glibc $glibc or newer, libxkbcommon, libxkbcommon-x11, fontconfig, freetype, and Vulkan or OpenGL drivers
INFO
python3 -c 'import json, sys; json.load(open(sys.argv[1]))' "$package/release.json"
# Every shared library must resolve on the build system; the package bundles none.
for executable in kodem kodem-host; do
    if ldd "$package/$executable" | grep -q 'not found'; then
        ldd "$package/$executable" >&2
        echo "$executable links a library that is not installed." >&2
        exit 1
    fi
done
file "$package/kodem" "$package/kodem-host"
echo "Requires glibc $glibc or newer."
# Keep a local package for each channel.
output="$KODEM_ROOT/target/$channel/$package_name"
if [[ -e "$output" ]]; then
    existing_id=$(sed -n 's/^[[:space:]]*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$output/release.json" 2>/dev/null || true)
    [[ "$existing_id" == "$app_id" ]] || { echo "Refusing to replace an unrelated package: $output" >&2; exit 1; }
    rm -rf "$output"
fi
mkdir -p "$(dirname "$output")"
cp -R -p "$package" "$output"
echo "BUILD SUCCEEDED: $output"
echo "Build ID: $build_id"
echo "Install it for this user with: $output/install.sh"
if [[ "$channel" != debug ]]; then
    archive="$KODEM_ROOT/target/$channel/Kodem-linux-x86_64.tar.gz"
    rm -f "$archive" "$archive.sha256" "$archive.asc"
    tar -C "$(dirname "$output")" --owner=0 --group=0 --numeric-owner -czf "$archive" "$package_name"
    (cd "$(dirname "$archive")" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
    signatures=("$archive.sha256")
    # Linux runs unsigned programs. A detached OpenPGP signature lets users verify the publisher.
    if [[ -n "${KODEM_GPG_KEY:-}" ]]; then
        gpg --batch --yes --local-user "$KODEM_GPG_KEY" --armor --detach-sign --output "$archive.asc" "$archive"
        gpg --batch --verify "$archive.asc" "$archive"
        signatures+=("$archive.asc")
    elif [[ "${KODEM_REQUIRE_SIGNING:-0}" == 1 ]]; then
        echo "KODEM_REQUIRE_SIGNING is set, but KODEM_GPG_KEY names no signing key." >&2
        exit 1
    else
        echo "Not signed: set KODEM_GPG_KEY to publish a detached OpenPGP signature."
    fi
    if kodem_release_publishes "$channel"; then
        kodem_release_publish "$channel" "$archive" "${signatures[@]}"
    else
        echo "PACKAGED: $archive (the release workflow publishes it)"
    fi
fi
echo "Built and verified: $output"
echo "The application was not launched."
