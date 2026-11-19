# Mixer research (FL Studio → KeyFrameXL)

Date: 2026-11-15. Scope: FL Studio Mixer at 100% + first effects to build.
Method: Image-Line online manual pages (URLs under each section), brief
`research_notes/brief-2026-11-15.md`, `docs/roadmap.md`, `docs/ui-rules.md`,
`src/engine.rs`, `src/sequence.rs`, `src/core/model.rs`,
`src/core/migrate.rs`, `src/audio.rs`, `Cargo.toml`.
Rule: every FL claim has a manual URL. Anything not found in the manual is
marked UNVERIFIED. Research only, no `src/` changes.

Canonical manual root: `https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/`

Main pages used:
- Mixer: `.../html/mixer.htm`
- Mixer menu: `.../html/mixer_mixermenu.htm`
- Effect slots: `.../html/mixer_plugin.htm`
- Track properties (EQ/PDC/IO): `.../html/mixer_trackprops.htm`
- Levels/mixing/clipping: `.../html/mixer_levelsandmixing.htm`
- Recording (disk/arm/pickup): `.../html/recording_audio.htm`
- Channel settings (FX selector): `.../html/chansettings_main.htm`
- Wrapper (threaded/smart-disable/latency): `.../html/plugins/wrapper.htm`
- Fruity Send: `.../html/plugins/Fruity%20Send.htm`
- Fruity Limiter: `.../html/plugins/Fruity%20Limiter.htm`
- Fruity Compressor: `.../html/plugins/Fruity%20Compressor.htm`
- Parametric EQ 2: `.../html/plugins/Fruity%20Parametric%20EQ%202.htm`
- Reeverb 2: `.../html/plugins/Fruity%20Reeverb%202.htm`
- Delay 3: `.../html/plugins/Fruity%20Delay%203.htm`
- Delay 2: `.../html/plugins/Fruity%20Delay%202.htm`
- Chorus: `.../html/plugins/Fruity%20Chorus.htm`
- Blood Overdrive: `.../html/plugins/Fruity%20Blood%20Overdrive.htm`
- Filter: `.../html/plugins/Fruity%20Filter.htm`
- Love Philter: `.../html/plugins/Fruity%20Love%20Philter.htm`
- Stereo Shaper: `.../html/plugins/Fruity%20Stereo%20Shaper.htm`
- Stereo Enhancer: `.../html/plugins/Fruity%20Stereo%20Enhancer.htm`

What exists today (code map):
- No mixer, no effects: `grep mixer|Mixer|FxSlot|Effect src/` returns nothing.
- Voice engine, block render, 128 voices, steal-oldest: `src/engine.rs:17-27,317-340,453-461`.
- Filter is per-voice TPT/SVF (Simper/Cytomic): `src/engine.rs:20-38,240-279`.
- Song v1: `Song { version:1, tempo, swing, instruments, patterns, clips }`,
  drum rows fixed 3×16, sampler preset per instrument:
  `src/sequence.rs:119-165,307-349`.
- Format 2 draft: `Project { format:2, ticks_per_beat, tempo, beats_per_bar,
  instruments, patterns, playlist, next_ids }`, no mixer section:
  `src/core/model.rs:15-48,93-148`. Migration v1→v2 maps 1 step→120 ticks:
  `src/core/migrate.rs:1-3,66-187`.
- Audio replaces whole `Song` via `ArcSwap`, engine lives in cpal callback,
  48 kHz stereo f32 only: `src/audio.rs:17-77`.
- Roadmap already plans: channel→mixer `Target mixer track` field in model,
  UI with mixer; params/automation system; then mixer; then CLAP→VST3:
  `docs/roadmap.md:31-48,94`.
- UI shell: deck/browser/egui_dock centre/status, `Tab::Playlist|Rack|
  Roll|Instrument`, kit widgets (`raised/sunken/panel/thumb/button/chips/
  knob/seg_text/led/caps`), tokens in `src/theme.rs`:
  `docs/ui-rules.md:146-202`.

---

## 1. Mixer structure

### 1.1 Track inventory

- All audio passes through the Mixer. New Channels route to Master by
  default. There are 500 Insert Tracks + 1 Current track + 1 Master track
  (`.../html/mixer.htm`).
- Inserts receive input from Channel Rack instruments and external audio
  inputs (`.../html/mixer.htm`).
- Master is the only track that must have an Output selected; it is the
  normal place for mastering FX and final level before render. Do not record
  external inputs on Master (mixes with everything)
  (`.../html/mixer.htm`).
- Current (C) is a utility track that receives the currently selected Mixer
  Track output. Intended host for Edison (5-minute audio logger) or Wave
  Candy visualisation. Warning in manual: FX/EQ on Current applies to
  whatever is selected, easy to get caught out (`.../html/mixer.htm`).
- User screenshot in brief shows Current + Master + Insert 1..16 and a "+"
  add-insert control; this matches manual "Add insert mixer track/s after the
  selected track" (`.../html/mixer.htm`).

Mapping: KeyFrameXL has 4 instruments, no tracks at all. Format 2
`Project` has no `mixer` field (`src/core/model.rs:15-25`). Add it (see §7).

### 1.2 Count, add/remove

- Add icons: top bar "+" and another at end of inserts. Right-click options:
  Add one / Add multiple... / Add from state (FX preset from menu)
  (`.../html/mixer.htm`).
- New tracks are auto-selected to highlight them (`.../html/mixer.htm`).
- Mixer Track right-click menu: Delete selected track(s); Insert one or N
  (in front of selected when invoked from track menu); Trim unused tracks
  (Unlinked only / Unlinked+non-empty) (`.../html/mixer_mixermenu.htm`).
- Reset selected track(s) to default deletes FX, resets name/colour; Reset
  selected track(s) routing deletes routing (`.../html/mixer_mixermenu.htm`).

For us: start with fixed N (e.g. 16 inserts + Master, hide Current until
Edison exists). Dynamic add/delete/trim is P2. Keep IDs stable so
automation and channel links survive reorder/delete (see §7).

### 1.3 Naming, colour, icons, reorder

- Track labels: middle-click or Right-click → Rename/colour, or F2. Good
  habit to name used tracks; required for multi-out plugins, PDC math,
  split-track render, auto assignment (`.../html/mixer.htm`).
- Mixer menu → Mixer Track → Rename, colour & icon (F2). For a selection,
  name/icon applies only to unnamed tracks. F2 in dialog picks random
  colour. Group menu: Separator before selected (not a dock), Create group
  with separators left/right + name/icon popup, Auto color group
  (`.../html/mixer_mixermenu.htm`).
- Reorder: select track, Alt/Opt+Left/Right, or mouse over track +
  Shift+Mouse-wheel. All linked internal/external sources move with the
  track (`.../html/mixer.htm`). Layout menu also has Move left
  (Shift+Left) / Move right (Shift+Right) (`.../html/mixer_mixermenu.htm`).

For us: `MixerTrack { name, colour, icon }` + `order: Vec<TrackId>`.
Reorder moves ID, does not renumber automation (store by ID, not index).

### 1.4 Send tracks

- Any Insert can serve as Send via Send switches. Typical use: run out of FX
  slots, or share one reverb/delay to save CPU (`.../html/mixer.htm`).
- Sends are post-fader by default. Pre-fader send = put Fruity Send in a slot
  (usually slot 1), it taps audio before fader (`.../html/mixer.htm`,
  `.../html/plugins/Fruity%20Send.htm`).
- 0%-send (knob at minimum, grey cable) is sidechain-only: no audible send,
  still available to sidechain inputs (`.../html/mixer.htm`).

For us P1: post-fader sends with level knob + sidechain-only at 0. Fruity
Send equivalent (pre-fader tap plugin) is P3.

### 1.5 Docking left/center/right

