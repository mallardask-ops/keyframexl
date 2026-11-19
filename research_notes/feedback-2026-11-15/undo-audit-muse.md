# Undo audit — src/ui/* (2026-11-15, muse)

Mechanism: `App::checkpoint()` (`src/ui/mod.rs:670`) clones `song` onto `history` (cap 100); `undo`/`redo` (`:683`,`:697`) swap it back + `publish()`. Rule: every user action that mutates `self.song` needs exactly one `checkpoint()` before the first mutation of that gesture. View-only state (zoom/scroll/selection/snap/tool/lane/ghosts/scale/meters) must not checkpoint.

## Missing or flooded undo steps (fix these)

Sorted by user impact. `checkpoint?`: yes / **no** / **per-frame** (floods stack) / **stray** (pushes history without a real edit).

| file:function (line) | user action | mutation | checkpoint? | fix (one line) |
|---|---|---|---|---|
| `plugins_ui.rs:parameters_page` (~538–613) | tweak VST param (slider / On-Off chips) | `Req::SetParam` to host + `params` cache; song bytes only at save-time `store_states` | **no** | route param edits through song (or checkpoint + store state into `plugin_ref` per gesture, like built-in FX knobs) |
| `plugins_ui.rs:store_states` (246–252) + `dirty_plugin_state` (613–615) | any VST editor/param tweak, then save/preset/collect | `plugin_ref.set_state()` / `edits += 1` with no history push | **no** | `checkpoint()` on first `SetParam` of a gesture (drag-started / 0.6 s wheel run, cf. `kit.rs:443,595`); undo must restore `state` bytes and re-push to host |
| `rack.rs:channel_strip` (625–633) → `set_channel_insert` (381–392) | wheel or drag the channel mixer-insert box | `inst.mixer = to` via `set_channel_insert` → `checkpoint()` per 50 px chunk | **per-frame** | checkpoint once per gesture: on `mr.drag_started()` or first wheel notch of a 0.6 s run (same `WHEEL_GESTURE` pattern as `kit.rs:595` and `deck.rs:404–408`) |
| `mod.rs:tap` (883–899) | tap tempo (3+ taps) | `song.tempo = …` on every tap after the 2nd | **per-frame** | checkpoint only on the first computed tempo of a tap run (clear `taps` grouping: checkpoint when `taps.len() == 3`, not on every later tap) |
| `files.rs:load_file` (602–625) + `instrument.rs:add_sampler` (15) + `mod.rs:land_preset` (1004–1039) | drop/load a sample onto empty rack (new channel) | `add_channel` (checkpoint #1) then async `set_sampler_preset` (checkpoint #2) | **2 steps for 1 gesture** | pass a "join next preset" flag: skip the `add_sampler` checkpoint when a non-quiet `load_preset` for the same id is already in flight, or checkpoint once in `load_file` |
| `instrument.rs:instrument_window` (248–255) | click into channel name field, type | `inst.name = …` per keystroke; checkpoint on `gained_focus` | **stray + no publish** | checkpoint on first `changed` (not focus), `publish()` after rename — cf. clean pattern in `playlist.rs:397–402`, `menu.rs:387–393` |
| `mixer.rs:inspector` (410–416) | click into insert name field, type | `mixer[i].name = …` per keystroke; checkpoint on `gained_focus`, no `publish()` | **stray + no publish** | same fix: checkpoint on first `changed`, add `publish()` |
| `rack.rs:rack` (356–364) | paint step pads over already-correct pads | `else if first { checkpoint }` pushes history even when `at.is_some() == fill` (no-op stroke) | **stray** | drop the `else if first` branch — only checkpoint when a pad actually flips |
| `library.rs:load_default_kit` (136–143) | startup default-kit load | `inst.volume = 0.78` directly, no checkpoint | **no (intentional quiet)** | OK as-is if intentional, but document: volume change is invisible to undo *and* to dirty flag; consider folding into the quiet preset landing |
| `plugins_ui.rs:collect_plugin_states` (305–331) called before `checkpoint()` in `instrument.rs:83–86`, `mixer.rs:571–574,594–597`, `presets_ui.rs:131–153` | delete channel / replace FX / load preset on a plugin channel | `store_states` mutates `song` *before* the checkpoint snapshots it | bakes in | collect-then-checkpoint means undo keeps the fetched plugin states instead of reverting them — acceptable, but note undo never reverts external-editor tweaks made since the last collect |

Notes on the two **no-publish** rename fields: audio doesn't consume names, so nothing sounds wrong, but any listener that diffs on `publish()` (meters/selection fixups aside) misses the rename; add `publish()` for consistency.

## Should NOT create undo steps but do (view-only pollution)

| file:function (line) | action | what happens | fix |
|---|---|---|---|
| `instrument.rs:248` | focus channel name field, change nothing | `checkpoint()` on `gained_focus` → Ctrl+Z appears to do nothing | checkpoint on first `changed` |
| `mixer.rs:411` | focus insert name field, change nothing | same | same |
| `rack.rs:363` | paint stroke that flips no pads | `checkpoint()` on `first` even for no-op | only checkpoint inside the `if at.is_some() != fill` branch |

Everything else view-only is clean: zoom (`roll.rs:zoom_roll`, `playlist.rs:795–813`), scroll, `roll_sel`/rack `select_channel`, `snap`, `tool`, `lane_target`, `ghosts`, `roll_scale`, `song_mode`/`seek_tick`, meters/clip-latch — none checkpoint. Mute/solo **do** checkpoint (`rack.rs:123–139`, `mixer.rs:129–137,285–291`, `playlist.rs:376–442`) and should: they mutate `song.*.muted`, i.e. real song state, not view state.

## Continuous gestures — one step per gesture? (mostly yes)

| gesture | checkpoint site | verdict |
|---|---|---|
| LCD tempo steppers hold-to-repeat | `deck.rs:338` (first press only; repeats mutate under it) | yes |
| tempo drag | `deck.rs:380` on `drag_started` | yes |
| tempo wheel run | `deck.rs:404–408`, 0.6 s gap grouping | yes |
| tempo type-in / right-click reset | `deck.rs:413,432` on apply | yes |
| knob/fader drags (mixer EQ/pan/sep/vol, slot mix, FX params, PEQ bands, sends, rack pan/vol, swing) | `started` from `kit::{knob,mini_knob,fader}` (`kit.rs:582,595`) → `mixer.rs:159,300,318,330,500,714,766,789`, `rack.rs:110,568,589` | yes (wheel grouped by `WHEEL_GESTURE = 0.6`, `kit.rs:443`) |
| knob typed entry / reset / arrows | `kit.rs:579–641` sets `started = true` on apply | yes (single step at Enter/reset) |
| roll note move/resize (incl. Shift-duplicate) | `roll.rs:1006` on press; live `note_mut` writes; `history.pop()` if click-no-move (`:1206–1214`) | yes |
| roll draw/paint stroke | `roll.rs:1028` on press; per-frame `add_note` under it (`:1141–1154`) | yes |
| roll erase / mute-tool drags | `roll.rs:1329–1334 erase_once` (first hit) | yes |
| roll slide edit | `roll.rs:987` on press; pop if unchanged (`:1268–1277`) | yes |
| roll event-lane drag | `roll.rs:1660` on `drag_started`/click | yes |
| rack step-pad paint stroke | `rack.rs:359` on first flip | yes, modulo the no-op stray above |
| rack velocity graph drag | `rack.rs:531–533` on `drag_started`/click | yes |
| playlist clip move/resize/trim/duplicate | `playlist.rs:944,966,975,986` once at drop (`Pending`) | yes |
| playlist paint stroke | `playlist.rs:825` `if !changed` once per stroke | yes |
| playlist right-erase drag | `playlist.rs:743` `if count == 0` once per drag | yes |
| playlist slice brush | `playlist.rs:775` per slice click (brush gated by `pressed`, `:771`) | yes |
| sampler program/zone/filter/LFO/mod knobs + envelope | `sampler.rs:93,128` via `pknob`/`edit_preset(started)`; chips/combos pass `true` (`:261,276,345,350,382,399,426,444,474`) | yes |
| MIDI live-record take | `settings_ui.rs:240` once per pass (`midi_take` flag; later notes `edits += 1`) | yes |
| MIDI import / MIDI-clip drop | `midi_io.rs:115,317` once; quiet `dress_midi_channels` loads ride along | yes |
| preset load (channel/mixer/FX/new) | `presets_ui.rs:133,138,145,151` once per load | yes |
| plugin channel add / FX slot set | `plugins_ui.rs:649`, `mixer.rs:597` | yes |
| pattern add/clone/delete, clip add/delete/mute/unique, track mute/solo/rename/clear | `mod.rs:754,784,804`, `playlist.rs:110,118,131,149,185,377,399,426,436,446` | yes |
| channel mute/solo/step toggle, move/clone/delete channels, route channels, delete channel | `rack.rs:36,42,110,124,133,144,364,385,405,420,452`, `mixer.rs:184`, `instrument.rs:86` | yes |
| note ops: nudge/paste/duplicate/delete/mute/porta/color/legato/chop/glue/quantize(+dialog) | `roll.rs:1344,1361,1399,1426,1445,1471,1492,1516,1758,1783,1814,1872` + pop-if-noop (`:1290,1501,1760,1874,1206,1270`) | yes |
| track rename (playlist), pattern rename (menu) | on apply/Enter (`playlist.rs:399`, `menu.rs:388`) | yes (clean; copy this pattern to the two stray fields above) |

## Deliberately un-undoable (no change needed)

- `load_default_kit` quiet loads (`library.rs:126–148`, `mod.rs:1004–1039 quiet` path): no checkpoint, no `edits` bump — project stays saved. Intentional.
- Audio-record takes (`record.rs:finish_takes`): write WAVs to disk, never touch `song` — nothing to checkpoint.
- `collect_plugin_states` / `shutdown/unload` / `reopen_plugins`: host sync, not user edits.
- Capture-mode scaffolding mutations (`mod.rs:383–537`): test harness, not user actions.

## Suggested fix order

1. VST params: checkpoint-on-gesture-start + persist `state` bytes into `song` so undo actually reverts them (biggest FL-parity gap).
2. Rack insert wheel/drag grouping + tap-tempo single-step (both one-line: reuse 0.6 s `WHEEL_GESTURE` / tap-run grouping).
3. Rename fields: focus→first-change checkpoint + missing `publish()`.
4. Rack paint no-op branch removal; file-drop double-step join.
