#!/bin/bash
# Double-click in Finder to build and install /Applications/Kodem.app.
cd "$(dirname "$0")" || exit 1
mkdir -p target || exit 1
log="target/build-release.log"
/bin/bash scripts/build-macos.sh release 2>&1 | /usr/bin/tee "$log"
result=${PIPESTATUS[0]}
if [[ $result != 0 ]]; then printf '\nRelease command stopped. See target/build-release.log for the failed step.\n'; fi
if [[ -t 0 && "${KODEM_NONINTERACTIVE:-0}" != 1 ]]; then read -r -p 'Press Return to close this window… '; fi
exit "$result"
