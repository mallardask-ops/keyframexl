# Piano roll + Playlist research (FL parity target)

Task complaint: our piano roll and playlist work on rigid ticks/steps and take too
many clicks. FL screenshots 2-3 in `research_notes/brief-2026-11-15.md` show the
arrow-menu-driven FL workflow we must match in feel, not just in features.

Grounding: every FL claim below cites an Image-Line manual page. Anything I could
not verify in the manual is marked UNVERIFIED. Code citations are `file:line`.

Manual pages used (all under
`https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/`):

- `pianoroll.htm` — piano roll basics, tools, gestures, ghosts, slides, note props
- `pianoroll_menu.htm` — full piano roll arrow-menu reference
- `playlist.htm` — playlist basics, tools, menus, clips, markers, picker
- `playlist_performance.htm` — performance mode
- `channelrack.htm` — graph editor per-step properties (Ctrl+K)
- `chansettings_misc.htm` — MODX/MODY, keyboard/velocity tracking
- `chansettings_ins.htm` — filter section (Mod X = cutoff, Mod Y = resonance)
- `basics_shortcuts.htm` — keyboard/mouse shortcuts
- `basics_interface.htm` — scroll/zoom bars, PPQ vs zoom resolution, scroll speed
- `support.image-line.com/action/knowledgebase?ans=47` — snap how-to

---

## 0. Where we are (what exists, file:line)

Live model (`src/sequence.rs`):

- `Note { start: u8, length: u8, pitch: u8, velocity: u8, group: u8, instrument: u64 }`
  (`sequence.rs:24-33`). `Glide { start, length, target, group, instrument }` (34-42).
- `STEPS: usize = 16` (`sequence.rs:13`). One pattern = 16 steps = 1 bar, no length option.
- `Clip { pattern: u64, bar: u32 }` (`sequence.rs:83-87`). Clips are whole-bar,
  bar-quantized, no offset/length/mute.
- `validate()` rejects `start >= 16`, `start+len > 16` (`sequence.rs:282-296`).

Piano roll (`src/ui/roll.rs`):

- `Geom::locate` clamps to 16 steps: `step.min(15)` (`roll.rs:80`).
- `snap_step(v: u8, grid: u8, alt: bool)` (`roll.rs:85-90`); `move_target` all-`u8`
  (`roll.rs:94-115`).
- Draw = click adds length-1 note (`roll.rs:535-544`); drag-draw sets length by drag
  span (`roll.rs:521-532`); move or right-edge resize only, resize-from-left absent
  (`roll.rs:454-468`); right-click erases (`roll.rs:546-567`).
- `occupied()` vetoes any overlap (`roll.rs:408-420`) — no chords on one pitch lane
  issue aside, this also blocks strum/flam-style micro-overlaps and slide overlap.
- Velocity lane only: lollipop stems at note starts, top-voice-per-step
  (`roll.rs:570-616`). No pan/release/pitch/mod lanes, no per-note editing.
- No selection (single `roll_gesture: Option<(u64, Gesture)>` in `ui/mod.rs:99`),
  no clipboard, no tools beyond Draw/Slide (`ui/mod.rs:32-37`), no arrow menu.
- Snap is 3-way global: `Snap::{Step, Beat, Bar}` → grid `1/4/16` (`ui/mod.rs:39-62`).
- Ghosts: other instruments' notes drawn as outlines (`roll.rs:315-319`); no
  out-of-pattern ghosts, no channel switching, no editable ghosts.
- Wheel: scrolls pitch, Ctrl+wheel changes row height (`roll.rs:204-214`).

Playlist (`src/ui/playlist.rs`):

- `add_clip` appends selected pattern after last clip (`playlist.rs:26-33`).
- Clips move whole-bar only, `bar_at` floors to bar (`playlist.rs:148,205-220`).
  No resize from either edge, no slip, no slice, no offset, no mute, no selection
  (single `self.clip: Option<usize>`), no snap setting at all.
- Track headers are generic `Track N` lanes auto-stacked by bar collision
  (`playlist.rs:92-124`); no names, colors, mute LEDs, lock, size, grouping.
- Context menu has 4 items only (open rack / open roll / unique / delete,
  `playlist.rs:233-250`). No Clip Menu (preview, swap source, chop, consolidate).
- No picker panel, no arrangements, no time markers, no performance mode,
  no automation/audio clips.

Draft format 2 (`src/core/`):

- `time.rs:4`: `TICKS_PER_BEAT = 480`, `Tick = i64`, `quantize()`, `step_ticks()=120`.
- `model.rs:57-68`: `Note { id, start: Tick, len: Tick, pitch, velocity, muted,
  slide_group, color, channel }`. Present: id, muted, color (= FL color group),
  channel (= FL MIDI channel). **Missing vs FL: pan, release, fine pitch,
  mod X, mod Y, slide-vs-porta flag** (§4).
- `model.rs:70-77`: `Slide { id, start, len, target, group }` — no porta flag.
- `model.rs:79-84`: `Clip { id, pattern, start_ticks }` — no length, offset,
  mute, fades.
- `select.rs`: ID-based `Selection` + `Clipboard` exist but are unused by any UI.
- `cmd.rs`: command stack with gesture coalescing (`MoveNotes`/`SetVelocity`
  merge by gesture id, `cmd.rs:156-177`) — matches FL's one-gesture-one-undo.
  Missing commands: resize-length, transpose/nudge, quantize, duplicate,
  clip commands.

