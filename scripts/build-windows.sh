#!/bin/bash
# Compiles, signs, packages and verifies Kodem for Windows on x64 and Arm64 without launching
# either executable. Run it from Git Bash; the Build *.cmd files at the repository root do that.
#
#   build-windows.sh debug|nightly|production [all|compile|package|finish]
#
# Steps, all of them by default:
#   compile  builds each processor's package folder in target/<channel>/windows-<processor>/
#            and signs its executables.
#   package  Nightly and Production: writes Kodem-windows-<processor>.zip, the archive that
#            installed apps update from, and Kodem-windows-<processor>-setup.exe, the installer
#            for a first installation (NSIS, see scripts/windows-installer.nsi), and signs it.
#   finish   Nightly and Production: writes the .sha256 files and publishes, unless the release
#            workflow publishes.
# The release workflow runs the steps one at a time, so that SignPath signs between them.
#
# Processors: KODEM_WINDOWS_ARCH lists x86_64, aarch64 or both. Nightly and Production build
# both by default, cross-compiling the one that the build computer does not have; Debug builds
# the build computer's own. Each needs the Visual Studio C++ tools for that processor.
#
# Authenticode signing, in order of preference:
#   KODEM_SIGN_TRUSTED_SIGNING_METADATA  metadata.json of an Azure Trusted Signing account, with
#                                        KODEM_SIGN_TRUSTED_SIGNING_DLIB (Azure.CodeSigning.Dlib.dll)
#   KODEM_SIGN_CERT_SHA1                 thumbprint of a certificate in the user's store or on a
#                                        hardware token
#   KODEM_SIGN_PFX                       a .pfx file, with KODEM_SIGN_PFX_PASSWORD
# KODEM_SIGN_TIMESTAMP_URL overrides the RFC 3161 timestamp server. KODEM_SIGN_EXTERNALLY=1
# means that another step signs between the steps of this script, such as SignPath in the release
# workflow; each step then checks the signatures of the previous one. Without an identity, the
# executables stay unsigned, and SmartScreen warns on other computers. KODEM_REQUIRE_SIGNING=1
# refuses to build a release without one.
set -euo pipefail
KODEM_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$KODEM_ROOT"
channel=${1:-debug}
step=${2:-all}
usage() {
    echo "Usage: $0 debug|nightly|production [all|compile|package|finish]" >&2
    exit 2
}
case "$channel" in
    debug) configuration=debug ;;
    nightly|production) configuration=release ;;
    *) usage ;;
esac
case "$step" in
    all|compile) ;;
    package|finish) [[ "$channel" != debug ]] || usage ;;
    *) usage ;;
esac
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) echo "Run this build on Windows from Git Bash." >&2; exit 2 ;;
esac
# A required signature without an identity would fail only after compiling. Say so first.
if [[ "$channel" != debug && "${KODEM_REQUIRE_SIGNING:-0}" == 1 && "${KODEM_SIGN_EXTERNALLY:-0}" != 1 \
    && -z "${KODEM_SIGN_TRUSTED_SIGNING_METADATA:-}${KODEM_SIGN_CERT_SHA1:-}${KODEM_SIGN_PFX:-}" ]]; then
    echo "KODEM_REQUIRE_SIGNING is set, but no signing identity is configured (see the top of scripts/build-windows.sh)." >&2
    exit 1
fi
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
if [[ -n "${KODEM_WINDOWS_ARCH:-}" ]]; then
    architectures=$KODEM_WINDOWS_ARCH
elif [[ "$channel" == debug ]]; then
    architectures=$(kodem_windows_host_arch)
else
    architectures="x86_64 aarch64"
fi
for arch in $architectures; do
    [[ "$arch" == x86_64 || "$arch" == aarch64 ]] || { echo "KODEM_WINDOWS_ARCH lists $arch. Use x86_64, aarch64 or both." >&2; exit 2; }
