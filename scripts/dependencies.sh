#!/bin/bash
# Sourced by the build commands. Installs only Kodem's prerequisites on macOS, Linux and
# Windows (Git Bash).
set -euo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:${PATH:-}"
KODEM_RUST_VERSION=1.97.1
KODEM_DEPENDENCIES_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
KODEM_CODEX_MIN_VERSION=$(cat "$KODEM_DEPENDENCIES_DIR/../codex-min-version")

# Names the build platform: macos, linux or windows.
kodem_platform() {
    case "$(uname -s)" in
        Darwin) echo macos ;;
        Linux) echo linux ;;
        MINGW*|MSYS*|CYGWIN*) echo windows ;;
        *) echo "Unsupported build platform: $(uname -s)" >&2; return 1 ;;
    esac
}
KODEM_PLATFORM=$(kodem_platform)
KODEM_EXE=""
[[ "$KODEM_PLATFORM" != windows ]] || KODEM_EXE=.exe

# Returns the Rust target that this platform's build produces.
kodem_build_target() {
    case "$KODEM_PLATFORM" in
        macos) echo aarch64-apple-darwin ;;
        linux) echo x86_64-unknown-linux-gnu ;;
        windows) echo x86_64-pc-windows-msvc ;;
    esac
}

# Returns the Codex package target that matches this platform.
kodem_codex_target() {
    case "$KODEM_PLATFORM" in
        macos) echo aarch64-apple-darwin ;;
        linux) echo x86_64-unknown-linux-musl ;;
        windows) echo x86_64-pc-windows-msvc ;;
    esac
}

# Returns the folder where Kodem looks for installed Codex packages. It matches
# kodem_core::providers::codex::runtimes_dir.
kodem_runtimes_dir() {
    if [[ "$KODEM_PLATFORM" == windows ]]; then
        echo "$(cygpath -u "${LOCALAPPDATA:?LOCALAPPDATA is unset}")/kodem/runtimes"
    else
        echo "$HOME/.local/share/kodem/runtimes"
    fi
}

# Prints the SHA-256 digest of a file.
kodem_sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d ' ' -f 1
    else /usr/bin/shasum -a 256 "$1" | cut -d ' ' -f 1; fi
}

kodem_dependencies() {
    case "$KODEM_PLATFORM" in
        macos) kodem_dependencies_macos ;;
        linux) kodem_dependencies_linux ;;
        windows) kodem_dependencies_windows ;;
    esac
    kodem_rust
    kodem_codex
    # Cargo installs every pinned Rust dependency, including bundled SQLite.
    cargo fetch --locked
}

kodem_dependencies_macos() {
    [[ "$(uname -m)" == arm64 ]] || { echo "This release targets Apple Silicon. An Intel build has not been validated." >&2; return 1; }
    local macos_version
    macos_version=$(/usr/bin/sw_vers -productVersion)
    [[ "${macos_version%%.*}" -ge 14 ]] || { echo "Kodem requires macOS 14 or newer." >&2; return 1; }
    command -v curl >/dev/null || { echo "macOS curl is unavailable." >&2; return 1; }
    if ! xcrun --find clang >/dev/null 2>&1 || ! xcrun --show-sdk-path >/dev/null 2>&1; then
        echo "Installing Apple Command Line Tools. macOS may ask for your administrator password."
        local marker=/tmp/.com.apple.dt.CommandLineTools.installondemand.in-progress
        local made_marker=0
        if [[ ! -e "$marker" ]]; then touch "$marker"; made_marker=1; fi
        local label
        label=$(/usr/sbin/softwareupdate --list 2>&1 | /usr/bin/awk '/Label: Command Line Tools/ {sub(/^.*Label: /, ""); label=$0} END {print label}') || label=""
        if [[ "$made_marker" == 1 ]]; then rm -f "$marker"; fi
        if [[ -z "$label" ]]; then
            echo "Apple did not offer a Command Line Tools package. Starting Apple's installer. Rerun this build when installation finishes."
            /usr/bin/xcode-select --install || true
            return 1
        fi
        sudo /usr/sbin/softwareupdate --install "$label" --verbose
    fi
    xcrun --find clang >/dev/null
    xcrun --show-sdk-path >/dev/null
    command -v git >/dev/null || { echo "Git is missing after the Apple tools installation." >&2; return 1; }
}

