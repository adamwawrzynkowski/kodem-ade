#!/bin/bash
# Detached updater. Arguments are passed directly by Rust, never interpolated as code.
set -euo pipefail
parent_pid=$1
destination=$2
bundle=$3
stage=$4
backup="$stage/previous.app"
[[ "$parent_pid" =~ ^[0-9]+$ && -d "$destination" && -d "$bundle" && -d "$stage" && ! -e "$backup" ]]
[[ -w "$(dirname "$destination")" ]]
/usr/bin/codesign --verify --deep --strict "$bundle"
touch "$stage/ready"
# Never replace an app that is still saving its state. Time out without touching it.
for ((attempt=0; attempt<120; attempt++)); do
    if ! kill -0 "$parent_pid" 2>/dev/null; then break; fi
    sleep 1
done
if kill -0 "$parent_pid" 2>/dev/null; then echo 'Desktop did not exit; update was not installed.'; exit 1; fi
replaced=0
rollback() {
    result=$?
    if [[ "$replaced" == 1 && "$result" != 0 ]]; then
        if [[ -d "$destination" ]]; then mv "$destination" "$stage/failed.app"; fi
        if [[ -d "$backup" ]]; then mv "$backup" "$destination"; fi
        /usr/bin/open -n "$destination" || true
        echo "Update failed; restored the previous app. Log: $stage/install.log" >&2
    fi
}
trap rollback EXIT
mv "$destination" "$backup"
replaced=1
mv "$bundle" "$destination"
/usr/bin/codesign --verify --deep --strict "$destination"
/usr/bin/open -n "$destination"
replaced=0
rm -rf "$stage"
