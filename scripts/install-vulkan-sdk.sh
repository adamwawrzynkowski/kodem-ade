#!/bin/bash
# Installs the pinned Vulkan SDK on a Linux x64 or Windows x64 build computer, for GitHub Actions
# and for computers whose package manager lacks it (Ubuntu 22.04 has no glslc). Voice input
# builds whisper.cpp with its Vulkan backend there, which needs the SDK's headers and glslc.
#
#   install-vulkan-sdk.sh <folder>
#
# The SDK goes into <folder>. In GitHub Actions the script also sets VULKAN_SDK and PATH for the
# following steps; elsewhere it prints the variables to set.
set -euo pipefail
version=1.4.363.0
folder=${1:?Usage: install-vulkan-sdk.sh <folder>}
mkdir -p "$folder"
case "$(uname -s)" in
    Linux)
        archive="$folder/vulkansdk-$version.tar.xz"
        curl --proto '=https' --tlsv1.2 --fail --location --retry 3 -o "$archive" \
            "https://sdk.lunarg.com/sdk/download/$version/linux/vulkansdk-linux-x86_64-$version.tar.xz"
        echo "197962f5cbf80baf2775a03336a01cee7c8745686c65aaa70d3f751ade4d7e43  $archive" | sha256sum --check
        tar -xJf "$archive" -C "$folder"
        rm "$archive"
        sdk="$folder/$version/x86_64"
        bin="$sdk/bin"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        installer="$folder/vulkansdk-$version.exe"
        curl --proto '=https' --tlsv1.2 --fail --location --retry 3 -o "$installer" \
            "https://sdk.lunarg.com/sdk/download/$version/windows/vulkansdk-windows-X64-$version.exe"
        echo "94a82d378f7a5e3e54c9db7d2fb7016af136e14ac0a18dbf0f2f67a36352d141  $installer" | sha256sum --check
        sdk=$(cygpath -w "$folder/VulkanSDK")
        "$installer" --root "$sdk" --accept-licenses --default-answer --confirm-command install
        rm "$installer"
        bin="$sdk\\Bin"
        ;;
    *) echo "The Vulkan SDK is needed on Linux and Windows only." >&2; exit 2 ;;
esac
if [[ -n "${GITHUB_ENV:-}" ]]; then
    echo "VULKAN_SDK=$sdk" >> "$GITHUB_ENV"
    echo "$bin" >> "$GITHUB_PATH"
else
    echo "Set VULKAN_SDK=$sdk and add $bin to PATH."
fi