done
export KODEM_WINDOWS_ARCHES="$architectures"
KODEM_BUILD_TARGETS=$(for arch in $architectures; do echo "$arch-pc-windows-msvc"; done)
export KODEM_BUILD_TARGETS
kodem_dependencies
source "$KODEM_ROOT/scripts/github-release.sh"
kodem_release_prepare "$channel"
export KODEM_RELEASE_CHANNEL="$channel" KODEM_APP_VERSION="$release_version" KODEM_RELEASE_PREVIOUS="$release_previous" KODEM_RELEASE_NOTES="$release_notes" KODEM_RELEASE_HIGHLIGHTS="$release_highlights"
python=$(kodem_python)
build_date=$(date -u +%Y.%m.%d)
build_id=$(date -u +%Y%m%dT%H%M%SZ)
build_revision=$(git rev-parse --short HEAD)
if ! git diff --quiet HEAD -- . ':!*.DS_Store'; then build_revision="$build_revision-dirty"; fi
build_id="$build_id-$build_revision"
# Names match updater::Channel::package_name and kodem_core::platform::app_id.
app_name=Kodem
app_id=dev.kodem.ade
if [[ "$channel" == debug ]]; then app_name='Kodem Debug'; app_id=dev.kodem.ade.debug; fi
if [[ "$channel" == nightly ]]; then app_name='Kodem Nightly'; app_id=dev.kodem.ade.nightly; fi

# Runs signtool without Git Bash rewriting its /options into paths.
signtool() {
    local tool
    tool=${KODEM_SIGNTOOL:-$(kodem_windows_sdk_tool signtool.exe)} || { echo "The Windows SDK is missing signtool.exe." >&2; return 1; }
    MSYS2_ARG_CONV_EXCL='*' "$tool" "$@"
}

