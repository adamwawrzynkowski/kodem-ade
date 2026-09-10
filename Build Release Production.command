#!/bin/bash
# Double-click to build and publish a Production GitHub release.
cd "$(dirname "$0")" || exit 1
mkdir -p target || exit 1
log="target/build-production.log"
/bin/bash scripts/build-macos.sh production 2>&1 | /usr/bin/tee "$log"
result=${PIPESTATUS[0]}
if [[ $result != 0 ]]; then printf '\nProduction command stopped. See %s for the failed step.\n' "$log"; fi
if [[ -t 0 && "${KODEM_NONINTERACTIVE:-0}" != 1 ]]; then read -r -p 'Press Return to close this window… '; fi
exit "$result"
