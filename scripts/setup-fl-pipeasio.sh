#!/usr/bin/env bash
set -euo pipefail

data=${XDG_DATA_HOME:-"$HOME/.local/share"}/keyframexl
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

git clone --quiet --depth 1 --branch master https://github.com/M0n7y5/pipeasio.git "$work/pipeasio"
revision=be171ebfbd379d313bde95bb596c93a64f6a2320
if [[ $(git -C "$work/pipeasio" rev-parse HEAD) != "$revision" ]]; then
    git -C "$work/pipeasio" fetch --quiet --depth 1 origin "$revision"
    git -C "$work/pipeasio" checkout --quiet --detach "$revision"
fi
cmake -S "$work/pipeasio" -B "$work/build" \
    -DBUILD_SETTINGS_PANEL=OFF -DBUILD_MANAGER=OFF -DBUILD_TESTS=OFF \
    -DCMAKE_INSTALL_PREFIX="$data/pipeasio"
cmake --build "$work/build" --parallel 4
cmake --install "$work/build"
