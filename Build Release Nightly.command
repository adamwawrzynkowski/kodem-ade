#!/bin/bash
# Double-click to release Nightly for macOS, Windows (x64 and Arm64) and Linux. Everything builds in
# GitHub Actions; this window follows the progress and publishes the release when it is ready.
cd "$(dirname "$0")" || exit 1
mkdir -p target || exit 1
log="target/build-nightly.log"
/bin/bash scripts/release.sh nightly 2>&1 | tee "$log"
result=${PIPESTATUS[0]}
if [[ $result != 0 ]]; then printf '\nNightly command stopped. See %s for the failed step.\n' "$log"; fi
if [[ -t 0 && "${KODEM_NONINTERACTIVE:-0}" != 1 ]]; then read -r -p 'Press Return to close this window… '; fi
exit "$result"