# Checks the compiler and development libraries that GPUI links on Linux. Package managers
# need root, so the build names the packages instead of installing them.
kodem_dependencies_linux() {
    [[ "$(uname -m)" == x86_64 ]] || { echo "This release targets x86_64 Linux. Another architecture has not been validated." >&2; return 1; }
    local missing=()
    local tool
    for tool in curl git cc pkg-config python3 tar gzip objdump; do
        command -v "$tool" >/dev/null || missing+=("$tool")
    done
    if command -v pkg-config >/dev/null; then
        local library
        for library in xkbcommon xkbcommon-x11 fontconfig freetype2 xcb; do
            pkg-config --exists "$library" || missing+=("$library (development files)")
        done
    fi
    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "Missing build prerequisites: ${missing[*]}" >&2
        echo "Debian or Ubuntu: sudo apt install build-essential binutils curl git pkg-config python3 libxkbcommon-dev libxkbcommon-x11-dev libfontconfig-dev libfreetype-dev libxcb1-dev libwayland-dev libvulkan1" >&2
        echo "Fedora: sudo dnf install gcc gcc-c++ curl git pkgconf-pkg-config python3 libxkbcommon-devel libxkbcommon-x11-devel fontconfig-devel freetype-devel libxcb-devel wayland-devel vulkan-loader" >&2
        echo "Arch: sudo pacman -S --needed base-devel curl git pkgconf python libxkbcommon libxkbcommon-x11 fontconfig freetype2 libxcb wayland vulkan-icd-loader" >&2
        return 1
    fi
}

# Checks the Visual Studio C++ tools and the Windows SDK. GPUI compiles its shaders with the
# SDK's fxc.exe in release builds, and the SDK's rc.exe embeds the icon and manifest.
kodem_dependencies_windows() {
    [[ "$(uname -m)" == x86_64 ]] || { echo "This release targets x86_64 Windows. Another architecture has not been validated." >&2; return 1; }
    command -v git >/dev/null || { echo "Install Git for Windows and run this command from Git Bash." >&2; return 1; }
    command -v curl >/dev/null || { echo "curl is unavailable. Use Windows 10 1803 or newer." >&2; return 1; }
    kodem_python >/dev/null || { echo "Install Python 3 from python.org; license notices need it." >&2; return 1; }
    local vswhere
    vswhere="$(kodem_program_files_x86)/Microsoft Visual Studio/Installer/vswhere.exe"
    if [[ ! -f "$vswhere" ]] || [[ -z "$("$vswhere" -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>/dev/null)" ]]; then
        echo "Install Visual Studio 2022 Build Tools with the 'Desktop development with C++' workload and a Windows 11 SDK:" >&2
        echo "  winget install Microsoft.VisualStudio.2022.BuildTools --override \"--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended\"" >&2
        return 1
    fi
    kodem_windows_sdk_tool fxc.exe >/dev/null || { echo "The Windows SDK is missing fxc.exe. Add a Windows 11 SDK in the Visual Studio Installer." >&2; return 1; }
}

# Prints the 32-bit Program Files folder, where Visual Studio and the Windows SDK install.
kodem_program_files_x86() {
    cygpath -u "$(printenv 'ProgramFiles(x86)' || echo 'C:\Program Files (x86)')"
}

# Prints the newest x64 Windows SDK tool with the given name.
kodem_windows_sdk_tool() {
    local root
    root="$(kodem_program_files_x86)/Windows Kits/10/bin"
    local found
    found=$(find "$root" -maxdepth 3 -path "*/x64/$1" 2>/dev/null | sort -V | tail -1)
    [[ -n "$found" ]] || return 1
    echo "$found"
}

