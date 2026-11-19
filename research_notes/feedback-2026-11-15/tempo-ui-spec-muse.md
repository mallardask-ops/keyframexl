# Tempo + meter UI spec (steps 5–6, design only)

Style: `docs/ui-rules.md` §§1–5 + `src/theme.rs` tokens only. No code changed.
Notation: `BAR` = 4/4 ticks; `STEP` = 1/16. All song ticks absolute.

## 0. Shared vocabulary

- Tempo track = song-level data (`Song.tempo_map`), not a pattern clip.
- Meter list = `Song.meters`, one entry per bar where the signature changes.
- Colours below are exact tokens; harcoded `Color32::from_*` forbidden (§5 ui-rules).

## 1. Tempo lane in the playlist

**Decision: pinned lane above track 1, not a clip.** Tempo is song-level
(like FL's tempo track); a clip could be muted/moved/looped, all wrong for
`tick_at(frame)` math. The lane scrolls sideways with the grid, never
vertically; it survives track mute/solo and clip edits.

```
┌ menu 26 ────────────────────────────────────────────────────┐
│ deck row1 48 / row2 34                                       │
│ TRACKS head 120 │ ruler 24 (bar numbers + meter flags §2)    │
│               │ TEMPO 48  ← pinned lane (this spec)          │
│ track 1 32  │ clips…                                        │
│ track 2 32  │ clips…                                        │
└─────────────────────────────────────────────────────────────┘
```

- Geometry: full grid width, height **48 px** (expanded) / **16 px**
  (collapsed, toggle in `▾` panel-header menu, default expanded).
  Background `SUNKEN` + 1 px `EDGE` outline like any well (`kit::sunken`).
  Left head cell (120 px): `caps` "TEMPO" mono 12 `FAINT` + current value
  mono 12 `TEXT` (e.g. `128.0`), right-aligned value like deck LCD ghosts.
- Scale: fixed **40–300 BPM** mapped to lane height with 4 px top/bottom
  padding. Y never auto-zooms (FL parity: readable jumps matter more than
  resolution). Gridlines at 60/120/180/240 BPM, 1 px `LINE gamma 0.45`.
- Curve: polyline through points, `ENC_BLUE` 1.6 px (tempo's address colour;
  `ENC_GREEN` is taken by plugin-parameter automation in `paint_clip`).
  - Step segment: horizontal then vertical riser at the next point's tick.
  - Linear segment: straight diagonal; ramp region gets a 12 % `ENC_BLUE`
    fill below it (same quad-fill technique as `automation.rs:169-173`).
  - Points: filled circles r 4.5 (`ENC_BLUE` ring `EDGE` 1 px), hover/selected
    r 6.0 white `TEXT` — same hit sizes as `automation.rs:175-184`.
- Collapsed 16 px: no fill, no gridlines; line 1.4 px + points r 3.0 only.
  Value still readable in the head cell.

**Gestures** (mirror `automation.rs:189-250`, one row, no Alt-tension):

| Gesture | Action |
|---|---|
| Click empty lane | Add Step point at snapped tick, value = interpolated map value there |
| Drag point body | Move in tick + BPM; tick clamped between neighbours ∓ 1, BPM clamped 40–300 |
| Right-click point | Delete (disabled when it is the only point) |
| Double-click segment | Toggle Step ↔ Linear for the segment starting at its left point |
| Small `S/⌁` chip at lane right | Toggle shape of selected point (keyboard-discoverable alternative) |

- Snapping: global Snap (`deck row2`), `Alt` bypasses to 1 tick — same as
  clips (`playlist.rs:742`) and roll (`ui-rules.md §3`).
- Readout: hover shows `128.0 BPM @ 5.1.1` (mono 11 `MUTED`, like
  `automation.rs:187`); selected point shows value in head cell while dragging.
- Playhead: 1.5 px `TEXT` line + ruler caret pass through the lane
  (same painter as `playlist.rs:996-998`), no second playhead.

## 2. Time-signature markers on the ruler

Bars keep **uniform pixel width** (`bar_w` 24–400, FL-like, plan §4);
a 3/4 bar is not narrower — it just contains fewer steps. Only the
tick↔x mapping becomes bar-index-aware (`bar_start`/`bar_len` in `tempo.rs`).

```
ruler 24px: ┌────┬────┬────┬────┬────┐
            │ 1  │ 2  │3/4 │ 4  │END │
            │    │    │▲3/4│    │    │
```

- Marker: 14 px-wide flag at the bar's left edge: `caps` text `3/4`
  mono 12 `TEXT` on `WAIT` (amber = structure, not status; `WAIT` is already
  the slide/trailing-pattern colour per `ui-rules.md §2.1.5`).
  Height 12 px, top-aligned in the 24 px ruler; bar number stays below it
  (number moves right 16 px when a flag is present). Unchanged bars show
  only the number, mono 12 `MUTED`, `TEXT` every 4th (`playlist.rs:352-358`
  unchanged).
- Grid: strong line `STRONG×0.6` at every bar start stays; beat lines inside
  the bar follow the meter: 3/4 → 3 quarter lines, 7/8 → 7 eighth lines
  (thin `LINE`). Beat `k % 4` accent logic elsewhere must read accents from
  bar starts, not `k%4` (plan §4 metronome).
- Add: `Alt+click` ruler on a bar without a marker → popup menu
  (`kit::popup_menu`, same as clip `▾`): Num `1–16` + Den `1/2/4/8/16`
  rows + Apply. Edit: click existing flag → same popup preselected.
  Delete: popup → `Back to 4/4` (markers are sparse; deleting = removing the
  row, bar falls back to the previous signature, never an explicit 4/4 row
  except bar 0). Right-click flag = delete directly (roll canon:
  right-click always deletes, `ui-rules.md §3`).
- Snap inside odd bars: beat snap divides the *actual* bar length
  (`bar_len/num`), so 1/4 snap in 3/4 gives 3 slots, not 4. Bar snap still
  lands on bar starts.

## 3. Transport LCD

LCD geometry unchanged: 272×42 well, `SUNKEN` r8, `seg_text` h18,
`LCD_ON = TEXT`, ghosts `LCD_OFF` (`deck.rs:278-320`).

- **BAR:STEP field**: bar = sig-aware bar index + 1, step = tick-in-bar /
  `STEP` + 1. In 3/4 it counts `01–12`; in 7/8 `01–14`. No zero-padding change
  (`{:02}` fits 16 max; 7/8 fits).
- **BPM field**: shows `bpm_at(now_tick)` while playing (moves with ramps,
  1 decimal, same format `{:5.1}`), frozen map value at playhead when
  stopped. Ghost segments stay visible.
- **The `4/4` label** (top-right of LCD, `caps` 12 `FAINT`, `deck.rs:318`):
  shows the signature **at the playhead** (`3/4`, `7/8`, …). When the map is
  non-flat (≥2 points or any Linear segment) a 4 px `ENC_BLUE` dot appears
  left of the BPM digits = "map active" indicator. Colour = address, shape =
  state (`ui-rules.md §1.2`); never red/green.
- **Editing BPM digits when a map exists — decision: edit the point at the
  playhead (FL behaviour).** Drag/wheel/type/right-click keep their gestures
  (`deck.rs:322-461`), but the write target changes:
  - Flat song (map empty): writes `Song.tempo` exactly as today.
  - Map exists: the edit writes the tempo **event in force at the playhead
    tick** (move playhead → select which event). If the playhead sits inside
    a Linear ramp, the edit first splits the ramp: left point keeps its value,
    a new Step point is inserted at the playhead tick with the edited value.
  - Rationale (FL parity): FL's tempo knob edits the tempo event at the
    playhead; there is no "scale whole map" knob on the transport. Whole-map
    scaling (×0.5/×2) belongs in the tempo lane's `▾` menu, not on the LCD,
    because it is destructive and needs a visible scope. Status line confirms:
    `Tempo event @ 5.1.1 → 132.0 BPM. Ctrl+Z undoes.`

## 4. Piano roll / channel rack: what changes

- **Roll ruler**: pattern-local ticks are untouched (patterns stay 4/4
  grids, plan §3). Only the *position readout* and the song-playhead
  conversion change: `bar.beat` labels come from the sig table when the roll
  is previewing song position. No per-bar width change inside the roll.
- **Channel rack**: nothing changes. Steps are pattern-local 1/16 at C5
  (`ui-rules.md §3`); odd meters only affect how many steps of a pattern fit
  in one playlist bar, which the clip-repeat ticks already visualise
  (`playlist.rs:594-600`). No step-count change, no meter UI in the rack.
- **What does NOT change**: pattern length rounding (`roll.rs:329`),
  `BAR`-as-4/4 pattern grid, ghost notes, velocity lane, slide editing.

## 5. Undo + edge cases

One `checkpoint()` per gesture, matching knobs/clips:

| Gesture | Undo step |
|---|---|
| Add / drag / delete tempo point, toggle shape, whole-map scale | 1 each (drag = press→release) |
| Add / edit / delete meter flag | 1 each |
| LCD BPM edit (drag run ≤0.6 s, typed value, stepper press) | 1, same grouping as `deck.rs:411-414,338-356` |
| Tap tempo | writes tick-0 point (or `Song.tempo` if flat), 1 step |

Edge cases:

1. **Deleting the tick-0 point**: forbidden — lane menu offers `Reset`
   instead (single Step point at edited value). Rationale: tick 0 always
   needs a defined tempo; `normalize_tempo_map` would re-add the fallback
   anyway (`tempo.rs:128-133`).
2. **Meter change inside a clip**: allowed. Clips are tick spans; notes do
   not move. Only the ruler/grid and the clip's internal repeat ticks
   re-render. No silent note shifting.
3. **Pattern longer than the bar** (e.g. 16-step pattern starting in a 3/4
   bar): pattern plays its full ticks; the bar line just passes mid-pattern
   (FL behaviour). No truncation, no re-quantise.
4. **Overlapping edits**: tempo points ≤1 tick apart are rejected by clamp
   (neighbours ∓ 1); meter on bar ≥ `MAX_BARS` rejected with `Tone::Bad`
   `Bad time signature`, same string as `validate_meters`.
5. **Empty map + old files**: lane shows a single ghost point at
   `Song.tempo` (non-interactive until first click adds it as a real point).