- Mixer has 3 docks (Left/Middle/Right) separated by Track Dividers. Tracks
  behave the same in any dock; Left/Right stay fixed, Middle scrolls.
  Dock via Right-click → Dock to Left/Middle/Right; drag divider splitters to
  open/close Left/Right (`.../html/mixer.htm`, `.../html/mixer_mixermenu.htm`).
- Track Dividers have 3-dot drag handle to set relative height of faders vs
  meters (`.../html/mixer.htm`).

For us: P1 single scroll row, no docks. Docks + dividers are P3 (egui_dock
already supports splits; mixer-internal docks are extra).

### 1.6 Views (Compact … Extra large)

- 8 view modes Compact→Extra large, interact with window size. Extra large
  shows FX in use per track: Left-click opens, Right-click empty slot adds,
  Right-click effect gives move/replace (`.../html/mixer.htm`).
- Compact plugin list option (Mixer menu → View) removes empty slots, shows
  logical order (`.../html/mixer.htm`, `.../html/mixer_mixermenu.htm`).
- Views menu also: Waveforms Alt+W, Names at top, Routing cables, Track
  inspector on left, Colourful mixer Off/Low/Med/High, Alternative
  highlighting, Lines between tracks, Always show external input levels,
  Detached (`.../html/mixer_mixermenu.htm`).
- Icons row: Mixer menu, Multi-touch control (off = gestures like 2-finger
  tap for multi-select work), Meter-wave view Alt+W, Extra properties
  toggle, Track inspector & FX panel toggle, Add track
  (`.../html/mixer.htm`).
- If mixer looks wrong, manual says choose Compact and stretch height; some
  controls hide when height too small (`.../html/mixer.htm`).

For us P1: one Compact-like strip + inspector panel (see §8). Waveform view,
names-at-top, colourful-mixer density are P3/P4.

### 1.7 Right-hand insert panel (selected track)

Manual calls it Mixer Track Properties, Ctrl+Enter, only one visible at a
time (`.../html/mixer_trackprops.htm`):

- Input selector (external audio INPUT, stereo pairs top + mono bottom;
  Right-click AutoMap from selected track; Alt+Left-click Reset mapping
  excl. Master Output) + Output selector per track. Master must have Output
  or no sound (`.../html/mixer.htm`).
- Recording pickup selector (icon before Input): EXT (recommended, direct
  from interface) / MIX (external+internal pre-FX) / FX (post slots) / EQ
  (post track EQ) / LVL (post level+pan) / POST (post mute). Monitor Off /
  When armed / On (`.../html/mixer_trackprops.htm`,
  `.../html/recording_audio.htm`).
- 10 FX slots (§1.8).
- Integrated 3-band parametric EQ: Band1 Low Shelf, Band2 Peaking,
  Band3 High Shelf fixed types. Left-drag = freq (x) + gain (y);
  Right-drag = bandwidth (x) + gain (y) (`.../html/mixer_trackprops.htm`).
- PDC panel: icon turns orange (auto latency) / blue (manual offset);
  hover shows latency in hint bar; manual set in ms/samples/beats or Set
  from another track; wheel = 10 ms, Ctrl+wheel = 1 ms,
  Ctrl+Alt+wheel = 1 sample (`.../html/mixer_trackprops.htm`).
- Audio INPUT delay compensation (record alignment): positive moves
  recording left/earlier, negative right/later; Adjust-position toggle
  includes plugin latency (`.../html/mixer_trackprops.htm`).
- Delay audio output selector (bypass PDC on sends/device outputs for
  monitoring): Selected/Deselected + Never/Always/When recording or not
  playing (`.../html/mixer_trackprops.htm`).

For us P1: input/output selectors can be stubs ("(none)"/"Out 1-2") until
audio-input work; pickup/monitor/IDC are P3 with recording. EQ (fixed 3-band)
+ 10 slots + PDC display are P1/P2 (see §5-§6).

### 1.8 Ten FX slots, slot menu, mix knob, LED

- Each track has exactly 10 slots, audio top→bottom 1→10. If >10 needed:
  load Patcher in one slot or send to another track
  (`.../html/mixer_plugin.htm`).
- Load: Left-click arrow → Select; Right-click arrow → Plugin Picker;
  Middle-click → More Plugins menu (`.../html/mixer.htm`).
- Open/close UI: Left-click name label (`.../html/mixer.htm`).
- Reorder: hover effect + mouse-wheel, or slot menu Move up/down (also `u`/`d`
  keys in menu) (`.../html/mixer.htm`, `.../html/mixer_plugin.htm`).
- Mute/Disable LED per slot: Left-click mute, Right-click/Ctrl+Left/Alt+Left
  solo, Shift+Click lock. Does not mute the through path, only that effect;
  automatable (`.../html/mixer.htm`).
- Effect mix level knob per slot: Full left 0%, full right 100%. Manual:
  normally leave at 100%, do wet/dry inside plugin
  (`.../html/mixer_plugin.htm`).
- Slot context menu (mostly Wrapper menu shortcut):
  Replace/Delete/Patcherize/Patcherize-all/Presets/Browse presets/Save
  preset as (draggable to copy effect+settings to another track)/Add to
  plugin database/Browse params/Last tweaked/Link all/Allow threaded
  processing/Smart disable/Move up-down/View editor/Rename-colour-icon/Help
  (`.../html/mixer_plugin.htm`).
- FX master switch per track: Left-click enable/disable all FX on track;
  Right-click open/close all UIs; dark grey when no FX. Responds to
  multi-track selection, so Ctrl+A + click toggles whole project
  (`.../html/mixer.htm`).
- Spare state: Store/Flip for A/B compare (`.../html/mixer_plugin.htm`).

For us: `FxSlot { kind, state, bypass:bool, mix:0..1 }`, mix defaults 1.0.
Patcherize, threaded toggle per plugin, spare-state A/B are P3/P4.

---

## 2. Strip controls

All below from `.../html/mixer.htm` unless noted. Level/pan/EQ section of
that page is the reference diagram.

### 2.1 Fader range and taper

- Level fader = track volume, applied post-effects. Pre-effect level = use
  Channel Rack volume knobs or route track→track
  (`.../html/mixer.htm`).
- Master fader rescales sum of routed tracks; monitor-volume knob does not
  affect render (`.../html/mixer_levelsandmixing.htm`).
- Exact dB range/taper of the GUI fader: UNVERIFIED (manual states output in
  dB and shows orange over 0 dB, but no min/max/step table was found on the
  mixer/levels pages).
- Usable facts: 0 dB reference = max before clip on fixed-point/file/DA;
  internal mixer is 32-bit float with ~1600 dB headroom, so insert overs are
  "practically impossible to clip"; only Master (or tracks routed to ASIO
  out, or renders <32-bit float) can clip
  (`.../html/mixer_levelsandmixing.htm`).
- dB-change aide (levels page): +12 dB ≈ 4×, +6 dB ≈ 2×, ±1 dB ≈ ±10%
  (≈JND), −6 dB ≈ ½, −12 dB ≈ ¼ (`.../html/mixer_levelsandmixing.htm`).
- Colours: Grey default, Green selected, Orange armed for recording
  (`.../html/mixer.htm`).
- Multi-fader: Ctrl+Shift + click/drag label/meter area selects group; moving
  one fader moves group (`.../html/mixer.htm`).
- Click above/below handle initiates smooth level change
  (`.../html/mixer.htm`).

For us: implement fader as linear-gain internally + dB display
(`gain = 10^(dB/20)`), default 0 dB, range −60..+12 dB + −inf mute,
taper UNVERIFIED — pick cubic/asymmetric taper and document. Smooth 5–20 ms
(see §7). Green=selected, orange=armed per ui-rules (no extra green "OK"
dots).

### 2.2 Pan

- Pan = position in stereo field; FL method mixes one side into the other, so
  100% pan is mono-sum of L+R. For independent L/R level pan use Stereo
  Shaper (`.../html/mixer.htm`).