# Prints the command that runs Python 3. Windows installs it as python or py, and its
# python3 can be a Microsoft Store placeholder.
kodem_python() {
    local candidate
    for candidate in python3 python py; do
        if command -v "$candidate" >/dev/null && "$candidate" -c 'import sys; sys.exit(sys.version_info < (3, 8))' >/dev/null 2>&1; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

# Installs rustup, the pinned toolchain, its components and the build target.
kodem_rust() {
    if ! command -v rustup >/dev/null 2>&1; then
        echo "Installing rustup from rust-lang.org…"
        local installer
        installer=$(mktemp -d)
        if [[ "$KODEM_PLATFORM" == windows ]]; then
            if ! curl --proto '=https' --tlsv1.2 --fail --location --retry 3 https://win.rustup.rs/x86_64 -o "$installer/rustup-init.exe"; then rm -rf "$installer"; return 1; fi
            if ! "$installer/rustup-init.exe" -y --no-modify-path --profile minimal --default-toolchain none; then rm -rf "$installer"; return 1; fi
        else
            if ! curl --proto '=https' --tlsv1.2 --fail --location --retry 3 https://sh.rustup.rs -o "$installer/rustup.sh"; then rm -rf "$installer"; return 1; fi
            if ! /bin/sh "$installer/rustup.sh" -y --no-modify-path --profile minimal --default-toolchain none; then rm -rf "$installer"; return 1; fi
        fi
        rm -rf "$installer"
    fi
    if ! rustup run "$KODEM_RUST_VERSION" rustc --version >/dev/null 2>&1; then
        rustup toolchain install "$KODEM_RUST_VERSION" --profile minimal
    fi
    if ! rustup run "$KODEM_RUST_VERSION" rustfmt --version >/dev/null 2>&1; then rustup component add rustfmt --toolchain "$KODEM_RUST_VERSION"; fi
    if ! rustup run "$KODEM_RUST_VERSION" cargo-clippy --version >/dev/null 2>&1; then rustup component add clippy --toolchain "$KODEM_RUST_VERSION"; fi
    rustup target add "$(kodem_build_target)" --toolchain "$KODEM_RUST_VERSION" >/dev/null
    export RUSTUP_TOOLCHAIN="$KODEM_RUST_VERSION"
    echo "Using $(rustc --version)"
}

# Selects a complete Codex package, installing the minimum supported one when none is found.
kodem_codex() {
    if ! kodem_find_runtime; then
        if [[ "${KODEM_CODEX_PATH+x}" == x ]]; then
            echo "KODEM_CODEX_PATH must point into a complete Codex $KODEM_CODEX_MIN_VERSION or newer package." >&2
            return 1
        fi
        local codex_target
        codex_target=$(kodem_codex_target)
        KODEM_RUNTIME_DIR="$(kodem_runtimes_dir)/$KODEM_CODEX_MIN_VERSION"
        echo "Installing the complete Codex $KODEM_CODEX_MIN_VERSION package from OpenAI…"
        local archive_dir package_dir
        archive_dir=$(mktemp -d)
        if ! curl --proto '=https' --tlsv1.2 --fail --location --retry 3 "https://github.com/openai/codex/releases/download/rust-v$KODEM_CODEX_MIN_VERSION/codex-package-$codex_target.tar.gz" -o "$archive_dir/codex.tar.gz"; then rm -rf "$archive_dir"; return 1; fi
        mkdir "$archive_dir/package"
        if ! tar -xzf "$archive_dir/codex.tar.gz" -C "$archive_dir/package"; then rm -rf "$archive_dir"; return 1; fi
        package_dir="$archive_dir/package"
        if [[ ! -f "$package_dir/codex-package.json" ]]; then
            package_dir="$archive_dir/package/codex-package-$codex_target"
        fi
        if ! kodem_runtime_valid "$package_dir"; then
            echo "The downloaded Codex package is incomplete or incompatible." >&2
            rm -rf "$archive_dir"; return 1
        fi
        mkdir -p "$(dirname "$KODEM_RUNTIME_DIR")"
        # Stage the whole package beside the destination so replacement is a rename.
        local runtime_stage
        runtime_stage=$(mktemp -d "${KODEM_RUNTIME_DIR}.install.XXXXXX")
        if ! kodem_copy_tree "$package_dir" "$runtime_stage"; then rm -rf "$archive_dir" "$runtime_stage"; return 1; fi
        if [[ -e "$KODEM_RUNTIME_DIR" ]]; then
            mv "$KODEM_RUNTIME_DIR" "$runtime_stage.previous"
        fi
        if ! mv "$runtime_stage" "$KODEM_RUNTIME_DIR"; then
            if [[ -d "$runtime_stage.previous" ]]; then mv "$runtime_stage.previous" "$KODEM_RUNTIME_DIR"; fi
            rm -rf "$archive_dir" "$runtime_stage"; return 1
        fi
        rm -rf "$archive_dir" "$runtime_stage.previous"
        export KODEM_CODEX_PATH="$KODEM_RUNTIME_DIR/bin/codex$KODEM_EXE"
    fi
    echo "Using $("$KODEM_CODEX_PATH" --version)"
}

# Copies a folder's contents into an existing folder, keeping modes.
kodem_copy_tree() {
    if [[ "$KODEM_PLATFORM" == macos ]]; then /usr/bin/ditto "$1" "$2"
    else cp -R -p "$1/." "$2"; fi
}

# Verifies the package layout, tool runner and bundled shell/search dependencies.
kodem_runtime_valid() {
    kodem_runtime_version "$1" >/dev/null
}

# Returns the version only after validating the complete package. The required files match
# kodem_core::providers::codex::PACKAGE_FILES.
kodem_runtime_version() {
    local root=$1
    local package_version cli_version
    [[ -f "$root/codex-package.json" ]] || return 1
    package_version=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$root/codex-package.json" | head -1)
    [[ -n "$package_version" ]] || return 1
    kodem_version_at_least "$package_version" "$KODEM_CODEX_MIN_VERSION" || return 1
    [[ -x "$root/bin/codex$KODEM_EXE" && -x "$root/bin/codex-code-mode-host$KODEM_EXE" && -x "$root/codex-path/rg$KODEM_EXE" ]] || return 1
    case "$KODEM_PLATFORM" in
        windows) [[ -f "$root/codex-resources/codex-command-runner.exe" ]] || return 1 ;;
        linux) [[ -x "$root/codex-resources/zsh/bin/zsh" && -x "$root/codex-resources/bwrap" ]] || return 1 ;;
        macos) [[ -x "$root/codex-resources/zsh/bin/zsh" ]] || return 1 ;;
    esac
    cli_version=$("$root/bin/codex$KODEM_EXE" --version 2>/dev/null | tr -d '\r') || return 1
    [[ "$cli_version" == "codex-cli $package_version" ]] || return 1
    "$root/bin/codex-code-mode-host$KODEM_EXE" --help >/dev/null 2>&1 || return 1
    "$root/codex-path/rg$KODEM_EXE" --version >/dev/null 2>&1 || return 1
    if [[ "$KODEM_PLATFORM" != windows ]]; then
        "$root/codex-resources/zsh/bin/zsh" -f -c 'exit 0' >/dev/null 2>&1 || return 1
    fi
    echo "$package_version"
}