---

## 1. Time model: PPQ, snap, free placement

### FL facts

- Default project timebase is **96 PPQ**, adequate for most situations; higher PPQ
  gives greater zoom resolution (`basics_interface.htm`). Maximum selectable PPQ
  value: UNVERIFIED (task statement says up to 960; manual page for Project
  General Settings not fetched).
- Snap is a **per-window view setting** (piano roll, playlist, event editor each
  have a magnet icon), plus a Global snap used when local snap = Main
  (`pianoroll_menu.htm` Snap, `playlist.htm` Snap, KB ans=47).
- Snap options, identical in both windows (`pianoroll_menu.htm`; playlist defers to
  it): **Main, Line, Cell, (none), 1/6 step … 1 step, 1/6 beat … 1 beat, Bar,
  Events, Markers** (playlist adds nothing; piano roll also lists Markers).
- Semantics (`pianoroll_menu.htm`):
  - **Line**: snap to nearest grid line; grid resolution **follows horizontal
    zoom**.
  - **Cell**: snap to start of the grid cell the event falls in (not nearest).
  - **(none)**: free movement limited only by PPQ.
  - **Steps/Beats**: absolute grid fractions (1/4 step = 64th notes … Bar = whole
    note; conversion table in manual).
  - **Events**: snap **relative** to existing events (incl. ghosts); surrounding
    events change color to show boundaries.
  - **Markers**: snap to time markers.
  - **Alt held = snap temporarily off** in every drag/resize/slice operation.
  - **Scroll speed slows when snap is anything but Line** (`basics_interface.htm`).
- Note position dialog shows `bar:step:tick`; tick size depends on PPQ, and
  **reducing PPQ mid-song repositions off-grid notes** (`pianoroll.htm`, Note
  Properties). I.e. FL stores absolute ticks too — PPQ is resolution, not grid.
- MIDI input quantize follows the Global snap; `(none)`, Line, Cell do not quantize
  input (`pianoroll_menu.htm` Snap note).

### Why ours feels rigid (exact causes)

1. Position unit is `u8` step 0..15 (`sequence.rs:24-33`, `roll.rs:80,85-115`).
   Nothing between steps exists, so triplets, strums, humanized timing,
   off-grid recording are unrepresentable — not just uneditable.
2. Only 3 snap grids (`ui/mod.rs:46-62`); no Line (zoom-following), no Cell,
   no (none), no absolute fractions, no Events/Markers. `grid()` returns `u8`.
3. Snap is global (`App.snap`, `ui/mod.rs:95`), not per window; no Main/global
   concept.
4. `occupied()` (`roll.rs:408-420`) + bar-floor clip moves (`playlist.rs:148`)
   make the grid feel like a prohibition, not a guide.
5. Pattern is fixed 16 steps / 1 bar (`sequence.rs:13`, `roll.rs:220`
   `cell = width/16`); no multi-bar patterns, no zoom-dependent grid.

### Proposed model

- Adopt `src/core` format 2 as the live model: **`Tick = i64`, 480 PPQ**
  (`core/time.rs:4-6`). Rationale: 480 is divisible by 2,3,4,5,6,8 (exact triplets
  and 1/6 steps: 480/3=160, 480/6=80), exact import of 96/240/480 files, matches
  existing `migrate.rs:7-9` (1 step = 120 ticks, bar = 1920). 960 adds nothing
  musical over 480 (both below any human timing need) and doubles nothing but
  numbers; keep 480. PPQ stays a **project constant**, never a grid.
- **Free placement by default in the data**; snap is a per-window view setting
  (`SnapSetting { mode, global }`) applied in `x_to_tick`/`tick_to_x` and in
  move/resize/slice/paint math only. Absolute snap for all modes; relative-to-event
  only in Events mode.
- **Alt bypasses snap in every drag** (already partially done in
  `roll.rs:436,477` — extend to all gestures and playlist).
- Line mode: effective grid = max(snap floor, pixel-quantum from zoom), e.g.
  `grid_ticks = max(1, round(ppq_per_pixel * 8))`; grid lines redraw on zoom.
- Keep `core/time.rs::quantize` (rounds to grid) for explicit Quantize commands;
  interactive drags use floor/nearest per mode (Line=nearest, Cell=floor).
- Migration: `migrate_v1` already maps step→120 ticks (`core/migrate.rs:66-70`).
  Promote `core` to live model per `docs/roadmap.md:31` before editor work.

---

## 2. Piano roll tools and gestures (full inventory)

Source: `pianoroll.htm` (toolbar + gestures + notes sections).

### Toolbar tools (left to right in FL)