- Default Circular Panning Law (`.../html/mixer.htm`; law details on song
  settings page — not fetched, treat curve as UNVERIFIED).
- Sampler/Clip note: circular law drops centre −3 dB vs hard-panned to keep
  apparent loudness (`.../html/mixer_levelsandmixing.htm`).

For us: default equal-power/circular pan, centre 0, L50..R50 or −100..+100%.
Exact FL curve UNVERIFIED — implement `cos/sin` constant-power and note
deviation risk.

### 2.3 Stereo separation

- Knob on strip + same process as Stereo Enhancer/Separation control.
  Middle = disabled. Right = decrease (sum L+R toward mono). Left = increase
  (inter-channel phase-inverted cross-mix). No effect on mono (works on L−R
  difference) (`.../html/mixer.htm`, `.../html/plugins/Fruity%20Stereo%20Enhancer.htm`).
- Max separation/merging ≈98% (`.../html/plugins/Fruity%20Stereo%20Enhancer.htm`).
- Tutorial: expansion = subtractive cross-mix (−6 dB cross ≈ widen;
  0 dB cross = centre-cancel L−R/R−L); reduction = additive cross-mix to
  identical L/R (`.../html/plugins/Fruity%20Stereo%20Shaper.htm`).

For us: `sep: -1..+1` (0 centre detent), implement as mid/side balance
`mid=1, side=1+sep*0.98`-style; right (mono) = L+R/2 to both. Mark exact FL
coefficients UNVERIFIED.

### 2.4 Swap L/R, polarity

- Swap channels icon: swaps L/R (fix mic cabling / match video pan)
  (`.../html/mixer.htm`).
- Invert phase icon: flips sign (positive↔negative) to avoid cancellation
  when multi-mic/kicks out of phase; thin sound = try invert or ms-delay
  (`.../html/mixer.htm`).
- Per-channel invert also in Enhancer (NONE/LEFT/RIGHT) and Shaper matrix
  (−0 dB = same level opposite phase) (`.../html/plugins/Fruity%20Stereo%20Enhancer.htm`,
  `.../html/plugins/Fruity%20Stereo%20Shaper.htm`).

For us: two bools `swap: bool, invert: bool` (or `phase_l, phase_r`).
Place after FX+EQ, before pan/sep/vol (FL order §5).

### 2.5 Mute/solo semantics

- Mute switch: Left = mute/unmute; Right or Ctrl+Left or Alt+Left = solo;
  Ctrl+Right = unmute all; Shift+Left = lock state; Alt+Left on mute = solo
  track AND all tracks routed to/from it (Alt Solo, menu also has S / Alt+S)
  (`.../html/mixer.htm`, `.../html/mixer_mixermenu.htm`).
- General setting "Restore previous state after solo" preserves prior
  mute pattern (`.../html/mixer.htm`).
- Solo-with-routing matters because sends mean muting one strip does not mute
  its send destinations; Alt Solo solves auditioning a submix
  (`.../html/mixer_mixermenu.htm`).
- Track mute is at POST pickup point (LVL→POST→MUTE chain, see §1.7).

For us: `mute, solo, locked` per track; solo = exclusive listen with
`restore_after_solo` option (default on). Alt Solo = solo closure over
routing graph (BFS both directions). Lock prevents Alt Solo/unmute-all from
touching track.

### 2.6 Peak meters

- Peak meters read in dB; orange over 0 dB (`.../html/mixer.htm`,
  `.../html/mixer_levelsandmixing.htm`).
- Inserts use 32-bit float: treat meter as relative guide, not clip alarm;
  Master/ASIO-out/<32-bit render are clip alarms
  (`.../html/mixer_levelsandmixing.htm`).
- Meter-wave view Alt+W replaces meters with waveform, width = peak
  (`.../html/mixer.htm`).
- Extra properties toggle + vertical size control which strip controls are
  visible (`.../html/mixer.htm`).
- Peak-hold time, clip-hold, ballistics (attack/release), numeric readout:
  UNVERIFIED (not specified on mixer/levels pages).

For us P1: stereo peak (per-block max, attack instant, release ~300 ms
UNVERIFIED choice), clip LED latching over 0 dB on Master only, click
clears. Peak-hold value + meter bridge are P2. Meters computed on audio
thread, sent to UI via atomics/ring at ~15–30 Hz (see §7).

### 2.7 Record arm

- Track recording arm switch: red when armed; Left arm, Right arm with
  file-name dialog (`.../html/mixer.htm`).
- Menu: Render armed to wav Alt+R, Render selected Shift+Alt+R, Auto-unarm on
  stop, Auto-create audio clips, 16/24/32-bit float (32-bit float
  recommended), Latency compensation toggle
  (`.../html/mixer_mixermenu.htm`).
- Full disk procedure: arm track(s) + Playlist Audio Track link + record
  filter + Play; files land in Recorded folder or custom Data folder
  (`.../html/recording_audio.htm`).

For us P1: arm flag stored, no-op render. Real disk recording + render-armed
are P3 (needs input routing + offline graph).

### 2.8 Track name/colour

- See §1.3. Rename/colour/icon F2; random colour F2 in dialog
  (`.../html/mixer_mixermenu.htm`).

Map to `docs/ui-rules.md`: colour = address (thin 2–3 px strip, not full
fill), text 12–15 px, mono for IDs/values.

---

## 3. Routing

### 3.1 Channel → insert

- Set from Channel Rack routing selector (drag, Alt+M show/hide) or Channel
  Settings → FX channel send. Display "---" = Master default. Double-click
  opens Mixer with track selected. TRACK label click auto-assigns first free
  track (`.../html/mixer.htm`, `.../html/chansettings_main.htm`).
- Multiple: select Channel Selector LEDs (Right-click multi), then Mixer
  Right-click → Channel routing → Route selected to this track Ctrl+L, or
  starting from this track Shift+Ctrl+L (e.g. 3 channels from track 4 → 4,5,6)
  (`.../html/mixer.htm`, `.../html/mixer_mixermenu.htm`).
- Drag-drop: Browser/Plugin Picker preset → Playlist header (creates
  Instrument Track linkage) or → Mixer track (auto-names target to
  instrument) (`.../html/mixer.htm`).
- Playlist tracks are NOT bound to mixer; audio path is Channel→Mixer
  direct. Instrument/Audio Track modes can lock Playlist→Mixer
  (`.../html/mixer.htm`).
- Wrapper Channel menu also has Route to free mixer track Ctrl+L, Save/Browse
  channel state (`.../html/plugins/wrapper.htm`).

For us: `Instrument.mixer_target: TrackId | None(=Master)` (see §7).
Ctrl+L / Shift+Ctrl+L are P1 keybindings. Drag-drop instrument→mixer is P2.

### 3.2 Insert → insert (sends)

- Select source, click Send switch on destination; level knob appears on
  destination, controls amount from source. Useful to share FX / work around
  10-slot limit (`.../html/mixer.htm`).
- Remove: click Deselect switch above knob, or turn level to 0 (keeps link,
  stops audio but keeps sidechain feed) (`.../html/mixer.htm`).
- Post-fader only via switches; pre-fader only via Fruity Send plugin
  (`.../html/mixer.htm`).
- Routing cables view toggles visualisation
  (`.../html/mixer_mixermenu.htm`).

For us: `Route { from, to, level: 0..1.25?, sidechain_only: bool }`.
Level taper UNVERIFIED — use same dB taper as fader, default 0 dB (1.0).
Display knob only when route exists.

### 3.3 Sidechain vs audio routing

- Sidechain = unheard send into a plugin input on destination (e.g. kick→
  compressor to duck bass; Vocodex dual modulator/carrier)
  (`.../html/mixer.htm`).
