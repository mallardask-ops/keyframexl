# FL Studio live link

A channel that plays FL Studio itself, for Image-Line instruments that do
not run inside *FL Studio VSTi* (FLEX above all). The code is
`src/fl_link.rs`; the channel window is `fl_link_panel` in
`src/ui/plugins_ui.rs`.

## How it works

- **Notes** leave through an ALSA sequencer port that FL sees as the MIDI
  input `KeyFrameXL - FL link`. The port lives in the main process (a relay)
  for the whole session: Wine lists MIDI devices only when FL starts.
  `open_fl` switches the input on in FL's registry before FL starts.
- **Sound**: FL plays into the null sink `kfxl_fl_link` (made with `pactl`);
  `parec` reads its monitor and the engine plays it as the channel's output.
  FL streams that play elsewhere are moved onto the sink every 2 s.
- **Delay**: the round trip note → sound is measured (median of the last
  five notes) and reported as plugin latency, rounded to 256 frames, so the
  engine compensates it.
- **Export**: FL cannot render faster than real time, so the link is
  silent in an export.

FL runs on the system Wine. When plugins use another Wine (Proton), FL gets
a prefix of its own (`~/.local/share/keyframexl/fl-prefix`), copied from the
user's prefix once.

## The channel window

A monitor thread refreshes the state once a second; the window never waits
on `pactl`. The link (which may run in a bridged plugin process) writes its
numbers to `$XDG_RUNTIME_DIR/keyframexl-fl-link.stats` twice a second.

States: *Not running* → *Starting* → *Connected*; *Check* when it works
but FL has not opened the input, the link channel is not running, or the
sound has gaps; *Error* when the port or sink is missing, `parec` cannot
run, or starting FL failed. Rows: FL Studio, notes port, FL listens (the
port has a subscriber other than PipeWire), sound sink, flow, measured
delay, gaps and queue.

*Reconnect* makes the relay and the sink again and moves FL's streams; it
says when FL must be restarted. Only one link channel is allowed: FL plays
only its selected channel, so a second link would double the sound.

## FLEX

- FLEX does **not** run inside FL Studio VSTi (any Wine). It runs in
  standalone FL Studio.
- It is reported to start under Wine only with **vkd3d-proton** (its
  `d3d12.dll` in the prefix's `system32`) and the **Verdana** font in
  `drive_c/windows/Fonts` (flmint, November 2026; Image-Line forum, August
  2026). The window checks both in FL's prefix; nothing is installed for
  you.
- **FL Studio 2026.1.4+** fails Wine's certificate check ("The validity of
  the program could not be verified") without Wine merge request 11824,
  which DAW-GE-Proton11-6c carries. FL 2026.1.3 and earlier start on stock
  Wine. The window warns when the prefix holds FL 2026.

## Checking it

- `keyframexl fl-link-test [project.flp]` starts FL, plays C4 eight times
  and prints peak, round trip, queue and underruns, then reloads the link
  and plays again. `KFXL_FL_NO_OPEN=1` checks only our side (port, relay,
  sink, stats) without FL; `KFXL_FL_WAIT` sets how long FL gets to load.
- `KFXL_FL_QUEUE=64` keeps less audio queued, which provokes gaps (the
  window turns to *Check*).
- `KFXL_FL_LOG=file` saves FL's Wine output; `KFXL_FL_WINE` and
  `KFXL_FL_PREFIX` choose the Wine and prefix.