| Tool | Key | Behavior + modifiers |
|---|---|---|
| Tools (wrench) | — | L-click: Tools submenu; R-click: Chord Progression tool; M-click: Riff machine |
| Snap magnet | — | L-click: snap menu; **R-click: set default note length** (also set by clicking any existing note) |
| Snap to scale | — | L: toggle; R: scale/root menu (§Scales). Dragged notes snap to scale; input quantize option |
| Stamp (note icon) | — | chord/scale/percussion stamp; Only-one toggle; auto-chord top-down/bottom-up |
| Draw | P | L-click add (+drag to place before release); drag length with **Shift**; R-click delete, R-drag mass delete; R-hold = slice tool; **Ctrl** = select; **Alt** = draw with snap none |
| Paint | B | drag paints rows of notes; monophonic-replace by default, **Shift** = free vertical; **Ctrl** = select |
| Paint drum seq | N | click toggles/mutes steps; **Alt** = half-snap paint |
| Delete | D | click/drag delete; **Ctrl** = select |
| Mute | T | click/drag mute; R-click deletes |
| Interpolate | I | event-data only: drag horizontal ramp across genuine Event data |
| Slice | C | drag vertical cut line; **Shift** = click-slice no drag; **Shift+Alt** = no-drag + no-snap; glue via Ctrl+G |
| Select | E | click/drag select; **Shift** adds/removes |
| Zoom | Z | drag = zoom to rect; click background = out; **Ctrl+R-click-drag** zooms in any mode; R-click toggles; R-click icon = quick-zoom menu |
| Play selected | Y | click plays notes; drag = scrub (order + speed from drag) |
| Play/Pause | Space | L toggles; R while playing = stop+return; R while stopped = return to start |

We have: Draw (no Shift-stretch, no last-length memory, no Ctrl-select),
right-edge resize only, R-click delete, velocity lane. Missing: everything else.

### Note gestures (operating with notes)

- Add: default length = **last touched note length** (we always add len 1,
  `roll.rs:538-544`).
- Move/resize from **both edges** (resize-from-left is an opt-in:
  `Edit > Allow resizing from left`, Ctrl+Alt+Home). Cursor becomes
  double-arrow at either edge.
- **Shift+drag = duplicate** (copy-drag); in playlist: Shift+V+drag on clip
  (`playlist.htm` Clone Clips). **Ctrl+drag = select** in any tool.
- **Click-and-hold** (F10 option): hold on note = copy; hold while adding =
  cycle slide/porta; hold between two touching notes at resize cursor = glue.
- **Alt+R-click drag in Draw = scrub playback**.
- Vertical lock: press note, then **Shift** + horizontal drag (keep pitch).
  Horizontal lock: press, then **Ctrl** + vertical drag (keep timing).
- Nudge: **Shift+arrows** = move by snap unit; **Alt+arrows** = pixel nudge
  (zoom-dependent); mouse-wheel over note with Shift (or Shift+Alt default) =
  nudge by PPQ; **Ctrl+arrows** = octave jump (also Edit menu transpose ±octave).
- Stretch: select + drag pop-up Stretch Handle; or **Right-Shift** + drag right
  edge of a note in selection (changes note lens + spacing); **Alt+R-Shift** =
  25% increments (playlist Rescale: same, `playlist.htm`).
- **Double-click note = Note Properties dialog** (§4 end).
- **Alt+wheel over note = edit current Target-control property** for that note
  (velocity default); **Ctrl+Alt+wheel = fine**. Max zoom to see values.
- Double-click / Ctrl+click **ruler** = time-range select; Ctrl+click preview
  keyboard = pitch-range select.
- **Ctrl+B duplicate**: selection or all, to the right; with independent
  time-line selection defining the repeat interval (notes outside it still dup).
- **Shift+G group / Alt+G ungroup**; toolbar group switch enables/disables.
- Ghosts: **double-R-click** a solid ghost = switch to its channel;
  Ctrl+Alt+V editable ghosts.

### Note properties (Target control, `pianoroll.htm` Note Properties + Event Editor)

Per-note, shown as lollipops in the bottom editor, moving with their notes:
**Velocity, Pan, Release (where supported), Mod X, Mod Y (where supported),
Pitch**. Plus note flags: **mute, slide on/off, porta on/off, color group
(16)**. We store velocity + muted + color + channel only (`core/model.rs:57-68`).

### Note colour / MIDI channel

16 color groups = 16 MIDI channels as seen by the hosted plugin; hover shows
channel in hint bar; palettes saveable (`View > Note colors`); some plugins use
colors functionally (Harmor etc.). Recording with "MIDI channel through" sorts
incoming notes into colors. — Our `color: u8` + `channel: u8` fields
(`core/model.rs`) already anticipate this; UI must expose the 16-color selector.

### Slide / Porta (native instruments only)

- **Slide note** (toggle S): must overlap a sounding note; pitch glides toward
  slide pitch over the slide-note length; topmost note is chord reference;
  same-color-only sliding (16 independent slides); carries its own note props.
- **Porta flag** (toggle O): fast slide at note transition only, set on the
  receiving note.
- Our `Glide`/`Slide` = slide to target only (`sequence.rs:34`, `core/model.rs:70`):
  no porta flag, no same-color rule, no chord-topmost rule. Engine treats slides
  per instrument already (`sequence.rs:482-495` test).

### Ghost notes