- Created by Right-click Deselect → Sidechain to this/only, or by setting
  send volume to 0 (grey cable). Any audio send IS also a sidechain feed;
  sidechain-only is audio at 0 (`.../html/mixer.htm`).
- Direction depends on plugin: Limiter receives FROM another track; Stereo
  Shaper sends difference TO a track. Available sidechain sources listed by
  track name on plugin's selector; wrong direction hides option
  (`.../html/mixer.htm`, `.../html/plugins/Fruity%20Limiter.htm`,
  `.../html/plugins/Fruity%20Stereo%20Shaper.htm`).
- Multi-input plugins: sidechain N tracks then Auto map inputs; outputs via
  Auto map outputs to following tracks; always name used tracks for PDC/render
  (`.../html/plugins/wrapper.htm`).

For us P1: sidechain graph edge only (audible path off). Actual multi-input
plugin hosting (Limiter sidechain input, Shaper difference out) is P2 with
effect instances exposing `num_sidechain_inputs`.

### 3.4 Route-to-this-track-only, submix, rules

- Menu: Route selected→this / →this only (deselects their Master sends);
  Sidechain selected→this / →only; Create submix (selected tracks rerouted
  through right-clicked track to Master, with name popup); This track→
  selected variants; Reset routing (`.../html/mixer_mixermenu.htm`).
- Select helpers: Channels routed here Alt+L, Channels+Playlist routed here,
  Tracks this one routes to / routed to this one Ctrl+Alt+L
  (`.../html/mixer_mixermenu.htm`).
- No feedback loops: source track shows greyed switch, unavailable, to
  prevent loops (`.../html/mixer.htm`). Fruity Send refuses loop-creating
  sends (`.../html/plugins/Fruity%20Send.htm`).
- External IO runs in parallel with internal routing: track can take both
  external input and internal sends, output to Master and device out
  simultaneously (`.../html/mixer.htm`).

For us: enforce DAG (DFS reject on add). "Only" = add routes + remove
from→Master. Submix macro = create group + reroute + name popup (P2).

### 3.5 Master and Current

- Master: final FX + level, feeds device outputs / render. Routed-to-ASIO
  inserts bypass it (`.../html/mixer.htm`).
- Current: mirrors selected track output for Edison/Wave Candy; not a mix
  bus (`.../html/mixer.htm`).

For us: Master = fixed id 0, always last in graph, no sends out. Current =
UI-only monitor tap (P3, skip until analyser/Edison exists).

### 3.6 "Disable all routing" equivalents

No single "Disable all routing" button found in manual — UNVERIFIED as named.
Closest: Reset selected track(s) routing + Reset mixer track routing, plus
FX master switch to bypass all FX (`.../html/mixer_mixermenu.htm`,
`.../html/mixer.htm`). Do not promise an FL menu item we could not verify.

---

## 4. How the mixer connects to the rest

### 4.1 Channel Rack target number

- Channel button → instrument UI; Channel Settings → FX selector shows mixer
  number; "---" = Master (`.../html/chansettings_main.htm`).
- Roadmap already requires `Target mixer track` field in model
  (`docs/roadmap.md:35`).

Change: `Instrument { mixer: TrackId }` where `0 = Master`, default `0`.
v1 files default all to Master (migration keeps render identical).

### 4.2 Plugin windows

- Effect UI opens from slot label click; Wrapper provides preset/spare/
  threaded/smart-disable/IO plumbing per instance
  (`.../html/mixer_plugin.htm`, `.../html/plugins/wrapper.htm`).
- Our Instrument window is `Tab::Instrument` (FL channel settings/plugin
  window). Mixer effect UI should reuse same pattern: slot label click opens
  effect editor tab/window, generic param UI when no custom editor
  (`docs/roadmap.md:32`, `src/ui/instrument.rs:19`).

### 4.3 Playlist audio clips

- Playlist not bound to mixer; Audio Clips route via their Channel/Audio
  Track association. Audio Track mode links Playlist lane→Mixer insert;
  recordings route back to originating insert
  (`.../html/mixer.htm`, `.../html/recording_audio.htm`).
- No audio clips in KeyFrameXL yet (`docs/roadmap.md:60-68`).

Change: when audio clips land (format 2), `AudioClip { mixer }` overrides
channel default, same `TrackId` space.

### 4.4 Automation of fader/pan/FX params

- Manual note on mixer pages: most controls automatable via Right-click →
  Create automation clip / Link to controller / Edit events
  (`.../html/mixer.htm`, `.../html/mixer_plugin.htm`).
- Roadmap §2 already designs `ParamAddr { owner: Channel|Mixer|Plugin|Transport,
  param: StableId }`, normalised 0..1 + units, Last tweaked, automation clips
  with tension, MIDI learn, rtrb + 5–20 ms smoothing, one gesture = one undo
  (`docs/roadmap.md:38-48`).

Change: mixer exposes `ParamAddr(Mixer(track, StableId))` for
vol/pan/sep/mute/solo/EQ/sends/slot-mix + each effect param (see §6 tables).
Right-click on any mixer knob = same menu as instrument params (P1 for
vol/pan, P2 for full FX list).

### 4.5 Presets for mixer states

- Mixer Track menu → File → Open/Save mixer track state (settings+plugin
  filters), Browse states (Browser presets); drag Save icon onto another
  track to duplicate FX chain (`.../html/mixer_mixermenu.htm`).
- Effect-level Save preset as... (FST) draggable to copy effect+settings
  (`.../html/mixer_plugin.htm`).
- Channel state save includes wrapper+sample+misc
  (`.../html/plugins/wrapper.htm`).

For us: `MixerTrackState` JSON (same as project sub-object) + `.kfxmix`
files later; drag-duplicate chain is P2. Keep preset = data only, no UI
positions.

### 4.6 Project file

- FL `.flp` includes mixer routing, FX states, Edison data (manual warns
  Edison bloats `.flp`, offers Remove-all-Edison)
  (`.../html/mixer_mixermenu.htm`).
- Our v1 `.kfxl` JSON has no mixer; format 2 JSON must include it
  (`src/sequence.rs:119-135`, `src/core/model.rs:15-25`).

Change: mixer inside `Project` (see §7), validated, migrated default =
single Master route (null-op).

---

## 5. Signal flow and processing order

### 5.1 Order inside an insert

Manual order (`.../html/mixer.htm`, `.../html/mixer_plugin.htm`,
`.../html/mixer_trackprops.htm`):

```
Channel instruments / external input (+ internal sends in)
  → FX slots 1..10 top→bottom (Fruity Send can tap out mid-stack)
  → integrated 3-band EQ
  → invert phase / swap channels
  → stereo separation / pan / volume fader
  → mute switch
  → sends (post-fader) to Master / inserts / sidechains + device outputs
```

Evidence: "audio passes through Effects Slots and then through integrated
effects (3 band parametric EQ, phase, stereo separation, volume and
panning). Finally sent to Master" (`.../html/mixer.htm`); "integrated
effects are the last stage prior to leaving, follow any plugin FX"
(`.../html/mixer_trackprops.htm`); fader is "post effect"
(`.../html/mixer.htm`); sends take audio "after fader"
(`.../html/mixer.htm`); pickup order EXT→MIX→FX→EQ→LVL→POST confirms
FX→EQ→Level/Pan→Mute (`.../html/mixer_trackprops.htm`).

For us: hard-code this order per track. Sends always post-mute (POST tap).
Pre-fader tap = P3 effect (Fruity-Send equivalent).

### 5.2 PDC (latency compensation) basics

- Needed when instruments/effects delay audio out of sync; PDC adds same
  delay to all other tracks to realign at Master. Covers instruments,
  effects, inter-track routing, multi-IO, sidechains
  (`.../html/mixer_trackprops.htm`).
