#!/bin/bash
# Usage: scripts/ux/shot.sh out.png mode [ms] [script-file]  (extra env passes through)
# One screenshot of the release app at 1366x768 in a headless KWin.
out=$1; mode=$2; ms=${3:-2500}; script=$4
BIN="$(cd "$(dirname "$0")/../.." && pwd)/target/release/keyframexl"
[ -n "$script" ] && export KFXL_CAPTURE_SCRIPT="$script"
KFXL_CAPTURE_SIZE=${KFXL_CAPTURE_SIZE:-1366x768} KFXL_CAPTURE_AFTER_MS=$ms timeout 90 kwin_wayland --virtual --no-lockscreen --width 1960 --height 1120 --socket kfxl-ux-$$ --exit-with-session "$BIN $mode $out" >/dev/null 2>&1
[ -f "$out" ] && echo "ok $out" || echo "FAILED $out"