# Compares semantic version precedence, including prereleases and build metadata.
kodem_version_at_least() {
    awk -v candidate="$1" -v minimum="$2" -f "$KODEM_DEPENDENCIES_DIR/codex-version.awk"
}

# Resolves CLI symlinks before checking the sibling package resources.
kodem_runtime_root() {
    local executable=$1 target hops=0
    while [[ -L "$executable" ]]; do
        ((hops += 1))
        [[ "$hops" -le 40 ]] || return 1
        target=$(readlink "$executable") || return 1
        if [[ "$target" == /* ]]; then executable=$target
        else executable="$(dirname "$executable")/$target"; fi
    done
    [[ -f "$executable" ]] || return 1
    (cd "$(dirname "$executable")/.." && pwd -P)
}

# Honors an explicit override or selects the newest valid installed package.
kodem_find_runtime() {
    local candidate root version best_version="" best_root="" seen=$'\n'
    local candidates=() path_entries=()
    if [[ "${KODEM_CODEX_PATH+x}" == x ]]; then
        candidates=("$KODEM_CODEX_PATH")
    else
        candidates=("$HOME/.local/bin/codex$KODEM_EXE" /opt/homebrew/bin/codex /usr/local/bin/codex)
        IFS=: read -r -a path_entries <<< "${PATH:-}"
        for candidate in "${path_entries[@]}"; do candidates+=("${candidate:-.}/codex$KODEM_EXE"); done
        candidates+=("$(kodem_runtimes_dir)"/*/bin/codex$KODEM_EXE)
    fi
    for candidate in "${candidates[@]}"; do
        root=$(kodem_runtime_root "$candidate") || continue
        [[ "$seen" != *$'\n'"$root"$'\n'* ]] || continue
        seen+="$root"$'\n'
        version=$(kodem_runtime_version "$root") || continue
        if [[ -z "$best_version" ]] || kodem_version_at_least "$version" "$best_version"; then
            best_version=$version
            best_root=$root
        fi
    done
    [[ -n "$best_root" ]] || return 1
    export KODEM_CODEX_PATH="$best_root/bin/codex$KODEM_EXE"
}