- Automatic (default for new projects) updates as latency detected/changes;
  state saved per project. Manual offsets add on top (blue icon);
  per-plugin permanent offset in Wrapper→Info→Latency, remembered per
  plugin (`.../html/mixer_trackprops.htm`, `.../html/mixer_mixermenu.htm`,
  `.../html/plugins/wrapper.htm`).
- Units ms/samples/beats or Set-from another track; icons grey none /
  orange auto / blue manual; hover shows latency
  (`.../html/mixer_trackprops.htm`).
- Compensate automations aligns FX automation with delayed audio (disable
  for pre-20.7 projects) (`.../html/mixer_mixermenu.htm`).
- Realtime cost: output delayed to sync; disable Delay-audio-output for
  monitored inputs when recording (`.../html/mixer_trackprops.htm`).
- Guide: route same hit through delayed vs clean track, adjust to sample
  accuracy (`.../html/mixer_trackprops.htm`).

For us P1: each track/effect reports `latency_samples()` (0 for built-ins
except lookahead limiter / linear-phase EQ later). Engine computes
per-track max-delay and delay-lines others (see §7). UI shows orange/blue
dot + samples in inspector. Auto+PDC-on + manual offset from day one; data
model carries it even if engine initially only compensates integer blocks.

### 5.3 CPU behaviour (smart disable)

- Smart disable: stop processing plugin when inactive. Per-slot toggle,
  global F10 Audio switch, Tools→Macros→Misc→Switch for all. Active only in
  live playback, disabled during render. Can break time/envelope/long-tail
  plugins (cutouts, late start, out of sync) — then disable per plugin
  (`.../html/mixer_plugin.htm`, `.../html/plugins/wrapper.htm`).
- Allow threaded processing: on only if global + wrapper-menu + processing-tab
  all on; per-plugin save; turn off for misbehaving (GPO/Kontakt examples)
  (`.../html/mixer_plugin.htm`, `.../html/plugins/wrapper.htm`).
- Disabled bands in PEQ2 use no CPU (`.../html/plugins/Fruity%20Parametric%20EQ%202.htm`).
- Internal 32-bit float means no per-track saturation logic needed; keep
  meters in range for readability (`.../html/mixer_levelsandmixing.htm`).

For us: `bypass` + `smart_disable: bool` per slot; engine skips silent tracks
(RMS < −90 dB for N blocks → sleep, wake on input/param). Render path
ignores smart-disable (always process). Threaded FX = P3 (start single
audio thread; parallelise tracks only after measurement).

---

## 6. First effects (7 P1 candidates + 1 P2)

Principle: cover corrective mix (EQ/dynamics), space (delay/reverb),
character (drive), width (stereo), movement (chorus/filter). All must be
automatable, have generic UI fallback, and run allocation-free.

### 6.1 EQ — integrated 3-band + 7-band PEQ2 clone (P1, first)

Manual (`.../html/mixer_trackprops.htm`,
`.../html/plugins/Fruity%20Parametric%20EQ%202.htm`):

Integrated (fixed types):
- Band1 Low Shelf, Band2 Peaking, Band3 High Shelf. Graph drag:
  Left = freq+gain, Right = bandwidth+gain.

PEQ2 (7 bands, Standard IIR + Linear-Phase FFT modes):
- Per band Type: OFF/LowPass/BandPass/HighPass/Notch/LowShelf/Peaking/
  HighShelf. Slope: Steep 4/6/8 (−24/−36/−48 dB/Oct), 2 (−12), Gentle
  4/6/8 (same slopes, gentler knee: Gentle −6..−12 dB at cutoff vs Steep
  ≈−3 dB). Order menu 12–48 dB/Oct. Right-click token: Type/Order/Key
  (snap to note fundamental).
- Per band: Gain fader (disabled for LP/BP/HP/Notch), FREQ, BW, Mute
  (Left-click disable) / Solo (Right-click or Ctrl+Click), band-solo-on-shift
  (headphones, Adaptive vs Bandpass modes, Inherit bandwidth).
- Tokens: drag x=freq y=gain, Shift/wheel = bandwidth, Alt+Click reset,
  Ctrl+Click fine, click outside = main level. Global gain slider.
- Options: Lock spare, Minimum-phase mod speed (smoothing for HQ sweeps),
  High precision / Enhanced frequency display, Heatmap position, LIN
  (FFT linear-phase, adds latency — relies on mixer APDC), Compare A/B spare
  banks. Disabled/OFF bands cost no CPU.
- Credits: "Thanks to Robert Bristow-Johnson for his EQ Cookbook"
  (i.e. RBJ biquad cookbook).

DSP: RBJ cookbook biquads (LP/HP/BP/Notch/Peak/Shelf) per band in series;
coefficient smoothing on freq/Q/gain to avoid zipper. Standard mode = IIR
biquads. LIN mode = FFT FIR (P3, needs latency path). Reuse `Svf` in
`src/engine.rs:20-38` for integrated shelf/peak or implement RBJ for exact
match.

Rust: own `biquad.rs` (~150 lines, `rbj-eq` crate as reference) — do NOT add
`fundsp` just for EQ (heavy graph dep for 7 biquads). `fundsp::bell/
highshelf/lowshelf` exists but pulls DSL. `biquad`/`rbj-eq` crates are small
alternatives if we want a dep; recommendation: hand-roll (already have SVF
idiom + tests) and cite RBJ + Simper.

Params (normalised 0..1 → units):
`{ type: enum, freq: 10..22000 Hz log default per band
(50/150/500/1500/5000/10000/15000 UNVERIFIED defaults — pick musical spread),
Q/BW: 0.1..8 default 1.0, gain: −24..+24 dB default 0 }` ×7 + output
`−24..+24 dB`. Mark exact FL freq defaults UNVERIFIED (manual gives no
table).

### 6.2 Compressor / Limiter — Fruity Limiter COMP+LIMIT+Gate (P1, second)

Manual (`.../html/plugins/Fruity%20Limiter.htm`):

Signal flow: Input → COMP → GAIN → LIMIT(+Gate) → SAT → Out.
Use COMP or LIMIT alone by maxing the other's THRESH/CEIL (+LIMIT ATT 0).

COMP (RMS, sidechainable):
- GAIN post-comp makeup; THRESH (0 dB = off); RATIO; KNEE (soft→hard, shows
  envelope while held); ATT ms (also sets limiter lookahead — affects
  latency); CURVE 1..8 (attack/release tension); Sidechain selector (needs
  mixer sidechain tracks; Master sees all); REL ms; SUSTAIN 0..1000 ms RMS
  averaging. Ranges for THRESH/RATIO/ATT/REL: UNVERIFIED numerically except
  SUSTAIN 0..1000 ms stated.
- Workflow: lower THRESH below peaks, raise RATIO/KNEE.

LIMIT (peak):
- GAIN input; SAT threshold (left = more saturation/soft-clip); CEIL ceiling
  (+12 dB max ≈ off); ATT lookahead (hover slot shows delay; enable APDC or
  set 0 to avoid phase vs dry); CURVE 1..8; REL; SUSTAIN 0..1000 ms.
- Display: pause on click, input/output peaks, analysis+gain envelopes,
  level markers, speed; 0.2 dB safety option.
- Gate: GAIN (100% = open), THRESH, REL (gated-snare recipe in manual).
- Compare spare-state A/B.

DSP: peak/RMS detector (one-pole + SUSTAIN window) → gain computer
(threshold/ratio/knee) → attack/release envelope (CURVE = exponent/tension)
→ makeup → lookahead limiter (delay line = ATT ms, zero-attack peak clamp
to CEIL) → saturation (tanh / Laurent de Soras waveshaper — manual credits
"Laurent de Soras: Saturation algorithm"). Gate = downward expander.

Rust: hand-roll (~300 lines). No crate does Fruity behaviour exactly;
`fundsp::compressor` exists but different curves. Own code keeps lookahead +
sidechain under control.

