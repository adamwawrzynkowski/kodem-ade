#!/bin/bash
# Double-click to build and publish a Production GitHub release for macOS, Windows (x64 and Arm64)
# and Linux: macOS builds here, the others in GitHub Actions, and one release carries all of them.
cd "$(dirname "$0")" || exit 1
mkdir -p target || exit 1
log="target/build-production.log"
# The same command builds the Linux package when it runs on Linux.
script=scripts/build-macos.sh
if [[ "$(uname -s)" == Linux ]]; then script=scripts/build-linux.sh; fi
/bin/bash "$script" production 2>&1 | tee "$log"
result=${PIPESTATUS[0]}
if [[ $result != 0 ]]; then printf '\nProduction command stopped. See %s for the failed step.\n' "$log"; fi
if [[ -t 0 && "${KODEM_NONINTERACTIVE:-0}" != 1 ]]; then read -r -p 'Press Return to close this window… '; fi
exit "$result"