Solid blocks = same-pattern other channels (switchable via double-R-click);
open blocks = other patterns overlapping in playlist (position reflects clip
placement; **which pattern's clip you opened decides** what ghosts show).
`View > Ghost channels (Alt+V)`, editable ghosts (Ctrl+Alt+V), per-channel and
per-track ghost toggles. — We draw solid ghosts only (`roll.rs:315-319`).

### Key / scale highlighting

`View > Note grid highlights (Alt+B)` extends key shading; `Scale highlighting`:
Default / Black notes / Automatic (detects scale, names it in hint bar) /
any scale+root; snap-to-scale constrains add/move/record; custom scales are
`.fsc` files. Piano-roll keyboard styles (Classic/Modern/Flat/List), key labels
(none/all/root/highlighted/white-only), custom key rename for drum maps.

### Preview

Click preview keyboard = preview channel (not while playing); middle-drag on
keyboard = vertical zoom; `Preview notes during playback` menu toggle; playhead
caret + line; `Center (Shift+0)`.

---

## 3. Piano roll arrow menu (every item)

Source: `pianoroll_menu.htm`. FL shortcut notation below is Windows/Cmd-mac
as in the manual.

**File** — Open score (.fsc); Save score as (.fsc; drag the menu item to
browser/another roll to copy); Import MIDI (Ctrl+M, notes only); Export MIDI
(Shift+Ctrl+M); Paste/Copy MIDI clipboard (Shift+Ctrl+V / copy); Export score
sheet → PDF notation.

**Edit** — Cut Ctrl+X, Copy Ctrl+C, Paste Ctrl+V (paste at leftmost **visible**
bar), Duplicate Ctrl+B, Delete Del (no selection = all); Shift left/right
(Shift+arrows, by snap); Rotate left/right (Shift+Ctrl+arrows, wrap in bar);
Transpose up/down (Shift+Up/Down), Octave (Ctrl+Up/Down); Discard lengths
(Shift+D, shrink to snap minimum); Allow resizing from left (Ctrl+Alt+Home);
Nudge with mouse wheel (Shift+wheel, PPQ steps); Change color (Alt+C);
Mute/Unmute (Alt+M / Alt+Shift+M); Insert space (Ctrl+Ins, shift followers
right); Slice & insert space (Ctrl+Alt+Ins); Delete space (Ctrl+Del, close gap);
Trim selection (delete outside, move selection to start); Insert current
controller value (Ctrl+I); Turn into automation clip.

**Tools** (shared with main Tools menu) — Riff machine (Alt+E); Generate chord
progression (Alt+P); Quick legato (Ctrl+L); Articulate (Alt+L: staccato/legato
preset dialog); Quick quantize (Ctrl+Q per manual — elsewhere shown as
Ctrl+U? mark conflict UNVERIFIED, manual says Quick quantize Ctrl+Q… actually
manual text: "Quick quantize (Ctrl/Opt+Cmd+Q)"; Quick quantize start times
(Shift+Q); Quantize dialog (Alt+Q); Quick chop (Ctrl+U); Chop dialog (Alt+U);
Glue (Ctrl+G); Arpeggiate (Alt+A, score presets); Strum (Alt+S, time+velocity
spread dialog); Flam (Alt+F); Claw machine (Alt+W, slice+retime); Limit (Alt+K,
key-range transpose); Flip (Alt+Y, vertical/horizontal mirror); Randomize
(Alt+R, notes and/or levels); Scale levels (Alt+X, mul/offset/invert/log-map
event values); LFO (Alt+O, writes event shapes; needs event target selected);
Script… (Python, `flpianoroll` API) + re-run (Ctrl+Alt+Y) + script list.
Dialog internals (strum ms ranges, chop counts, randomizer rows): UNVERIFIED.

**Stamp** — chord/scale/percussion/special presets + Only-one toggle (§2).

**View** — Grid color (Ctrl+Alt+G), contrast, invert, time segments; Keep labels
on screen, Shadow, Rounded, Note-colors palette editor + presets; Incremental
scrolling, Precise time indicator, Mini preview on scrollbar; Note grid
highlights (Alt+B), Scale highlighting (+ custom .fsc scales), Note length in
properties; Ghost channels (Alt+V), Editable ghosts (Ctrl+Alt+V), Background
waveform (Alt+N); Keyboard style, Keyboard labels, Swap panels,
Flip separator (Shift+Enter).

**Snap** — as §1 (Main/Line/Cell/(none)/1/6 step…Bar/Events/Markers).

**Select** — Deselect Ctrl+D; Select all Ctrl+A; One at random Shift+R; More at
random Shift+M; By color Shift+C; Odd (off-grid) Shift+O; Muted; Overlapping;
Stacked; Invert Shift+I; Time around selection Ctrl+Enter; Prev/next time
Ctrl+Left/Right; Magic lasso (draw loop, no click).

**Group** — Group Shift+G; Ungroup Alt+G (+ toolbar group switch).

**Zoom** — In/Out PgUp/PgDn (to cursor); Quick 1/2/3 (Shift+1..3); Out far
(Shift+4); On selection (Shift+5).

**Time markers** — Add one (Alt+T, name+drag); Add auto (Ctrl+T); Add two
(selection ends); One every bar/2/4/8 (needs selection); Time-signature change;
Key marker; Delete (selection or all). Marker types on R-click: none/start/
loop/marker-loop/marker-skip/marker-pause/time-signature/start-stop recording.

**Target channel** — channel dropdown (ticks = has notes); wheel-hover preview;
Ctrl+wheel shows all; auto-locate first non-empty on pattern switch.

**Target control** — event lane target (note props + automation).

**Auto smoothing** — interpolate+smooth events on mouse release.

**Preview notes during playback**; **Center (Shift+0)**; **Detached**.

---

## 4. Event editor (control lane)

Sources: `pianoroll.htm` (Note Properties & Event Editor), `channelrack.htm`
(Graph Editor), `chansettings_*.htm` (MODX/MODY meaning).

- **Targets** (`Target control` menu, cyclable with Shift+F, wheel-hover):
  note Velocity, Pan, Release, Mod X, Mod Y, Pitch per note + pattern automation
  events (cutoff/resonance etc.). Lollipop display; same-start notes disambiguated
  by pre-selecting; tail shows independent values.
- **Gestures**: click/drag paint values; **Shift+drag = set all to same**;
  **Alt+click = reset**; Interpolate tool (I) ramps genuine Event data;
  Tools > Scale levels / LFO operate on lane data; Alt+wheel / Ctrl+Alt+wheel
  coarse/fine adjust under cursor.
- **Note Properties dialog** (double-click note; applies to whole selection
  except Time): PAN, VEL, REL, MODX, MODY levels; Slide on/off; Porta invert;
  Color; **Time bar:step:tick + Duration bar:step:tick** (tick = PPQ unit);
  Reset/Accept.
- **Semantics**: Mod X = filter cutoff, Mod Y = resonance on stock sampler
  (`chansettings_ins.htm`); keyboard/velocity trackers route to PAN/CUT/ModX/Y
  with MID reference (`chansettings_misc.htm`); Release velocity supported by
  Sytrus/Harmor/Harmless/Ogun-class instruments.
- **Channel-rack graph editor** (Ctrl+K, `channelrack.htm`) is the step version:
  per-step Note / Velocity / Release / Fine pitch (±100c) / Mod X / Mod Y /
  Shift; L-drag, Ctrl-scales-all, R-drag ramp. Our roadmap §1 wants this per
  channel once steps generalize.
- We have: velocity lane for top voice per step only (`roll.rs:570-616`).
  Missing: lane-target selector, all other props, Shift/Alt gestures,
  interpolate, properties dialog, selection-aware editing.

---

## 5. Playlist

Sources: `playlist.htm` (all), `playlist_performance.htm`, `basics_shortcuts.htm`.

### Arrow menu (every item, `playlist.htm` Playlist Menu section)

**Edit** — Cut/Copy/Paste/Duplicate/Delete (Del, none = all); Shift L/R by snap
(Shift+arrows), Shift U/D by track; Allow resizing from left (Ctrl+Alt+Home);
Nudge with wheel; Dropping-on-header locks to content; Drop-audio options (as
audio clips / audio tracks / instrument tracks / always ask); Mute/Unmute;
Insert space (Ctrl+Ins) / Slice+insert (Ctrl+Alt+Ins) / Delete space (Ctrl+Del);
Merge pattern clips (Ctrl+G), Merge similar (Shift+Ctrl+G, new pattern),
Merge automation (Ctrl+Alt+G, 3 link options); Pitch audio ±semitone (7/8),
Reverse audio (9); crossfade/fade-handle/variant/track-select options.

**Tools** — Quick quantize start times (Ctrl+Q); Consolidate selection (From
selection start Ctrl+Alt+C / From song start Shift+Ctrl+Alt+C; source clips
muted; render dialog for FX); Export all tracks / track groups (stems; 3 range
options); Clone track(s) (Shift+Ctrl+B, choose patterns/audio/automation/FX/
groups).

**View** — Grid color/contrast/invert, separators, time segments, keep labels,
title-bar content, clip backgrounds (Nothing/Plain/Cel/Glass/Aqua/Solid),
shadow, resize all tracks 33–200%, hide collapsed groups, fade/gain previews,
incremental scrolling, precise indicator, performance progress ×2, mini preview
(+double height, +markers), audio/instrument track controls+levels, background
picture.

**Snap / Select / Group / Zoom** — same vocabulary as piano roll (§3) with
clip wording (Select by selected source Shift+C; overlapping/stacked clips;
Shift+5 selection, Shift+6 performance zone). Playlist Zoom tool = Shift+Z(?)
manual lists "Zoom to selection (Shift+Z)" icon but shortcuts table says
Ctrl+R-click; keep both, mark conflict UNVERIFIED.

**Time markers** — full action-marker system: add/auto/two/every/jump-to-next/
time-sig/loop-place/delete; types None/Start/Loop/Marker-loop/Marker-skip/
Marker-pause/Time-signature/Start-stop-recording; song-loop marker; live jump
(MIDI channel, number-pad shortcuts, Live mode bar-quantized jumps).

**Picker panel** (Alt+P) — clip manager: filters All/Name/Color/Mixer,
select/rename/recolor/gradient, unused select+purge, freeze/render-in-place,
reorder (drag or Shift+wheel — renumbers patterns!), transpose, Shift+C locate,
Alt+R-click preview. Options: right side, width, select-linked-channels,
select-from-playlist, show-empty, drag-rearrange, auto-group, select-unused.

**Current clip source**; **Edit patterns in** (Automatic / Channel rack /
Piano roll / Piano roll-or-event-editor / scroll-to-position; Alt+dbl-click =
opposite); **Quantize song jumps** (Off/Beats/Bars 1–8);
**Automation clips reset remote controls**; Prepare for performance mode;
**Performance mode (Ctrl+P)**; declicking switch; timestretch switch;
lock/unlock-all-to-content; reset empty tracks; collapse/expand groups;
**Center (Shift+0)**; **Detached**.

### Tracks

Headers: name (F2-style rename), color, icon, mute LED (L), solo (Ctrl/R-click),
group mute/solo (Alt+click), lock (Shift+click), size %, lock-to-size, group
with above (nested sub-tracks), auto-name (+clips), Track Mode
(Unassigned/Instrument/Audio: drop instrument/sample on header links
channel↔track↔mixer incl. name/color/icon ripple), lock-to-content, merge,
consolidate (Ctrl+Alt+C / +Shift), mute/unmute all clips, insert/delete,
clone, move (Shift+wheel), record arm + input on audio tracks, peak meters.
— We have numbered lanes only (`playlist.rs:117-124`).

### Clip types & operations

Pattern (notes+events), Audio (fades, stretch, zero-cross, reverse, pitch),
Automation (points+tension, slide-preserve). Any type on any track, overlaying
with transparency; Clip Focus tabs (M cycles) pick which stack level edits;
`+` adds instrument/audio tracks. Operations: insert (draw/paint), move (snap;
Alt free), **resize both edges**, **slip (S)**, ripple (Shift+S), rescale
(R-Shift + 25% steps on Alt), slice (C, vertical or Shift-diagonal, zero-cross
aware), copy-drag (Shift), duplicate (Ctrl+B + independent timeline interval),
chop into bar/beat (+random), swap source (Shift keeps bounds), make unique
(Shift+V+drag), merge, consolidate/freeze, group (Shift+G), mute tool vs track
mute independent, audition (play tool / Alt+R-click / clip menu preview).
Insert/delete **time** (Ctrl+Ins / Ctrl+Del on timeline selection).
— We have: place-at-end, whole-bar move, delete, make-unique (pattern clone),
4-item context menu.

### Arrangements / song position / zoom-scroll

Multiple arrangements (clone/rename/delete/merge with start/current/end ×
insert/merge/replace); play-position marker draggable; timeline selection
(Ctrl+drag ruler, Shift+drag copy-move; loop/render section); song loop via
markers; H-zoom via scrollbar edges or Ctrl+wheel (PPQ caps resolution);
V-zoom via dividers, Ctrl+R-click fit; MMB-drag pans both axes; MMB+R-click =
context tool menu; Shift+wheel over headers reorders tracks.

---

## 6. Interaction feel (spec-level)

- **Wheel**: over grid = vertical scroll; Ctrl+wheel = H-zoom at cursor
  (playlist) / row height (we do `roll.rs:204-214` — change to FL: wheel = V
  scroll, Ctrl+wheel = H-zoom, middle-drag = 2-D pan); Alt+wheel = note
  velocity/property; Shift+wheel = nudge/reorder depending on focus;
  Shift+Alt+wheel = free nudge. Playlist track area: Shift+wheel reorders.
- **Middle drag pans both axes** in roll/playlist/event editors
  (`basics_interface.htm`, both tool notes). MMB+R-click opens context tool menu
  at cursor — this is the click-count killer feature: full tool+menu access
  without travelling to the toolbar/arrow menu.
- **Scroll speed slows under coarse snap** (`basics_interface.htm`) — replicate:
  wheel step scales with effective grid.
- **Auto-scroll during drag**: when pointer nears edge while dragging notes/
  clips/selections, scroll + extend. (FL behavior, UNVERIFIED in manual text but
  visible in app; mark UNVERIFIED.)
- Hover feedback: edge = resize cursor (we do right edge, `roll.rs:447-452`;
  add left), note = grab, empty = pencil per active tool; ghost-channel hover
  hints; surrounding events recolor in Events snap.
- Min hit sizes: edge grab ≥8px (we use `EDGE_GRAB=8`, `roll.rs:18` — keep);
  lollipop heads r=4 (`roll.rs:589` — bump to ≥6px at 100%+ zoom); clip edges
  ≥6px; lane headers ≥ track-min-height.
- Cursors: pencil/grab/grabbing/resize-H/zoom/scrub per tool+hover; we set
  Grab/Resize/Grabbing (`roll.rs:450,479`) — extend per tool.
- **Undo granularity**: one finished gesture = one history entry; drags coalesce
  (our `core/cmd.rs:156-177` already does this — keep pattern, add gesture ids
  to every drag op). Ctrl+Z/Ctrl+Shift+Z or Ctrl+Y. FL counts paint strokes,
  dialog applies (quantize/chop/strum), nudges as single entries.
- **Performance**: FL handles 1000s of notes. egui is immediate-mode: cull to
  visible tick/pitch range (we already filter by pitch range `roll.rs:358`;
  add tick-range cull), batch fills (single mesh per color where possible),
  mini-preview as pre-rendered strip, avoid per-note text below threshold
  (we gate labels by size `roll.rs:371` — keep, extend to playlist content
  preview `playlist.rs:181-195`).

---

## 7. Concrete design for our code

### 7a. Data model changes (all in `src/core/`, migrate v1→v2 already in

`core/migrate.rs`)

1. `core/model.rs:57-68` `Note`: add `pan: i8` (−64..+63, 0 = center),
   `release: u8` (default 64/100? choose 100 = full, UNVERIFIED FL default —
   use 100 to match velocity convention), `fine_pitch: i8` (cents, default 0),
   `mod_x: u8`, `mod_y: u8` (default 0? FL default cutoff-open = 100% ModX on
   stock sampler; store raw 0..127, default 127/0 — decide in P1 spec, record
   choice), `porta: bool` (default false). Keep `muted, color (0..15), channel
   (0..15), slide_group`. Validate ranges in `Project::validate`
   (`core/model.rs:93-133`).
2. `Slide`: add `porta: bool` (slide vs porta-note).
3. `Clip` (`core/model.rs:79-84`): add `len_ticks: Tick` (default = pattern len),
   `offset_ticks: Tick` (default 0, for resize-from-left/slip window),
   `muted: bool` (default false). Validate `start+len` overflow-safe,
   `offset+len <= pattern len`.
4. `Pattern`: add `len_ticks` already exists; add optional `time_sig: (u8,u8)`
   later (P3) — not P1.
5. New `core/automation.rs` (P2): `AutoClip { id, param: ParamAddr-ish String,
   points: Vec<(Tick,f32,tension)> }` + playlist clip kind enum. P1 only reserves
   `ClipKind { Pattern, }` enum shape so P2 doesn't re-migrate.
6. `select.rs`: reuse `Selection`/`Clipboard` (extend clipboard with
   `Vec<Slide>` already there; add `kinds` for clips in playlist selection
   struct).
7. `cmd.rs`: add `ResizeNotes { ids, d_len, d_start, gesture }`,
   `TransposeNotes { ids, d_pitch, gesture }`, `QuantizeNotes { before: Vec<Note> }`,
   `DuplicateNotes { notes }`, `MoveClips/ResizeClips/SlipClips`,
   `SetNoteProp { field, before, after, gesture }`. All drag ops take the
   gesture id from `Record::begin_gesture` (exists `cmd.rs:187-191`).

### 7b. Editor state machine (new `src/ui/edit.rs`, shared by roll + playlist)

```rust
enum Tool { Draw, Paint, PaintDrum, Delete, Mute, Slice, Select, Zoom, Play, Slip /*playlist*/ }
enum SnapMode { Main, Line, Cell, Free, Step(frac), Beat(frac), Bar, Events, Markers }
struct SnapSetting { mode: SnapMode } // per window; Main -> App.global_snap
enum Gesture { DrawNew{...}, Move{ids, grab_tick, grab_pitch}, ResizeL{...},
  ResizeR{...}, Slice{from,to}, Select{rect, add}, Scrub{...}, Paint{...}, Slip{...} }
struct Editor { tool: Tool, snap: SnapSetting, selection: Selection,
  gesture: Option<(u64 /*gesture id*/, Gesture)>, clipboard: Clipboard,
  last_note_len: Tick, allow_resize_left: bool }
```

- One `Editor` per piano roll + one for playlist (replace `App.tool/snap/
  roll_gesture/clip` in `ui/mod.rs:94-99`). Global snap lives in deck (roadmap
  §5 P1: "global Snap").
- Commit path: gesture start → `Record::begin_gesture`; drag frames emit
  coalesced commands; release pushes single undo entry (pattern in `cmd.rs`).
- Clipboard: Ctrl+C/X/V, paste at **leftmost visible tick** (FL rule);
  Shift+drag = duplicate (copy on drop); Ctrl+B = duplicate-right with optional
  timeline-range interval; Ctrl+D deselect; Shift+I invert; Del deletes.

### 7c. Rendering in egui for thousands of notes

- Keep painter-direct approach (`roll.rs:311-375`); add **tick-range cull**
  alongside pitch-range cull; single `rect_filled` per note color bucket is not
  directly possible with per-note alpha — instead: two passes (muted/ghost
  outlines, then faces), skip text < 34px (existing), skip shadow/rounded when
  note count > threshold or zoomed out (FL has Shadow/Rounded as toggles —
  expose as View options defaulting off at >2000 notes).
- Grid lines: draw only visible lines (we draw 17 fixed, `roll.rs:279-283` —
  switch to visible-range loop for multi-bar); ghost pass first (existing).
- Playlist content preview (`playlist.rs:181-195`): keep dots/dashes; add
  culling by clip visibility; tile mode at small heights (FL performance tiles).
- Mini preview strip on scrollbar (FL has it): P3, pre-render pattern
  occupancy to a small texture on edit, blit in scrollbar area.

### 7d. File-by-file implementation plan

1. `src/core/model.rs` — extend `Note`/`Slide`/`Clip`, validation, defaults,
   serde defaults for old v2 files.
2. `src/core/automation.rs` (new, P2 stub) — `ClipKind`, auto-point types.
3. `src/core/cmd.rs` — new commands + tests (one-gesture-one-undo per op).
4. `src/sequence.rs` — keep as v1 engine facade; add tick→step bridge only
   until engine moves to core (roadmap §1: engine on core model). No editor
   logic here.
5. `src/ui/edit.rs` (new) — `Tool`, `SnapMode`, `Gesture`, `Editor`, tick↔x
   math, Alt-bypass, Line-from-zoom, Events-nearest; unit-tested without egui.
6. `src/ui/roll.rs` — rewrite input on `Editor`: all tools, both-edge resize,
   selection rect, lasso (P2: magic lasso), stretch handle, scrub, note
   properties dialog, target-control lane (vel/pan/rel/pitch/modx/mody),
   ghosts incl. open ghosts, scale highlight, keyboard styles (P3), context
   tool menu (MMB+R-click), arrow menu bar (File/Edit/Tools/Stamp/View/Snap/
   Select/Group/Zoom/Markers/Target channel/Target control/Auto smooth/Preview/
   Center/Detached as egui menu — start with working subset per P1 list).
7. `src/ui/playlist.rs` — rewrite on `Editor`: tools incl. slip, select/slice/
   resize-both-edges, clip focus, clip menu (preview/swap/make-unique/chop/
   consolidate), track headers (name/color/mute/solo/lock/size/group), picker
   panel, arrangements (P2), markers (P2), performance mode (P4).
8. `src/ui/mod.rs` — per-window `Editor` state, global snap in deck, shortcut
   dispatcher (context-dependent keys per `basics_shortcuts.htm`), detached
   windows already via egui_dock (keep).
9. `src/ui/deck.rs` — global snap widget, last-tweaked display (P2).
10. Tests: `edit.rs` math tests; `cmd.rs` undo tests; `roll.rs`/`playlist.rs`
    probe tests (existing `probe` pattern in `ui/mod.rs:104-118`); render
    equivalence after model switch.

---

## 8. Prioritised list with acceptance tests

### P1 — kill the rigidity (now)

1. **Tick model live**: core format-2 `Tick`/480 PPQ drives roll+playlist;
   v1 migration bit-exact (`migrate.rs` tests). *Accept*: 1/6-step triplet grid
   placeable; project with off-step note round-trips JSON unchanged; old `.kfxl`
   opens with identical render.
2. **Per-window snap**: Main/Line/Cell/(none)/step/beat fractions/Bar in both
   windows; Alt bypass everywhere; Line follows zoom. *Accept*: scripted drag
   test snaps to 1/6 step (80 ticks); Alt-drag lands on arbitrary tick;
   zoom 2× halves Line grid.
3. **Draw with last-length memory + both-edge resize + R-click delete +
   Shift-duplicate + Ctrl-select**. *Accept*: click empty adds len = last touched;
   grab either edge resizes (left needs opt-in flag on); Shift+drag copies;
   each = 1 undo step.
4. **Selection + clipboard + duplicate + delete** (rect select, Shift add,
   Ctrl+A/D/I, Ctrl+C/X/V paste at leftmost visible, Ctrl+B, Del). *Accept*:
   select 5 notes, Ctrl+B → 5 new ids right of original, 1 undo.
5. **Nudge keyboard**: Shift+arrows (snap), Ctrl+arrows (octave), Alt+arrows
   (fine); transpose dialog via menu. *Accept*: each nudge = 1 undo; pitch
   clamps 0..127.
6. **Per-note props lane**: velocity + pan + release + pitch + modX + modY
   targets, lollipops, Shift-set-same, Alt-reset, double-click properties
   dialog (Time/Duration bar:step:tick). *Accept*: set pan on 3 stacked notes
   independently; values survive save/load.
7. **Mute tool + per-note mute + slide/porta flags + 16 colors (= MIDI ch)**.
   *Accept*: muted notes silent in render; same-pitch different-color notes
   slide independently.
8. **Playlist basics parity**: clips place/move at snap (not bar-only), resize
   both edges, slip tool, slice, make-unique, mute clip, named/colored tracks
   with mute LED. *Accept*: place clip at beat 2.3 of bar 1; slip content ±tick
   with bounds held; slice → two clips sharing source, windowed by offset/len.
9. **Quantize (Ctrl+Q quick + Alt+Q dialog w/ strength/swing?)** — dialog
   details UNVERIFIED; ship quick + start-times + dialog with grid+strength.
   *Accept*: recorded off-grid notes quantize to 1/16 at 100%.

### P2 — FL feel (next)

10. Paint (+drum mode), slice tool with Shift/Alt variants, glue (Ctrl+G +
    click-hold), zoom tool + quick zooms (Shift+1..5), play/scrub (Y),
    interpolate, stretch handle + R-Shift edge stretch.
11. Ghost channels incl. open (cross-pattern) ghosts, double-R-click switch,
    editable ghosts; ghost toggles per channel/track.
12. Scale system: snap-to-scale + highlighting + key labels + keyboard styles;
    stamp tool chords/scales/drums.
13. Tools batch 1: chop, arpeggiate, strum, flam, legato/articulate, limit,
    flip, randomize, scale-levels, LFO lane writer, claw machine, riff machine
    (dialog params UNVERIFIED — design from app behavior, note deviations).
14. Playlist: picker panel, clip menu (preview/swap/chop/consolidate), merge
    (Ctrl+G / similar), insert/delete time, arrangements, time markers +
    song loop, track groups/sizes/lock, automation-clip stub, consolidate to
    audio via existing render path.
15. Context tool menu (MMB+R-click) in both windows; full arrow menus wired
    (File MIDI/score I/O, Edit space ops, View toggles incl. shadow/rounded/
    mini-preview, Target channel/control, auto smooth, preview toggle,
    center, detached).
16. MIDI record + input quantize via Global snap; typing-keyboard piano.

### P3 — depth (later)

17. Scripts (Python `flpianoroll`-compatible subset — API surface UNVERIFIED
    beyond tutorial All-notes example; ship recordable-macro subset first).
18. Event automation clips + envelope editor + Turn-into-automation-clip;
    per-pattern time signatures; background waveform; custom scales (.fsc);
    score-sheet PDF export.
19. Audio clips (record/stretch/fades/zero-cross) — blocked on engine work
    (roadmap §4 audio); plugin hosting for per-channel instruments.

### P4 — skip

Performance mode + live jumping, Edison/NewTone integration, stem extraction,
score notation beyond PDF export, online/cloud features, Edison-grade sample
editing in roll. Rationale: live-performance and external-editor surface with
near-zero overlap with composer editing; revisit after mixer + plugins land.