# Signs one executable with the configured Authenticode identity and verifies the signature.
# Returns 2 when no identity is configured.
sign() {
    local file timestamp
    file=$(cygpath -w "$1")
    timestamp=${KODEM_SIGN_TIMESTAMP_URL:-http://timestamp.digicert.com}
    if [[ -n "${KODEM_SIGN_TRUSTED_SIGNING_METADATA:-}" ]]; then
        [[ -n "${KODEM_SIGN_TRUSTED_SIGNING_DLIB:-}" ]] || { echo "Set KODEM_SIGN_TRUSTED_SIGNING_DLIB to Azure.CodeSigning.Dlib.dll." >&2; return 1; }
        signtool sign /v /fd SHA256 /tr "${KODEM_SIGN_TIMESTAMP_URL:-http://timestamp.acs.microsoft.com}" /td SHA256 \
            /dlib "$(cygpath -w "$KODEM_SIGN_TRUSTED_SIGNING_DLIB")" /dmdf "$(cygpath -w "$KODEM_SIGN_TRUSTED_SIGNING_METADATA")" "$file"
    elif [[ -n "${KODEM_SIGN_CERT_SHA1:-}" ]]; then
        signtool sign /v /fd SHA256 /tr "$timestamp" /td SHA256 /sha1 "$KODEM_SIGN_CERT_SHA1" "$file"
    elif [[ -n "${KODEM_SIGN_PFX:-}" ]]; then
        signtool sign /v /fd SHA256 /tr "$timestamp" /td SHA256 /f "$(cygpath -w "$KODEM_SIGN_PFX")" /p "${KODEM_SIGN_PFX_PASSWORD:-}" "$file"
    else
        return 2
    fi
    signtool verify /pa /v "$file" >/dev/null
}

# Checks that an executable was built for the expected processor, from its PE header.
check_machine() {
    "$python" - "$(cygpath -w "$1")" "$2" <<'PYTHON'
import struct, sys
data = open(sys.argv[1], "rb").read(4096)
offset = struct.unpack_from("<I", data, 0x3C)[0]
machine = struct.unpack_from("<H", data, offset + 4)[0]
expected = {"x86_64": 0x8664, "aarch64": 0xAA64}[sys.argv[2]]
sys.exit(0 if data[offset:offset + 4] == b"PE\0\0" and machine == expected else 1)
PYTHON
}
# Signs a file with the configured identity, unless another step signs it. Without either, the
# file stays unsigned, which KODEM_REQUIRE_SIGNING=1 refuses for a release.
sign_or_report() {
    local status=0
    [[ "${KODEM_SIGN_EXTERNALLY:-0}" != 1 ]] || return 0
    sign "$1" || status=$?
    if [[ "$status" == 2 ]]; then
        if [[ "$channel" != debug && "${KODEM_REQUIRE_SIGNING:-0}" == 1 ]]; then
            echo "KODEM_REQUIRE_SIGNING is set, but no signing identity is configured." >&2
            return 1
        fi
        echo "Not signed: $1. Windows SmartScreen warns when it runs on another computer."
        return 0
    fi
    [[ "$status" == 0 ]] || return "$status"
    echo "Signed and verified $1"
}

# Fails unless a file carries a valid signature, when this release requires signatures.
check_signature() {
    [[ "${KODEM_SIGN_EXTERNALLY:-0}" == 1 || "${KODEM_REQUIRE_SIGNING:-0}" == 1 ]] || return 0
    signtool verify /pa "$(cygpath -w "$1")" >/dev/null || { echo "$1 has no valid signature." >&2; return 1; }
}

# Prints the version in a package folder's release.json.
package_version() {
    sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1/release.json" 2>/dev/null || true
}

# The Windows editions whose processor each package runs on.
declare -A requirements=(
    [x86_64]="Windows 10 version 1809 or newer on x64"
    [aarch64]="Windows 10 version 1809 or newer on Arm64, or Windows 11 on Arm"
)

# Builds one processor's package folder in target/<channel>/windows-<processor>/ and signs its
# executables.
compile_step() {
    local arch=$1 build_target executables package executable output existing_id
    # Pin both the output root and target. Shell/global Cargo overrides must never
    # make packaging read an older executable from another output directory.
    build_target="$arch-pc-windows-msvc"
    echo "Building $build_target"
    "$python" scripts/licenses.py prepare --target "$build_target"
    if [[ "$configuration" == release ]]; then
        kodem_release_rustflags
        cargo build --workspace --release --locked --target "$build_target" --target-dir "$KODEM_ROOT/target"
    else
        cargo build --workspace --locked --target "$build_target" --target-dir "$KODEM_ROOT/target"
    fi
    executables="$KODEM_ROOT/target/$build_target/$configuration"
    package="$work_dir/$arch/$app_name"
    mkdir -p "$package/licenses"
    for executable in kodem.exe kodem-host.exe kodem-update.exe; do
        cp "$executables/$executable" "$package/$executable"
        check_machine "$package/$executable" "$arch" || { echo "$executable is not a $arch Windows program." >&2; exit 1; }
    done
    cp LICENSE "$package/LICENSE"
    cp assets/THIRD_PARTY.md "$package/THIRD_PARTY.md"
    # The tracked notices describe the macOS dependency graph. This target's notices replace them.
    cp assets/licenses/* "$package/licenses/"
    rm -f "$package/licenses/Rust-THIRD-PARTY.txt" "$package/licenses/Rust-standard-library.html" "$package/licenses/option-ext-0.2.0-source.tar.gz"
    cp "target/licensing/$build_target/assets/licenses/"* "$package/licenses/"
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
Requires: ${requirements[$arch]}
INFO
    "$python" -c 'import json, sys; json.load(open(sys.argv[1]))' "$(cygpath -w "$package/release.json")"
    for executable in kodem.exe kodem-host.exe kodem-update.exe; do
        sign_or_report "$package/$executable"
    done
    # Keep a local package for each channel and processor.
    output="$KODEM_ROOT/target/$channel/windows-$arch/$app_name"
    if [[ -e "$output" ]]; then
        existing_id=$(sed -n 's/^[[:space:]]*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$output/release.json" 2>/dev/null || true)
        [[ "$existing_id" == "$app_id" ]] || { echo "Refusing to replace an unrelated package: $output" >&2; exit 1; }
        rm -rf "$output"
    fi
    mkdir -p "$(dirname "$output")"
    cp -R -p "$package" "$output"
    echo "BUILD SUCCEEDED: $output"
}

# Writes one processor's update archive and installer from its package folder, and signs the
# installer.
package_step() {
    local arch=$1 folder archive installer executable windows_tar entry icon makensis
    folder="$KODEM_ROOT/target/$channel/windows-$arch/$app_name"
    [[ "$(package_version "$folder")" == "$release_version" ]] || { echo "$folder is not the $release_version package. Run the compile step first." >&2; exit 1; }
    for executable in kodem.exe kodem-host.exe kodem-update.exe; do
        check_signature "$folder/$executable"
    done
    # The archive's name matches updater::asset_name on that processor.
    archive="$KODEM_ROOT/target/$channel/Kodem-windows-$arch.zip"
    installer="$KODEM_ROOT/target/$channel/Kodem-windows-$arch-setup.exe"
    rm -f "$archive" "$archive.sha256" "$installer" "$installer.sha256"
    # Windows' own bsdtar writes the ZIP archive that the updater lists and extracts.
    windows_tar="$(cygpath -u "$SYSTEMROOT")/System32/tar.exe"
    (cd "$(dirname "$folder")" && "$windows_tar" -a -c -f "$(cygpath -w "$archive")" "$app_name")
    while IFS= read -r entry; do
        [[ "$entry" == "$app_name/"* ]] || { echo "Unexpected archive entry: $entry" >&2; exit 1; }
    done < <("$windows_tar" -t -f "$(cygpath -w "$archive")" | tr -d '\r')
    # Cargo keeps the icon that crates/kodem-desktop/build.rs makes in that build script's
    # output folder; the newest one comes from the latest build.
    icon=$(ls -t "$KODEM_ROOT/target/$arch-pc-windows-msvc/release/build/"kodem-desktop-*/out/kodem.ico 2>/dev/null | head -1 || true)
    [[ -n "$icon" ]] || { echo "The $arch build left no app icon for the installer." >&2; exit 1; }
    makensis=$(kodem_makensis)
    MSYS2_ARG_CONV_EXCL='*' "$makensis" -V2 \
        "-DAPP_NAME=$app_name" "-DAPP_ID=$app_id" "-DVERSION=$release_version" "-DARCH=$arch" \
        "-DPACKAGE=$(cygpath -w "$folder")" "-DOUTFILE=$(cygpath -w "$installer")" "-DICON=$(cygpath -w "$icon")" \
        "$(cygpath -w "$KODEM_ROOT/scripts/windows-installer.nsi")"
    [[ -f "$installer" ]] || { echo "makensis did not write $installer" >&2; exit 1; }
    sign_or_report "$installer"
    echo "PACKAGED: $archive and $installer"
}