Params P1: comp `{ thresh −60..0 dB, ratio 1..20, knee 0..1, att 0.1..100 ms,
rel 10..1000 ms, gain −24..+24 }`, limiter `{ ceil −24..+12, att 0..10 ms,
rel 10..1000 ms, sat 0..1 }`, gate `{ thresh −60..0, gain 0..1, rel }`.
Exact FL knob ranges UNVERIFIED — expose these musical ranges, document.

### 6.3 Delay — Delay 3 subset (P1)

Manual (`.../html/plugins/Fruity%20Delay%203.htm`; v2
`.../html/plugins/Fruity%20Delay%202.htm`):

- Input Wet (automate to dub only sections).
- Time 1..1000 ms (sync OFF) or 1/48 step..16 steps (1 bar) with Tempo Sync
  ON (Right-click Set menu; dots = steps; triplets/dots in v2 hint:
  1/2=8:00 etc.). Keep Pitch on/off (off = tape pitch-bend while changing
  Time), Smoothing (slew for tape feel), Offset/Pan (stereo offset % or
  ping-pong pan).
- Model: Mono (sum, Offset = L/R time mult) / Stereo (independent L/R) /
  PingPong (alternate L/R flip; mono needs Pan for effect) / Off (filter/
  sat/bitcrush only).
- Feedback: Level (can exceed 100%, ring green→orange→red), Cutoff + Res
  (>75% red, can self-oscillate), Filter LP/HP/BP/Off in loop, SampRate
  (≤20 kHz audible), Bits (≤10 audible, vintage digital).
- Modulation: sine Rate 0..20 Hz → Time (wow/flutter/flange) and Cutoff.
- Diffusion Level/Spread (smear ≈ reverb).
- Feedback distortion (waveshaper): Type Limit(hard)/Sat, Knee, Symmetry,
  Level, display X/Y.
- Output: Wet, Tone (LP centre HP on wet), Dry (0 for sends).

DSP: tempo-synced delay line (interpolated read for Keep-Pitch-off sweeps +
modulation), one-pole/BP filter in loop, soft-clip, diffusion = 2–4 short
allpasses. Classic Dattorro/KEP Freeverb-style diffusion for smear.

Rust: hand-roll ring buffer (preallocated `Vec<f32>`, no `fundsp` needed);
resample with linear interp (P1) → cubic/Hermite P2. `dasp_interpolate` as
reference only.

Params P1 subset: `{ time_ms|steps, sync: bool, feedback 0..1.2,
cutoff 100..18000, filter: off/lp/hp, mod_rate 0..20, mod_amt, mix/dry/wet }`.
Full bitcrush/diffusion/distortion = P2 toggles.

### 6.4 Reverb — Reeverb 2 subset (P1)

Manual (`.../html/plugins/Fruity%20Reeverb%202.htm`):

- MID/SIDE process select (SIDE needs stereo input; keeps centre dry-ish for
  full mixes).
- H.CUT / L.CUT (de-mud drums), DELAY predelay (+TEMPO switch), SIZE room,
  DIFF density (low = sparse echoes, high = dense decay), MOD depth +
  SPEED rate (break metallic ringing), BASS multiplier + CROSS crossover,
  DEC decay to −60 dB (up to 20 s per intro), DAMP HF damping (max = OFF
  bypass), DRY / ER / WET (send use: DRY 0%, WET 100%), SEP stereo width.
- Advice: easy to muddy; light delay often better than reverb in busy mixes;
  rarely on Master.
- Alternatives listed: Reeverb 1, Convolver, LuxeVerb.

DSP: Schroeder/Moorer → FDN (4–8 modulated comb + 2 allpass, standard
Freeverb topology extended with predelay + damping lowpasses + MID/SIDE
matrix). No exact FL topology published — UNVERIFIED, use Freeverb/FDN.
Modulation = sine LFO on comb delays (chorus-y diffusion).

Rust: hand-roll Freeverb (~200 lines) for control + no dep; `fundsp::reverb`
exists but fixed character. Reference: J. Stautner/M. Puckette FDN,
Gardner nested allpass.

Params: `{ predelay 0..200 ms, size 0..1, decay 0.1..20 s, diff 0..1,
damp 0..1 (1=off), bass/cross, dry/er/wet, width }`. Exact ranges
UNVERIFIED except decay ≤20 s.

### 6.5 Chorus — Fruity Chorus (P1)

Manual (`.../html/plugins/Fruity%20Chorus.htm`):

- Delay 0..30 ms (low = deep), Depth (0 = static), Stereo 0..360° (0 = LFOs
  in sync, 180 = opposite, ~10° subtle widening), LFO1/2/3 Freq + Wave
  (incl. Multi-sine = sine + 3× speed), Cross Type HP/LP + Cutoff (process
  only highs/lows), Wet Only (for sends).
- Related: Vintage/Hyper Chorus.

DSP: 2–3 modulated short delays (5–30 ms, sine/multi-sine LFO, opposite
phase L/R by Stereo angle) mixed with dry; crossover = LP/HP split so only
chosen band choruses.

Rust: hand-roll (shares delay-line code with §6.3). `fundsp::chorus` exists
but fixed voices; own 3-voice matches FL UI.

### 6.6 Drive — Blood Overdrive + waveshaper (P1)

Manual (`.../html/plugins/Fruity%20Blood%20Overdrive.htm`):

- Pre Band (pre-LP amount) + Color (pre-LP cutoff) → Pre Amp (drive) + x100
  (×100 power, single instruments) → Post Filter (2-pole post LP) → Post
  Gain (compensate; max = unity) → digital clip. Flow stated in Notes.
- Tips: don't lower input to fix loudness (kills drive), use Post Gain;
  preamp can push +300%; even clean pass adds upward compression (valve-like).
- Alternative: Waveshaper plugin.

DSP: pre-emphasis BP/LP → tanh/soft-clip (drive) → post LP → makeup.
Add Tone + Mix for modern use (UNVERIFIED extension, mark as ours).

Rust: hand-roll 1-pole + `tanh_fast`; no dep needed. `fundsp` distortion
units exist but character differs.

Params: `{ pre_lp 0..1, color 100..10000 Hz, drive 0..1 (+x100 bool),
post_lp 500..18000 Hz, post_gain −24..0 dB, mix 0..1 }`.

### 6.7 Stereo — separation/gain/polarity (P1, cheapest)

Manual (`.../html/plugins/Fruity%20Stereo%20Enhancer.htm`,
`.../html/plugins/Fruity%20Stereo%20Shaper.htm`):

- Enhancer: Stereo Sep (mid off; right→mono sum, left→expand; ≈98% max;
  mono unaffected), Pre/Post (phase offset before/after other filters),
  Phase offset (delay afflicted ch up to 500 ms; 20–40 ms stereoizes mono),
  Invert NONE/LEFT/RIGHT, Pan (compensate phase-induced image shift), Vol.
- Shaper: 4-slider matrix R→L / L / R / L→R (+ = add, − = subtract to −0 dB
  = inverted same level; hint bar for precise dB), Effects Pre/Post, Delay
  ±500 ms (20–40 ms stereoize), Phase allpass 10 Hz..22 kHz, In/Out
  Difference sidechain send (Input−Output = removed part to sidechained
  track; selector counts send-to tracks, not mixer numbers).
- Presets cover MID=(L+R)/2, SIDE=(L−R)/2, splitters, LR→LL/RR/RL/MS→LR etc.
  Tutorial derives all from matrix + sidechain difference.

DSP: M/S encode/decode + per-channel gain/delay/allpass. All linear except
delay modulation.

Rust: hand-roll (~120 lines). No dep.

Params: `{ sep −1..+1, gain −24..+24, pan (circular), swap: bool,
invert_l/r: bool, delay_ms −20..+20 (P2), width (M/S balance P2) }`.
P1 = sep/gain/pan/swap/invert only (covers 90% + integrated strip knob).

