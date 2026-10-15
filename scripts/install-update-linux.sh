#!/bin/bash
# Detached Linux updater. Arguments are passed directly by Rust, never interpolated as code.
set -euo pipefail
parent_pid=$1
destination=$2
package=$3
stage=$4
backup="$stage/previous"
[[ "$parent_pid" =~ ^[0-9]+$ && -d "$destination" && -d "$package" && -d "$stage" && ! -e "$backup" ]]
[[ -w "$(dirname "$destination")" ]]
[[ -x "$package/kodem" && -x "$package/kodem-host" && -f "$package/release.json" ]]
touch "$stage/ready"
# Never replace an app that is still saving its state. Time out without touching it.
for ((attempt=0; attempt<120; attempt++)); do
    if ! kill -0 "$parent_pid" 2>/dev/null; then break; fi
    sleep 1
done
if kill -0 "$parent_pid" 2>/dev/null; then echo 'Desktop did not exit; update was not installed.'; exit 1; fi
# Starts the app outside this script's session, so it outlives the updater.
launch() {
    if command -v setsid >/dev/null; then
        setsid "$1/kodem" </dev/null >/dev/null 2>&1 &
    else
        nohup "$1/kodem" </dev/null >/dev/null 2>&1 &
    fi
}
replaced=0
rollback() {
    result=$?
    if [[ "$replaced" == 1 && "$result" != 0 ]]; then
        if [[ -d "$destination" ]]; then mv "$destination" "$stage/failed"; fi
        if [[ -d "$backup" ]]; then mv "$backup" "$destination"; fi
        launch "$destination" || true
        echo "Update failed; restored the previous app. Log: $stage/install.log" >&2
    fi
}
trap rollback EXIT
mv "$destination" "$backup"
replaced=1
mv "$package" "$destination"
[[ -x "$destination/kodem" ]]
launch "$destination"
replaced=0
rm -rf "$stage"
