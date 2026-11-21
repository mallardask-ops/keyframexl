#!/bin/bash
# Usage: scripts/live/listen.sh out.wav shot.png ms --capture-xxx  (env such as
# KFXL_CAPTURE_SCRIPT / _PREVIEW / _EQ / _RECORD passes through).
# Runs the release app in a headless KWin for `ms` milliseconds and records the
# default sink's monitor (what the speakers get) with pw-record meanwhile.
out=$1; shot=$2; ms=$3; mode=$4
BIN="$(cd "$(dirname "$0")/../.." && pwd)/target/release/keyframexl"
KFXL_CAPTURE_SIZE=1366x768 KFXL_CAPTURE_AFTER_MS=$ms timeout 60 kwin_wayland --virtual --no-lockscreen --width 1406 --height 808 --socket kfxl-listen-$$ --exit-with-session "$BIN $mode $shot" >/tmp/kfxl-listen-$$.log 2>&1 &
kpid=$!
node=""
for i in $(seq 1 100); do
  node=$(pw-dump 2>/dev/null | python3 -c '
import json,sys
for o in json.load(sys.stdin):
    p=o.get("info",{}).get("props",{}) or {}
    if o.get("type","").endswith("Node") and p.get("media.class")=="Stream/Output/Audio" and "keyframexl" in (str(p.get("application.process.binary",""))+str(p.get("application.name",""))+str(p.get("node.name",""))).lower():
        print(o["id"]); break')
  [ -n "$node" ] && break
  sleep 0.1
done
if [ -z "$node" ]; then echo "no app audio node"; wait $kpid; exit 1; fi
pw-record -P "{ stream.capture.sink=true }" --target "$(pactl get-default-sink)" --rate 48000 --channels 2 --format f32 "$out" &
rpid=$!
wait $kpid
sleep 0.2; kill $rpid 2>/dev/null; wait $rpid 2>/dev/null
grep -E "Captured|panick" /tmp/kfxl-listen-$$.log | head -3
echo "node $node -> $out"