### 6.8 Filter — simple SVF (P1 small, Love Philter is P3)

Manual (`.../html/plugins/Fruity%20Filter.htm`,
`.../html/plugins/Fruity%20Love%20Philter.htm`):

- Fruity Filter: Cutoff, Resonance, LP/BP/HP mix amounts, x2 oversampling,
  Center (DC remove). Automation-friendly.
- Love Philter: 8 banks parallel/series (NEXT cog, negative = phase invert),
  12 types AP/BP/HP/HP2/HS/LP/LP2/LP3/LS/NOT/PK/SVF, ×1/×2/×3 slopes
  (12/24/36 dB) + ALT variants, ENV/CUT/RES/DRIVE(L/B/H), WaveShaper
  (unipolar/bipolar, AMP, MIX), OUT NEXT/PAN/VOL, articulators PAN/VOL/CUT/
  RES/L/B/H/WS/WMIX × sources PAT1-10/LFO/MOD X-Y/IEF(ATT/REL/USE1),
  X-Y mod, oversample 0/2/4/8/16×, HQ envelopes, Center DC.

DSP: SVF (already in engine) with LP/BP/HP/Notch/Allpass + shelf/peak via
RBJ; Love Philter = P3 graph of 8 such units + step/LFO/IEF modulators.

Rust: reuse `Svf` (`src/engine.rs:20-38`); add HP/LP morph + drive. No dep.

Params P1: `{ type: lp/hp/bp/notch, cutoff 20..20000 log, res 0..1,
mix, keytrack (P2) }`. Full 8-bank modulator = P3.

---

## 7. Real-time architecture for Rust

### 7.1 Constraints from current code

- `cpal 0.15` callback owns `Engine`, pulls `Arc<ArcSwap<Song>>` snapshot
  each block, renders stereo interleave, clamps −1..1
  (`src/audio.rs:47-64`, `src/engine.rs:354-380`).
- `Cargo.toml` has no `rtrbisb/fundsp/dasp/biquad` yet; only
  `arc-swap/cpal/serde/egui` (`Cargo.toml:8-20`).
- Roadmap §2 already mandates `rtrb` + 5–20 ms smoothing + normalised params
  (`docs/roadmap.md:38-48`).

### 7.2 Mixer graph on audio thread

```
voices (Engine) → per-instrument track input bus
  → [10 × FxSlot process] → EQ → swap/invert → sep/pan/vol → mute
  → post-fader send matrix (DAG topo order) → Master chain → out
```

- One `f32` stereo bus per active track per block, preallocated
  (`Vec<[f32;2]>` pools, no alloc in callback).
- Process tracks in topological order (Kahn, cached on routing change; cycle
  rejected at edit time so audio never has to handle it).
- Sends sum into destination bus (gain = send level). Sidechain-only sends
  sum into sidechain bus consumed by dynamics inputs, not audible bus.
- Master sums all Master-routed tracks + explicit sends-to-Master, runs its
  own 10 slots + EQ + fader, writes callback buffer.

Per-block budget (48 kHz, 256 frames ≈5.3 ms): keep mixer <50% (≈2.5 ms);
7-band EQ ≈ 7 biquads ×2ch, comps/delays/reverbs are the only heavy nodes —
gate heavy tails with smart-disable.

### 7.3 Lock-free param updates

- Continuous: `AtomicU32` (f32 bits) / `AtomicU8` (enums/bools) per param,
  UI stores target, audio loads each block and smooths (one-pole 5–20 ms;
  freq/log params smooth in log domain; gain smooth as linear ramp to avoid
  zipper). `Relaxed` ordering (independent values).
- Discrete: `rtrb::RingBuffer<MixerCmd>` SPSC UI→audio (bounded 1024):
  `SetRoute/AddRoute/RemoveRoute/MoveTrack/AddTrack/LoadPreset/SetSlot…`.
  Audio drains with fixed cap per block (e.g. 32) at block start.
- Large swaps (IR, preset blob, rebuilt graph): build on UI/worker, send
  `Box<T>` via second `rtrb`, `mem::replace` on audio, old `Box` into garbage
  ring dropped on worker (standard triple-role pattern).
- Meters back: audio writes `AtomicU32` peaks + clip flags per track/slot;
  UI polls at 15–30 Hz, never locks audio. Same for PDC latency reports.
- No `Box/Vec/String/format!/log` in callback; preallocate buses, delay
  lines, envelopes at graph build.

Crates: add `rtrb` (SPSC rings) only for P1. Defer `fundsp/dasp/biquad` —
hand-rolled biquad/SVF/delay/freeverb are smaller, testable, and match FL
character better. Revisit if convolution (Convolver/LuxeVerb) lands.

### 7.4 Concrete Rust types (proposal, not yet in `src/`)

```rust
pub type TrackId = u32; // 0 = Master, stable across reorder
pub type SlotIx = u8;   // 0..10
pub type ParamStableId = u32;

#[derive(Clone, Copy, PartialEq)]
pub struct Db(pub f32); // display only; DSP uses gain

pub struct MixerTrack {
  pub id: TrackId,
  pub name: String, pub colour: u32, pub icon: u32,
  pub input: AudioInput, pub output: AudioOutput, // P1: None/Master stubs
  pub slots: [FxSlot; 10],
  pub fx_master: bool,
  pub eq: TrackEq,          // fixed 3-band
  pub swap_lr: bool, pub invert: bool,
  pub sep: f32,             // -1..+1
  pub pan: f32,             // -1..+1
  pub vol_db: f32,          // -60..+12, -inf = mute-ish
  pub mute: bool, pub solo: bool, pub locked: bool,
  pub armed: bool,          // P1 flag only
  pub pdc_manual_samples: i32,
  pub peak: TrackPeak,      // atomics, UI mirror
}

pub struct FxSlot {
  pub kind: EffectKind,      // None | Eq7 | CompLim | Delay | Reverb | Chorus | Drive | Stereo | Filter
  pub state: EffectState,   // Box'd params, swapped via ring
  pub bypass: bool,         // LED, automatable
  pub mix: f32,             // 0..1, default 1
  pub smart_disable: bool,
}

pub struct Route { pub from: TrackId, pub to: TrackId, pub level: f32, pub sidechain_only: bool }

pub trait Effect: Send {
  fn latency_samples(&self, sr: u32) -> u32;
  fn process(&mut self, io: &mut [f32], sc: Option<&[f32]>, ctx: &FxCtx);
  fn set_param(&mut self, id: ParamStableId, norm: f32);
}
pub struct FxCtx { pub sr: u32, pub tempo: f64, pub block: usize }
```

`Mixer { tracks: HashMap<TrackId, MixerTrack>, order: Vec<TrackId>,
routes: Vec<Route>, master: TrackId }` lives beside `Engine`, owns buses.

### 7.5 Format 2 schema changes (back-compatible)

`src/core/model.rs:15-25` gains (all `#[serde(default)]`):

```rust
pub struct MixerTrackState {
  pub id: u32, pub name: String, pub colour: u32, pub icon: u32,
  pub slots: [FxSlotState; 10], pub fx_master: bool,
  pub eq: Eq3State, pub swap_lr: bool, pub invert: bool,
  pub sep: f32, pub pan: f32, pub vol_db: f32,
  pub mute/solo/locked/armed: bool,
  pub pdc_manual_samples: i32,
}
pub struct FxSlotState { pub kind: String, pub bypass: bool, pub mix: f32, pub params: BTreeMap<String,f32> }
pub struct RouteState { pub from: u32, pub to: u32, pub level: f32, pub sidechain_only: bool }
// Project += pub mixer_tracks: Vec<MixerTrackState>, pub routes: Vec<RouteState>,
// Instrument += pub mixer: u32 (0=Master, default 0)
```

- `validate()` rejects: unknown track id in routes/instruments, route to
  self, cycle (DFS), slot mix out of 0..1, vol out of −96..+12, >10 slots.
