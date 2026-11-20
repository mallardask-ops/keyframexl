#!/bin/bash
# Usage: scripts/ux/shots.sh [out-dir]   (default docs/screenshots)
# The README screenshots, all from the showcase song (src/demo.rs), stopped,
# in a headless KWin. Needs a release build.
set -u
dir=${1:-"$(cd "$(dirname "$0")/../.." && pwd)/docs/screenshots"}
shot="$(dirname "$0")/shot.sh"
mkdir -p "$dir"
export KFXL_CAPTURE_DEMO=1 KFXL_CAPTURE_STOPPED=1
# file            size        mode                    extra env
while read -r file size mode env; do
  [ -z "$file" ] && continue
  env KFXL_CAPTURE_SIZE="$size" ${env//,/ } "$shot" "$dir/$file" "$mode" 3000
done <<'LIST'
workspace.png 1600x900 --capture-rack KFXL_CAPTURE_SONG=1,KFXL_CAPTURE_POS=12,KFXL_CAPTURE_BAR_W=40
mixer.png 1366x768 --capture-mixer KFXL_CAPTURE_SONG=1
roll.png 1366x768 --capture-roll KFXL_CAPTURE_INSTRUMENT=Vocal
tide.png 1366x768 --capture-instrument KFXL_CAPTURE_INSTRUMENT=Pad
forge.png 1366x768 --capture-instrument KFXL_CAPTURE_INSTRUMENT=Riser
sampler.png 1366x768 --capture-instrument KFXL_CAPTURE_INSTRUMENT=Texture
genres.png 1366x768 --capture-rack KFXL_CAPTURE_WELCOME=genres
LIST