# Writes the checksums of one processor's archive and installer, and publishes them unless the
# release workflow publishes.
finish_step() {
    local arch=$1 archive installer
    archive="$KODEM_ROOT/target/$channel/Kodem-windows-$arch.zip"
    installer="$KODEM_ROOT/target/$channel/Kodem-windows-$arch-setup.exe"
    [[ -f "$archive" && -f "$installer" ]] || { echo "$archive or $installer is missing. Run the package step first." >&2; exit 1; }
    check_signature "$installer"
    kodem_write_sha256 "$archive"
    kodem_write_sha256 "$installer"
    if kodem_release_publishes "$channel"; then
        kodem_release_publish "$channel" "$archive" "$archive.sha256" "$installer" "$installer.sha256"
        # The next processor adds its files to the release that this one published.
        release_existing=$release_tag
    else
        echo "READY: $archive and $installer (the release workflow publishes them)"
    fi
}

if [[ "$step" == all || "$step" == compile ]]; then
    echo "Compiling $channel $release_version for $architectures from $KODEM_ROOT"
    cargo fmt --all -- --check
fi
for arch in $architectures; do
    case "$step" in
        all)
            compile_step "$arch"
            if [[ "$channel" != debug ]]; then
                package_step "$arch"
                finish_step "$arch"
            fi
            ;;
        compile) compile_step "$arch" ;;
        package) package_step "$arch" ;;
        finish) finish_step "$arch" ;;
    esac
done
if [[ "$step" == all || "$step" == compile ]]; then
    echo "Build ID: $build_id"
fi
if [[ "$channel" == debug ]]; then
    echo "Run it from target/debug/windows-<processor>/$app_name/kodem.exe."
fi
echo "Finished $step for $architectures"
echo "The application was not launched."