- `migrate.rs`: v1 → all instruments `mixer:0`, one Master default track +
  empty inserts lazily created; render identical (mixer = unity gain,
  no FX).
- Automation (roadmap §2): `ParamAddr::Mixer(TrackId, StableId)` + slot
  params reuse same `StableId` registry; v1 has no automation so no migration.

---

## 8. UI plan (inside `docs/ui-rules.md`)

Mixer = new `Tab::Mixer` in existing `egui_dock` centre (alongside
Playlist/Rack/Roll/Instrument). F5/F6/F7/F8 canon: F8 or new shortcut opens
Mixer; closed→open at last dock, open-unfocused→focus, focused→close
(`docs/ui-rules.md:163-171`).

### 8.1 Layout

```
┌ Mixer tab ───────────────────────────────────────────────┐
│ toolbar: [view Compact|Wide] [inspector toggle] [+ Add]   │
│ strips scroll-x → │M│ │1│ │2│ … (Master pinned right?)     │
│ inspector (right 320px): selected track IO/slots/EQ/PDC   │
└──────────────────────────────────────────────────────────┘
```

- Strips: vertical, name top (mono uppercase 10–11 px, colour strip 2–3 px —
  colour = address per §2.5), LED row (mute M `wait`, solo S `enc_blue`,
  arm R `brand`), pan + sep mini-knobs (`kit::knob`), fader (custom vertical,
  34 px hit, dB mono readout), peak meter (twin bar, clip LED latches).
- Sizes per ui-rules: controls 34 px, grid rows 24–28, strip width ~96–120 px
  Compact (UNVERIFIED optimal — tune to 1280×820 start, min 900×620).
  Scroll-x when overflow; mouse-wheel over names scrolls (FL parity).
- Inspector 320 px (`docs/ui-rules.md:70`): Input/Output selectors, 10 slots
  (label + LED + mix knob on expand), EQ graph (drag x/y per §1.7), PDC/latency
  line, sends list (destination + level knob + sidechain toggle).
- Only `kit::` widgets + `src/theme.rs` tokens; no hard-coded colours;
  brand red only for arm/record; playing = white frame, not red
  (`docs/ui-rules.md:34-45,188-203`).

### 8.2 P1 vs later

- P1 (usable mix): strips with name/M/S/pan/sep/fader/meter, select
  (click) + multi-select Ctrl+Shift, reorder Alt+Left/Right, inspector with
  10 slots (load/bypass/mix/reorder by wheel), integrated 3-band EQ graph,
  sends (click switch + knob, sidechain-only at 0), Ctrl+L/Shift+Ctrl+L,
  F2 rename. No docks, no waveform view, no Current, no IO recording.
- P2: Extra-large per-track FX peek, routing cables view, submix macro,
  track states open/save/duplicate-drag, group separators + auto-colour,
  meter bridge/waveform Alt+W, peak-hold numbers.
- P3: Left/Right docks + divider heights, colourful-mixer density, Current
  tap + analyser, Fruity-Send pre-fader plugin, external IO + pickup +
  disk arm/render, PDC manual editor UI.
- P4 (skip): multi-touch fader mode, Edison integration, cloud/mastering
  services, surround icons beyond labels.

---

## Prioritised list with acceptance tests

### P1 — mixable song (do now)

1. **Model + migration**: `Project.mixer_tracks/routes`,
   `Instrument.mixer`. Default Master-only, v1→v2 bit-identical render.
   - Test: `migrate_v1` old file → validate ok, `routes.is_empty()`,
     all `mixer==0`; `render` before/after byte-equal (like
     `src/engine.rs:716-732` live-vs-export pattern).
2. **Graph + post-fader sends**: topo order, per-track bus, send levels,
   cycle reject, Master sum.
   - Test: sine → track1 → send 0 dB to track2 + Master-only off → Master
     energy = direct + sent (±0.5 dB); loop attempt returns `Err(Cycle)`.
3. **Strip**: vol (−60..+12 dB + −inf) / pan (circular) / sep / swap / invert /
   mute / solo / Alt-solo closure / lock / F2 rename / reorder preserves IDs.
   - Test: automated `cargo test` on gain/pan/solo-closure math + native
     screenshot: 4 strips + Master visible at 1280×820, 34 px targets,
     mono dB readouts.
4. **Integrated 3-band EQ** (LowShelf/Peak/HighShelf) with graph drag.
   - Test: pink-noise render, +12 dB high-shelf @8 kHz raises HF energy
     >6 dB, null when flat (±0.2 dB).
5. **PEQ7** (§6.1) as first slot effect + **Comp/Limiter** (§6.2) as second.
   - Test: PEQ notch −18 dB @1 kHz drops 1 kHz sine >15 dB, others <1 dB;
     limiter CEIL −6 dBFS on +6 dB input never exceeds −5.5 dBFS.
6. **Delay + Reverb sends** (§6.3–6.4) with DRY 0/WET 100 send recipe.
   - Test: impulse → delay 250 ms @120 BPM sync returns echo at 250±5 ms;
     reverb DEC 2 s tail −60 dB within 2.0±0.3 s.
7. **RT plumbing**: `rtrb` cmds + atomic params + 10 ms smoothing + meter
   atomics at 20 Hz + no-alloc audit (`cargo test` with allocation counter
   feature or manual review).
   - Test: 256-frame blocks, 64 params tweaked per block, zero xruns in
     offline harness; UI poll shows peaks move.
8. **Automation exposure**: vol/pan/slot-bypass/slot-mix + EQ gains as
   `ParamAddr(Mixer…)` with Right-click Create-automation stub.
   - Test: automation clip on fader renders gain ramp ±1 dB accuracy.

### P2 — complete music workflow (next)

9. Chorus + Drive + Stereo + simple Filter (§6.5–6.8); full Delay/Reverb
   params (bitcrush/diffusion/M-S).
10. Extra-large FX peek, cables view, submix macro, track states
    save/open/duplicate-drag, groups/separators, meter peak-hold + numbers.
11. Sidechain inputs actually consumed (comp ducking: kick −12 dB dips bass
    >6 dB), multi-select routing menus, drag instrument→mixer.
12. CLAP host hooks (`clack-host`) behind `Effect` trait; VST3 (`vst3-host`)
    after. Sandbox scan in helper process (roadmap §1).

### P3 — studio (later)

13. External IO + pickup EXT/MIX/FX/EQ/LVL/POST + monitor modes + arm/render
    armed/selected + Auto-create clips; Current tap; Fruity-Send pre-fader.
14. Manual PDC editor (ms/samples/beats, Set-from), per-plugin latency
    overrides, Compensate-automations toggle; Love Philter 8-bank; linear-phase
    EQ mode (adds latency path).
15. Docks, colourful-mixer, names-at-top, divider heights, waveform view.

### P4 — skip

- Multi-touch fader mode, Edison/Wave Candy ports, surround 5.1/7.1 mixes,
  cloud mastering/store, FL Steps/MIDI-out specifics, online news/licensing
  dialogs. Reason: out of Linux MVP scope, needs hardware/services we do not
  have.

---

## UNVERIFIED list (do not present as FL fact)

- Fader dB min/max/taper/step; pan-law curve coefficients; meter
  attack/release/hold times; send-level taper/max.
- Exact PEQ2 band default freqs/Q/gains; knob numeric ranges for Limiter
  THRESH/RATIO/ATT/REL, Reeverb SIZE/DIFF/MOD/SPEED/BASS/CROSS/DAMP/SEP,
  Delay feedback-filter Q, Chorus LFO Hz ranges, Blood Color Hz range,
  Filter cutoff/res ranges.
- Strip pixel widths, 500-track performance envelope, Current-track internal
  tap point sample accuracy.
- Anything about `.flp` binary layout (not needed; we use JSON format 2).
