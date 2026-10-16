#!/bin/sh
# Installs Kodem for the current user: the app folder under ~/.local/share/kodem, a desktop
# entry and a command in ~/.local/bin. Run it with --uninstall to remove them; settings and
# tasks stay. Only a first installation needs it: the app updates itself afterwards.
#
# scripts/build-linux.sh appends the package archive after the last line of this script, which
# makes Kodem-linux-x86_64-setup.run.
set -eu
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
payload=$(awk '/^__KODEM_PACKAGE__$/ { print NR + 1; exit }' "$0")
[ -n "$payload" ] && tail -n +"$payload" "$0" | tar -xzf - -C "$work" || { echo "This installer is damaged. Download it again." >&2; exit 1; }
source_dir=
for folder in "$work"/*; do source_dir=$folder; done
[ -f "$source_dir/release.json" ] || { echo "This installer is damaged. Download it again." >&2; exit 1; }
field() {
    sed -n "s/^[[:space:]]*\"$1\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" "$source_dir/release.json"
}
app_id=$(field identifier)
name=$(field name)
package=$(basename "$source_dir")
[ -n "$app_id" ] && [ -n "$name" ] || { echo "This installer is damaged. Download it again." >&2; exit 1; }
data=${XDG_DATA_HOME:-$HOME/.local/share}
destination="$data/kodem/$package"
entry="$data/applications/$app_id.desktop"
command_link="$HOME/.local/bin/$package"
if [ "${1:-}" = --uninstall ]; then
    rm -rf "$destination" "$entry"
    if [ -L "$command_link" ]; then rm -f "$command_link"; fi
    echo "Removed $name."
    exit 0
fi
mkdir -p "$data/kodem"
stage=$(mktemp -d "$data/kodem/.install.XXXXXX")
cp -R -p "$source_dir/." "$stage/"
rm -rf "$destination.previous"
if [ -e "$destination" ]; then mv "$destination" "$destination.previous"; fi
mv "$stage" "$destination"
rm -rf "$destination.previous"
mkdir -p "$data/applications" "$HOME/.local/bin"
cat > "$entry" <<ENTRY
[Desktop Entry]
Type=Application
Name=$name
Comment=Agentic development environment
Exec="$destination/kodem"
Icon=$destination/kodem.png
Terminal=false
Categories=Development;IDE;
StartupWMClass=$app_id
ENTRY
ln -sf "$destination/kodem" "$command_link"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data/applications" >/dev/null 2>&1 || true
fi
echo "Installed $name in $destination"
echo "Open it from your applications menu or run $package."
exit 0
__KODEM_PACKAGE__
