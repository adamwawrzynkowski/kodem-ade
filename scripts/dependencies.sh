#!/bin/bash
# Sourced by the double-click build commands. Installs only Kodem's prerequisites.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:${PATH:-}"
KODEM_RUST_VERSION=1.97.1
KODEM_CODEX_VERSION=0.153.4
KODEM_RUNTIME_DIR="$HOME/.local/share/kodem/runtimes/$KODEM_CODEX_VERSION"

kodem_dependencies() {
    [[ "$(uname -s)" == Darwin ]] || { echo "These build commands require macOS." >&2; return 1; }
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
    if ! command -v rustup >/dev/null 2>&1; then
        echo "Installing rustup from rust-lang.org…"
        local installer
        installer=$(mktemp -t kodem-rustup)
        if ! curl --proto '=https' --tlsv1.2 --fail --location --retry 3 https://sh.rustup.rs -o "$installer"; then rm -f "$installer"; return 1; fi
        if ! /bin/sh "$installer" -y --no-modify-path --profile minimal --default-toolchain none; then rm -f "$installer"; return 1; fi
        rm -f "$installer"
    fi
    if ! rustup run "$KODEM_RUST_VERSION" rustc --version >/dev/null 2>&1; then
        rustup toolchain install "$KODEM_RUST_VERSION" --profile minimal
    fi
    if ! rustup run "$KODEM_RUST_VERSION" rustfmt --version >/dev/null 2>&1; then rustup component add rustfmt --toolchain "$KODEM_RUST_VERSION"; fi
    if ! rustup run "$KODEM_RUST_VERSION" cargo-clippy --version >/dev/null 2>&1; then rustup component add clippy --toolchain "$KODEM_RUST_VERSION"; fi
    export RUSTUP_TOOLCHAIN="$KODEM_RUST_VERSION"
    echo "Using $(rustc --version)"
    if ! kodem_runtime_valid "$KODEM_RUNTIME_DIR"; then
        echo "Installing the complete Codex $KODEM_CODEX_VERSION package from OpenAI…"
        local archive_dir package_dir
        archive_dir=$(mktemp -d -t kodem-codex)
        if ! curl --proto '=https' --tlsv1.2 --fail --location --retry 3 "https://github.com/openai/codex/releases/download/rust-v$KODEM_CODEX_VERSION/codex-package-aarch64-apple-darwin.tar.gz" -o "$archive_dir/codex.tar.gz"; then rm -rf "$archive_dir"; return 1; fi
        mkdir "$archive_dir/package"
        if ! tar -xzf "$archive_dir/codex.tar.gz" -C "$archive_dir/package"; then rm -rf "$archive_dir"; return 1; fi
        package_dir="$archive_dir/package"
        if [[ ! -f "$package_dir/codex-package.json" ]]; then
            package_dir="$archive_dir/package/codex-package-aarch64-apple-darwin"
        fi
        if ! kodem_runtime_valid "$package_dir"; then
            echo "The downloaded Codex package is incomplete or incompatible." >&2
            rm -rf "$archive_dir"; return 1
        fi
        mkdir -p "$(dirname "$KODEM_RUNTIME_DIR")"
        # Stage the whole package beside the destination so replacement is a rename.
        local runtime_stage
        runtime_stage=$(mktemp -d "${KODEM_RUNTIME_DIR}.install.XXXXXX")
        if ! /usr/bin/ditto "$package_dir" "$runtime_stage"; then rm -rf "$archive_dir" "$runtime_stage"; return 1; fi
        if [[ -e "$KODEM_RUNTIME_DIR" ]]; then
            mv "$KODEM_RUNTIME_DIR" "$runtime_stage.previous"
        fi
        if ! mv "$runtime_stage" "$KODEM_RUNTIME_DIR"; then
            if [[ -d "$runtime_stage.previous" ]]; then mv "$runtime_stage.previous" "$KODEM_RUNTIME_DIR"; fi
            rm -rf "$archive_dir" "$runtime_stage"; return 1
        fi
        rm -rf "$archive_dir" "$runtime_stage.previous"
    fi
    export KODEM_CODEX_PATH="$KODEM_RUNTIME_DIR/bin/codex"
    echo "Using $("$KODEM_CODEX_PATH" --version)"
    # Cargo installs every pinned Rust dependency, including bundled SQLite.
    cargo fetch --locked
}

# Verifies the package layout, tool runner and bundled shell/search dependencies.
kodem_runtime_valid() {
    local root=$1
    [[ -f "$root/codex-package.json" ]] || return 1
    [[ "$(/usr/bin/plutil -extract version raw -o - "$root/codex-package.json" 2>/dev/null)" == "$KODEM_CODEX_VERSION" ]] || return 1
    [[ -x "$root/bin/codex" && -x "$root/bin/codex-code-mode-host" && -x "$root/codex-path/rg" && -x "$root/codex-resources/zsh/bin/zsh" ]] || return 1
    [[ "$("$root/bin/codex" --version 2>/dev/null)" == "codex-cli $KODEM_CODEX_VERSION" ]] || return 1
    "$root/bin/codex-code-mode-host" --help >/dev/null 2>&1 || return 1
    "$root/codex-path/rg" --version >/dev/null 2>&1 || return 1
    "$root/codex-resources/zsh/bin/zsh" -f -c 'exit 0' >/dev/null 2>&1 || return 1
}
