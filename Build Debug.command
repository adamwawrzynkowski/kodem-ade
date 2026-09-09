#!/bin/bash
# Double-click in Finder to build the development app.
cd "$(dirname "$0")" || exit 1
/bin/bash scripts/build-macos.sh debug
result=$?
if [[ $result != 0 ]]; then printf '\nDebug build failed. Read the error above.\n'; fi
if [[ -t 0 && "${KODEM_NONINTERACTIVE:-0}" != 1 ]]; then read -r -p 'Press Return to close this window… '; fi
exit "$result"
