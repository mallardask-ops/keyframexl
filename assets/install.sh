#!/bin/sh
# Install KeyFrameXL for the current user: the binary to ~/.local/bin, the
# launcher and icons to ~/.local/share. Run from the unpacked archive.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-$HOME/.local}
install -Dm755 "$here/keyframexl" "$prefix/bin/keyframexl"
sed "s|^Exec=keyframexl|Exec=$prefix/bin/keyframexl|" "$here/share/applications/keyframexl.desktop" \
  > "$here/.desktop.tmp"
install -Dm644 "$here/.desktop.tmp" "$prefix/share/applications/keyframexl.desktop"
rm -f "$here/.desktop.tmp"
cp -r "$here/share/icons" "$prefix/share/"
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" || true
echo "KeyFrameXL installed to $prefix/bin/keyframexl"
