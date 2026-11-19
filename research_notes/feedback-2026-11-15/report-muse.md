# KeyFrameXL — feedback research: B1–B9 (Muse, 2026-11-15)

Scope: docs/feedback-2026-11-15.md (B-backlog), docs/ui-rules.md, docs/ux-plan-2026-11-15.md,
research_notes/ux-audit/report-muse-followup.md, plus code in src/ (read-only, no edits).
FL references are the Image-Line online manual (verified hub page
`html/envsettings.htm`; section URLs follow its stable naming).

How to read: each topic = (1) FL behavior + manual links, (2) what we have (`file:function`),
(3) spec (data model / layout / gestures / edge cases), (4) effort S/M/L.
Rule for all UI below: existing dark BMS/TE style — `sunken` wells, `raised` controls,
`kit::knob`/`mini_knob`/`fader`, `caps` labels ≥12px (ui-rules §1–2), statusbar hints, no new popups.

---

## B1. Settings window (Plugins page that isn't a stub)

### (1) FL

| FL section | What it holds | Manual |
|---|---|---|
| Audio | device, sample rate, buffer, resampling, CPU/panel | `html/envsettings_audio.htm` |
| MIDI | in/out port list w/ enable per port, controller type, velocity curve, release | `html/envsettings_midi.htm` |
| General | language, undo levels, UI zoom, animations, autosave | `html/envsettings_general.htm` |
| File | data folders, backup/autosave locations, project folders | `html/envsettings_files.htm` |
| Project / Info | tempo/time-sig defaults, title/author/genre/comments | `html/envsettings_project.htm` |
| Plugins / Manage plugins | Plugin Manager window: verified list w/ **type (VST/VST3/AU/CLAP), vendor, category, path, 32/64-bit, status (ok/error), favorites star, enable/disable toggle, rescan one, verify, rescan all, add search path, bridged option** | `html/plugin_manager.htm`, `html/plugins/wrapper.htm` |

Plugin Manager specifics (what makes the page useful): table columns
Favorite ★ / Name / Vendor / Type+format / Category / Path / Status;
"Verify plugins" + "Rescan" (full) + per-plugin "Rescan"; failed plugins listed with error text
and excluded from menus until re-verified; search filter; favorites show first in plugin picker.

### (2) What we have

- `src/ui/settings_ui.rs:App::settings_window` — 4 pages (`Page::{Audio,Midi,Plugins,General}`), 480px window. Audio/MIDI/General are real; there is **no File/Project/Info page**.
- `src/ui/plugins_ui.rs:App::plugins_page` (~line 667) — stub: "safety" bridged checkbox, status line,
  scroll list of `name + INST/FX + status text` (vendor/path only in hover tooltip), `Rescan all`,
  `Add folder…`, extra-folders list. No per-plugin row actions, no filter, no enable/disable,
  no favorites, no rescan-one/verify, no format/category/path columns, no per-plugin bridged toggle.
- Backend already has the data: `src/plugins.rs:Scanned{path,name,vendor,category,instrument,gui,status,stamp,bridged}`,
  `scan()/load_cache()/bundles()/search_dirs()`, `Settings.plugins{extra_dirs,blocked,bridged}` in
  `src/settings.rs`. `blocked: Vec<String>` = display path of crashed plugins (≈ FL's failed list).

### (3) Spec

**Data model** (`src/settings.rs`, `src/plugins.rs` — additive, serde-defaulted):
- `PluginSettings`: add `favorites: Vec<String>` (display path), `enabled: Vec<String>` +
  `disabled: Vec<String>` (default on; mirror `MidiSettings::is_on` pattern), keep `blocked`.
- Optional later: per-plugin `bridge_mode: HashMap<String,bool>` (override of global `bridged`).
- Cache: extend `Scanned` consumers, not `Scanned` itself (all fields already scanned).

**Layout** (fits current Settings window; widen to ~640px, keep `kit::chips` page tabs):
1. Top row: search `TextEdit` (filter name/vendor/path) + `Rescan all` + `Verify selected` + `Add folder…`.
2. Table (`egui::Grid` or row rects, 26px rows): `★ | Name | INST/FX | Vendor | Category | Format(VST3·bridged?) | Status`.
   Status colors: `t::OK` ready, `t::BAD` failed/crashed/hangs, `t::WAIT` scanning. Path in tooltip + elided second line under name (mono 12).
3. Row click = select; right-click / `☰` or inline buttons per selected row:
   Enable/disable toggle (disabled plugins hidden from `plugin_choices()`), Favorite ★,
   Rescan this plugin, Reveal path (open folder), Clear from blocked.
4. "Extra folders" section stays below; failed/blocked section lists error text per plugin
   (from `Status::Failed(e)`), with `Forget` (re-probe next scan).
5. New **File page** (small, same window): data/preset folders, autosave minutes (move from General or mirror).
   Project/Info page: defer (needs per-project fields first) — show as disabled with "Arrives with project info".

**Behavior**: scan already async (`scan_rx` + `ScanMsg`); per-plugin rescan = `probe_in_child`-equivalent for one path
in thread, then replace entry + `save_cache`. Disabling never unloads a live instance mid-play:
applies to menus immediately, unloads on project reload or via `unload_plugin`. Filter is substring on
name+vendor+path, case-insensitive (same as `parameters_page` filter pattern in `plugins_ui.rs`).

**Edge cases**: yabridge `.vst3` bundles (bridged flag) probe slowly — per-plugin rescan has 45s timeout already;
Wave-shell-style multi-plugin bundles: show one row per bundle (we do today), note it. Missing bundle on disk:
row status "missing", kept in cache until rescan.

### (4) Effort: **M** (backend exists; work is table UI + rescan-one + enable/favorite state).

---

## B2. Plugin wrapper + channel settings

### (1) FL

- Wrapper: every plugin opens in FL's wrapper: title bar with **plugin picker/preset arrows, Save preset as (.fst),
  cog/wrench wrapper settings**, tabs **Plugin (editor) / Wrapper settings**. Wrapper settings:
  Processing tab (bridged mode, multithreaded processing, allow window resize, fixed size buffers,
  use block-size buffers, notify about rendering), **MIDI tab (input/output port, channel, pitch bend range,
  support helpers)**, VST wrapper options (program/params expose), Troubleshooting (latency/PDC).
  Manual: `html/plugins/wrapper.htm`.
- Channel settings (native/Sampler channels): gear icon → window with tabs **SMP (sample), MISC (time stretch,
  tracking), INS (filter/env: cutoff/res, envelope ADSR, LFO), FUNC (arpeggiator, echo/delay gate, time, swing,
  keyboard tracking range)**. Manual: `html/chansettings.htm`, `html/chansettings_smp.htm`,
  `html/chansettings_misc.htm`, `html/chansettings_ins.htm`, `html/chansettings_func.htm`.

### (2) What we have

- `src/ui/plugins_ui.rs:App::plugin_wrapper` — `Plugin | Parameters` chips, vendor line, fail status +
  `Reload`, `preset_stepper` (◀ ▶), `☰` menu (save/load presets, piano roll, delete, separate window).
  `embedded_editor` (X11 child window, auto-grow) + `show_editor` fallback; `parameters_page` (filter,
  `ParamRow{name,value,text,automatable}`, sliders + On/Off chips, `tidy_value`). `plugin_status`.
- Channel side: `src/ui/instrument.rs` (sampler/plugin host window), `src/ui/sampler.rs` (full sampler editor:
  zones, program, envelopes, LFO, filter, modulation matrix), `src/ui/rack.rs:channel_strip`
  (mute LED, Pan/Vol 24px knobs, mixer box, name). **No** wrapper processing/MIDI-port page, **no**
  per-channel MISC/INS/FUNC tabs. Per-note data that a channel-settings page would edit already exists
  (`sequence.rs:Note{pan,release,fine,mod_x,mod_y,color,group,porta}`) but lives only in the roll lane.

### (3) Spec — build order (do 1–3, defer 4)

1. **Wrapper "Settings" page (S)**. Third chip next to Plugin/Parameters: `Settings`.
   Rows: Bridged on/off **for this plugin** (override; default = global), Multithreaded: n/a (show disabled w/ reason),
   Fixed-size buffers on/off (passes MAX_BLOCK hint — needs `Vst3Proc` support later; store now, apply when supported),
   MIDI input port/channel selector (store in `PluginRef.ext` — see below), UI scale note. All values persist in project.
2. **Channel "Settings" strip inside instrument window (M)** — not new tabs yet, one scrollable section under the
   sampler/plugin area: `Volume / Pan / Insert` (already in rack, mirror here), `Glide (program.glide)`,
   `Polyphony / Play mode`, `Transpose`, `Velocity tracking`. These map to `sampler::Program` + `Instrument{volume,pan}`.
3. **MIDI I/O routing (M)**: `PluginRef` gains `midi_in_port: Option<u8>, midi_in_chan: u8, midi_out_port: Option<u8>`.
   Engine: `live_midi` already routes to plugin by id (`engine.rs:live_midi`); extend with port/channel filter.
   UI: dropdowns on wrapper Settings page; needs `midi_in` port list (already in `settings_ui.rs:midi_page`).
4. **Defer**: INS-style per-channel filter/env for plugin channels (plugins own their DSP — FL's wrapper
   doesn't add a filter either), FUNC arpeggiator/echo (build as MIDI note FX later, not channel tabs).

**Layout**: keep one instrument window; add third chip `Settings` only for plugin channels (sampler keeps its
existing editor sections). Follows `plugins_ui.rs:plugin_wrapper` pattern, no new window type.

### (4) Effort: **S** for (1) Settings page; **M** for (2)+(3). Full FL-parity tabs: L (not recommended — wrong model for hosted plugins).

---

## B3. Piano roll

### (1) FL (`html/pianoroll.htm`)

- **Scale highlighting**: piano-roll menu → Helpers → Scale highlighting; stamp tool chord/scale menu;
  ghost notes; View → Keyboard style; selectable root + scale shows in-scale rows highlighted.
- Full tool/option list: Select (E), Draw (P), **Paint (B)**, **Delete (D)**, **Mute (T)**, **Slice (C)**,
  **Slip**, **Quantize (Ctrl+Q / Alt+Q dialog: strength, start/end, groove)**, **Chop (Alt+U), Glue (Ctrl+G),
  Strum (Alt+S), Arpeggiate (Alt+P… actually Tools menu), Flip (Alt+Y), Randomize, Limit, Scale levels,
  Articulate, LFO to event**, **Stamp (chords/scales/riffs)**, **Time markers (Alt+T, numbered, loop/song jump)**,
  **Slide (S) / Portamento (O) per-note**, velocity/pan/release/fine/modX/modY event lanes, color groups (16),
  ghost channels (all), snap modes (Line/Cell/None + triplet), swing, legato/chop/glue, target channel/waveform view.

### (2) What we have (`src/ui/roll.rs`, `src/note_ops.rs`)

| FL feature | Status |
|---|---|
| Draw/Paint/Delete/Mute/Slice/Select/Zoom/Slide tools | ✅ (`Tool::ALL`, header icon buttons, `Gesture::{Edit,Paint,Erase,Mute,Slice,Select,Zoom,Slide}`) |
| Snap Line/Cell/None + Alt bypass; Ctrl+Q quick + Alt+Q dialog (strength/starts/ends) | ✅ (`Snap`, `snap_floor/round`, `note_ops::quantize`) |
| Velocity/Pan/Release/Fine/ModX/ModY lane | ✅ (`LaneTarget::ALL`, `event_lane`, per-note fields in `sequence.rs:Note`) |
| Color groups | partial: `Note.color: u8` 0–15 validated, but UI offers 6 (`NOTE_COLOR_NAMES`) |
| Ghost notes (toggle Alt+V) | ✅ (outline ghosts incl. slides, `ghosts` flag) |
| Slide + Porta | ✅ (`Glide`, `note_slide`, porta slash, `Slide` tool, `porta_new`) |
| Legato Ctrl+L / Chop Ctrl+U / Glue Ctrl+G | ✅ (`note_ops::{legato,chop,glue}`) |
| Right-drag erase, Paint drag, Ctrl+drag select, Shift+drag duplicate | ✅ (`roll_input`) |
| **Scale highlighting + key/scale chooser** | ❌ no scale concept anywhere |
| **Stamp (chords/scales)** | ❌ |
| **Strum / Arpeggiate / Randomize / Flip / Limit** | ❌ no `note_ops` fns |
| **Time markers** | ❌ no model |
| View modes / keyboard style / helpers menu | ❌ |

### (3) Spec — priority order

1. **Scale highlighting (S, do first)**. Pure view state (never data — cf. `sequence.rs` "Snap is a view setting").
   - State per roll view: `scale: Option<(root: u8 /*0-11*/, intervals: &[u8])>` + preset list
     (Major, Natural/Harmonic minor, Dorian, Pentatonic maj/min, Blues, Chromatic=off).
   - UI: `▾ → Helpers → Scale highlighting` submenu (mirror existing `Snap`/`Tool` menus in
     `roll_header`): root dropdown (C..B) + scale dropdown. Persist in workspace prefs, not project.
   - Paint: rows whose pitch-class ∈ scale get `ROW_WHITE`, others dimmed (`ROW_BLACK`-ish) — extend
     `paint_keys_and_grid`. Keys labeled with scale degrees. Ghost/out-of-scale notes get dotted outline.
   - Effort S.
2. **`note_ops` batch ops (S–M, pure fns + menu rows in existing Tools menu)**:
   `strum(offset_ms_per_note, direction)`, `arpeggiate(pattern: Up/Down/UpDown, rate: Tick)`,
   `randomize{velocity±, start±ticks, seed}`, `flip(horizontal: about pitch axis / vertical: time reverse around selection)`,
   `limit{velocity min/max, pitch min/max, length max}`. Same undo pattern as legato/chop/glue (checkpoint once).
3. **Stamp tool (M)**: new `Tool::Stamp` (key suggestion: `B`-adjacent free key or reuse Paint with modifier — FL uses its own tool; check `Tool::key()` collisions first). Stamp palette popup (existing `kit::popup_menu` pattern): chord buttons (Maj, Min, 7th, …) + selected scale chords; click places notes at cursor with `last_len`; drag extends length. Data: reuse `Note` with distinct `color` = stamp color.
4. **Time markers (M)**: `Pattern { markers: Vec<(Tick, String, u8 color)> }` (serde-defaulted, validated sorted+in-range).
   Ruler draws numbered flags; Alt+click jumps; menu Add/Rename/Delete; drag moves. Pattern-mode only initially.
5. Color groups 6→16 names (S): extend `NOTE_COLOR_NAMES` to FL's 16; `color: u8` already supports it.

### (4) Effort: scale **S**; ops batch **M**; stamp **M**; markers **M**; 16 colors **S**.

---

## B4. Automation

### (1) FL

- **Automation clips** (`html/automationclip.htm`, `html/automation.htm`): any automatable control →
  right-click → "Create automation clip"; clip holds envelope points (tension/curve types per segment:
  single curve, double, hold/stairs, smooth), LFO tool (Tools → LFO: sine/triangle/saw + depth/speed/phase),
  min/max, smoothing. Clips live in playlist, loop with pattern, can be linked to multiple targets (one clip →
  many knobs) and multiple clips → one target (last wins / merge). **Plugin parameters**: browse parameters →
  same right-click. **Tempo**: dedicated "Tempo" automation (playlist track). **Time signatures**: per-bar
  in playlist (`html/playlist.htm` — time signature markers per pattern/bar).

### (2) What we have

- Nothing functional. `src/ui/kit.rs` knob context menu has **disabled** entries:
  `Create automation clip` ("Arrives with automation"), `Link to controller…` ("Arrives with MIDI").
- No automation types in `src/sequence.rs`; engine reads only `Song.tempo: f64` (global, per-block constant
  via `plugin_transport` in `engine.rs`), fader/knob values directly from song state per block.

### (3) Spec

**Data model** (`sequence.rs` terms, all serde-defaulted + validated):

```rust
enum AutoTarget { MixerParam{track:u16, slot:Option<u8>, param:u32}, ChannelParam{inst:u64, param:u32},
                  PluginParam{plugin:u64, param:u32}, Tempo }
enum Curve { Hold, Linear, Smooth, SingleCurve(f32 tension), DoubleCurve(f32) }  // per segment
struct AutoPoint { tick: Tick, value: f32 /*0..1 normalized*/, curve_to_next: Curve }
struct AutoClip { id: u64, name: String, length: Tick, points: Vec<AutoPoint>, looped: bool }
struct AutoLink { clip: u64, target: AutoTarget, min: f32, max: f32 }  // value = min+(max-min)*env
// Song: auto_clips: Vec<AutoClip>, auto_links: Vec<AutoLink>,
//       tempo_map: Vec<(bar:u32,bpm:f64)> (default [(0,tempo)]), time_sigs: Vec<(bar:u32,num:u8,den:u8)>
```

- Normalized 0..1 storage; min/max + unit formatting reuse `fx::ParamSpec` / `format()`.
- Validation: points sorted, ticks within length, target ids exist, tempo 40–300.

**Engine read (per block)**:
- Precompute per-block: for each sounding `AutoLink`, sample its clip at the block's song position
  (playlist time → clip local time with loop; step through points — clips are small, linear scan fine;
  cache last segment index in `Engine` keyed by clip id for O(1) steady state).
- Application points: (a) mixer/fx params — copy `track.fx[k].params` per block into scratch and override
  linked params before `FxState::process` (never mutate `Song` on audio thread); (b) tempo — replace the
  `song.tempo` scalar used by `loop_frames/clip_frame/note_frame` with a **per-block tempo**: blocks are 512
  frames; tempo changes apply at block boundaries (FL-equivalent resolution, no pitch artifacts since tempo only
  maps ticks→frames); (c) plugin params — push `SetParam`-equivalent events into `Instance.events` at block start
  (needs a param-event path on `Processor`: new `PluginEvent::Param{at,param,value}` — cheap, no trait break
  for TestSynth/TestGain if defaulted… actually trait change; alternative: `set_parameter` via the shared `Mutex`
  off-callback — choose event variant to stay RT-safe).
- Time signatures: affect `clip_frame`/`BAR` math — `BAR` becomes per-bar length; playlist ruler + metronome read it.
  Highest-risk part: isolate behind `fn bar_len(song, bar) -> Tick` helper first (defaults to `BAR`).

**UI (in BMS/TE style)**:
- Knob/slider right-click → `Create automation clip` (enabled): creates 1-bar clip with two points at current value,
  selects it, opens playlist automation lane. `Link to controller…`: submenu listing existing clips + "New clip".
- Playlist: automation clips render as envelope rows under their track (same `Clip` windowing: start/offset/length);
  double-click adds point, drag moves, right-drag deletes, per-segment curve via right-click menu; LFO tool dialog
  (like `Alt+Q` quantize dialog pattern): shape/rate/depth → generates points. Keep in one new file `src/ui/automation.rs`.

**Stages**: 1) model + tempo map + engine read (no UI except list) 2) create-link + envelope editor 3) curves/LFO dialog 4) time-sig.

### (4) Effort: **L** overall; stage 1 **M**, stage 2 **M**, stage 3 **S**, stage 4 **M**.

---

## B5. Routing

### (1) FL (`html/mixer.htm`, `html/channelrack.htm`)

- Channel → mixer: channel settings / rack shows **target mixer track**; Ctrl+L routes selected, right-click menu
  "Assign to new mixer track / Route to this track". Mixer shows **which channels feed it** ("In" readout per track).
- Insert → insert: **sends** (sidechain-capable: "Sidechain to this track / Sidechain to this track only"),
  route-to-master toggle per track, **selected track highlights its routing: source channels + destination arrows
  stay lit** ("where this goes" is visible by selecting the track). Dragging between tracks creates routing.
- Sidechain: any track can send audio to another's sidechain input (compressor/limiter sidechain selector).

### (2) What we have

- Channel→insert: `sequence.rs:Instrument.mixer: u16` (0=Master); rack `channel_strip` mixer box (`rack.rs:set_channel_insert`,
  click popup + Ctrl+wheel step); `mixer.rs:route_selected_channels` (Ctrl+L), `free_insert`.
- Insert→insert: `MixerTrack{to_master: bool, sends: Vec<Send{to:u16, level:f32}>}` (`sequence.rs`);
  engine `mix()` routes post-fader to Master + sends in index order (`engine.rs:mix`). Sends constrained to **later
  inserts only** (no cycles by construction; validated in `Song::validate`).
- "Where this goes" today: inspector `From:` line (channel names feeding selected track, `mixer.rs:inspector`),
  `→ M` route button per strip, `From:` + mute/solo chain (`solo_keeps`). **No sidechain inputs** (sends are audio-sum
  into the target bus pre-FX; no separate key input; compressor/limiter have no sidechain selector).
- Fixed 17 tracks (Master + 16); no add/remove track (not found in UI; `default_mixer` only).

### (3) Spec

1. **"Where this goes" highlight (S, do first)**: selecting a strip highlights (a) channel rows in rack feeding it
   (outline on mixer box), (b) destination strips of its sends + Master arrow (accent border). Implement as derived
   state in `mixer.rs:strip` + a `mix_sel`-driven pass — no model change. Also reverse: selecting channel(s) in rack
   highlights their insert strip.
2. **Cable view (M)**: optional toggle in mixer header ("Routes"): draws bezier lines in the strips well background
   from each used send to its target + channel→insert summary counts. Keep subtle (1px `STRONG`, selected = `TEXT`).
   Read-only first; drag-to-create later.
3. **Dynamic inserts (M)**: `MixerTrack` list already variable-length (`validate` allows ≤128; sends use indices).
   Add Insert/Delete track (delete only if no channels/sends point at it, else re-point to Master with confirm).
   Renumber hazard: `Instrument.mixer` + `Send.to` are indices — deletion must remap (write `remap_inserts()` in
   `sequence.rs`, covered by unit test like `unique_detaches…`).
4. **Sidechain (L)**: model: `Send{sidechain: bool}` (serde default false; sidechain sends skip the target's audible
   bus and feed a second bus). Engine: per-track sidechain bus (reuse `scratch` double-buffer or add `side_buses`);
   compressor/limiter gain a `Sidechain: on/off (+ source = this track's side bus)` param; UI: send row toggle
   "Sidechain", FX param "SC". Validation: sidechain sends may target any later insert (still acyclic).

### (4) Effort: highlight **S**; cables **M**; dynamic inserts **M**; sidechain **L**.

---

## B7. Built-in effects

### (1) FL natives vs ours (`src/fx/mod.rs:FxKind::ALL`, `BUILTINS`)

| Ours (`src/fx/*.rs` params today) | FL native | Gap → what to add, in priority order |
|---|---|---|
| ParamEq 7-band (freq/gain/Q ×7) | Fruity Parametric EQ 2 (`html/plugins/Fruity Parametric EQ 2.htm`): spectrum display, linear-phase option, band solo/mute, HP/LP slopes, A/B | **1. spectrum analyzer + curve display (pre/post + per-band curves)**; band enable toggle; HP/LP slope select. Effort M (FFT in UI thread on tap buffer, not audio thread). |
| Limiter (Input/Ceiling/Release) + Compressor (Thr/Ratio/Att/Rel/Makeup) | Fruity Limiter (`html/plugins/Fruity Limiter.htm`): comp+lim+gate in one, sidechain, saturation, **gain-reduction meter + waveform display**, lookahead | **2. GR meter + I/O waveform in FX window** (engine exposes `FxState` meter values via atomics/tap; UI reads like `read_meters`); lookahead option; gate section. Effort M. |
| Reverb (Size/Damp/Width/Lowcut/Wet/Dry, Freeverb) | Fruity Reeverb 2 (`html/plugins/Fruity Reeverb 2.htm`): early/late sections, decay, stereo spread display, low/high damp, modulation | **3. decay-time param + early/late mix + damp display**; impulse-shape presets. Effort S–M. |
| Delay (Time steps/Feedback/Ping-pong/Tone/Wet/Dry, tempo-synced) | Fruity Delay 3 (`html/plugins/Fruity Delay 3.htm`): per-tap level/pan/filter/pitch, ping-pong modes, ducking, diffusion | **4. per-tap filter + level meter; feedback filter already via Tone**; stereo offset; ducking (needs sidechain — after B5.4). Effort M. |
| Compressor (above) | Fruity Compressor (`html/plugins/Fruity Compressor.htm`): sidechain, knee, attack/release curves, GR meter | covered by #2 + sidechain (B5.4). Add **knee** param (S). |
| Filter (LP/BP/HP + reso + mix) | Fruity Love Philter (`html/plugins/Fruity Love Philter.htm`): 8 filter units, envelope follower, LFO | **5. envelope follower + 2nd LFO-modulatable cutoff** (needs automation B4 or internal LFO — internal first, S). |
| Chorus (Delay/Depth/Rate/Mix) | Chorus / Flangus (`html/plugins/Flangus.htm`): stereo phase, multiple voices, delay display | **6. voice count + stereo phase display**. Effort S. |
| Drive (Drive/Tone/Output/Mix) | Fruity Blood Overdrive / Distortion: pre/post filter, x100 drive curve | **7. pre-filter + curve select (tube/fold)**. Effort S. |
| Stereo (Width/Balance/Haas) | Stereo Shaper / Enhancer (`html/plugins/Fruity Stereo Shaper.htm`): per-band delay/phase graph, mid/side, correlation meter | **8. correlation meter + mid/side balance**. Effort S–M. |
| Eq 3-band | = strip EQ already; keep as lightweight. | no change. |
| — (missing) | Gross Beat (`html/plugins/Gross Beat.htm`): time/volume/pitch envelopes, glitch slots | new effect only **after** automation clips (shares envelope UI). Note as candidate. |

Visual-feedback rule for all: every dynamics/spectral FX window gets (a) its curve (EQ bands, comp knee, filter response —
precomputed from params, no audio tap needed), (b) live meter only where the engine already exposes it
(peaks exist; GR needs new meter plumbing). Curve first (cheap, always correct), meters second.

Priority: **P1 spectrum+curve for ParamEq; P2 GR meter + knee + lookahead; P3 delay per-tap filter + ducking-ready;
P4 reverb decay/early-late; P5 filter env-follower; P6 chorus voices/phase; P7 drive curves; P8 stereo correlation.**

### (2) What we have

`src/fx/mod.rs` (`FxSlot{kind,params,enabled,mix}`, `ParamSpec{name,min,max,default,log,unit}`, `format()`),
10 `BUILTINS` + `HOSTED`; engine `FxState::process` per slot + slot mix + strip EQ/SEP/fader (`engine.rs:mix`);
UI knobs in `mixer.rs:fx_params/param_eq_bands`, FX windows in `fx_windows_ui`. No spectrum, no GR meter,
no curve displays anywhere.

### (3) Effort: P1 **M**, P2 **M**, P3 **M**, P4–P8 **S** each.

---

## B8. Mouse editing

### (1) FL canon (`html/channelrack.htm`, `html/pianoroll.htm`, `html/playlist.htm`)

- Piano roll: **left-drag paints/moves, right-click deletes, right-drag mass-deletes**, Ctrl+drag = select box,
  Shift+drag = duplicate, Alt = bypass snap, middle-drag/scroll pans, Ctrl+right-drag zooms (X), scroll wheels zoom/scroll.
- Playlist: brush (B) paints clips, right-click deletes, Ctrl+drag selects, Shift+drag duplicates, Alt bypasses snap,
  middle/scroll pans+zooms.
- Channel rack: left-click toggles step, right-click deletes step, **click-drag paints a row of steps**,
  Shift+click cycles velocity, Ctrl+click select.

### (2) What we have vs missing

| Gesture | Roll | Playlist | Rack |
|---|---|---|---|
| Left click draw/toggle | ✅ (`Gesture::Edit` place note) | ✅ Draw | ✅ `toggle_step` |
| **Click-drag painting** (held button paints across cells) | ✅ Paint tool (`Gesture::Paint`) | ✅ Paint copies w/o overlap | ❌ **pads use `Sense::click()` only (`rack.rs:rack`) — drag across pads does nothing** |
| Right-click delete | ✅ `secondary_pressed → Erase` | ✅ marker `secondary_clicked → Delete` | ✅ `secondary_clicked` on pad w/ vel>0 clears |
| **Right-drag mass delete** | ✅ (`Erase` held) | partial: Delete tool + Select+Del; right-drag path — check `playlist.rs` erase handling | ❌ no drag at all |
| Ctrl+drag select box | ✅ (`Gesture::Select`) | ✅ Select tool | ❌ (selector bar only click/Ctrl-toggle) |
| Shift+drag duplicate | ✅ notes (`duplicate`) | ✅ Shift+V+drag / `DragMode` | ❌ |
| Alt bypass snap | ✅ | ✅ Snap Off / Alt | n/a (steps are grid) |
| Wheel behaviors | ✅ pitch/time/zoom/row | ✅ shift=scroll, ctrl=zoom anchor | ✅ Ctrl+wheel reroutes (plain wheel scrolls) |
| Double-click open | n/a | ✅ roll | ✅ preview→roll |

### (3) Spec

1. **Rack drag-paint (S, biggest win)**: change pad `Sense::click()` → `click_and_drag()`; on `dragged()`,
   toggle steps the pointer enters (same value as the stroke start: if stroke started on empty → fill, on lit → clear —
   FL paints one value per stroke). One `checkpoint()` per stroke. Hit-test via existing `pad_x`/row math; track visited
   `(id, step)` set in stroke state (mirror `Gesture::Paint`). Edge: long patterns scroll while dragging — reuse
   playlist's autoscroll if present, else clamp to visible.
2. **Rack right-drag clear (S)**: same stroke machinery with clear-value; `secondary_down` held.
3. **Playlist right-drag delete (S)**: verify/extend Delete-tool + right-drag path to delete every clip entered
   (consistent with roll `Erase`); keep marker ▾ menu for Make unique/Delete single.
4. **Rack Shift+drag velocity / Ctrl+drag select rows (M)**: lower priority; rack selection already multi via selector bar.
5. No changes to roll (complete) besides keeping cursor hints (`set_cursor_icon` already does).

### (4) Effort: rack paint + right-drag **S**; playlist right-drag **S**; rack select/duplicate **M** (defer).

---

## B9. Native wavetable synth — original design, beats Serum 2 on usability

Constraint honored: original name/look/presets. Working title **"KeyFrame Tide"** (rename freely).
Not a Serum clone: single-window flow, drag-to-assign modulation, no Serum layout/menu/preset copying,
no Serum wavetable/factory content reuse (ship original tables + additive/spectral generation).

### (1) FL reference (what "native synth" means in FL)

Fruity Wrapper hosts natives like Harmor/Sytrus/Harmless (`html/plugins/...htm`): channel → synth,
piano-roll notes + slide/porta drive it, knobs automatable, presets in browser. Our synth is a **built-in
`InstrumentKind` variant** (like `Drum`/`Sampler`), not a VST — zero load latency, state in project, undo-native.

### (2) What we have to build on

- `src/plugin.rs:Processor` trait (`process(events, io, transport)`) — synth implements this; engine already
  mixes per-instrument voices into buses with channel vol/pan (`engine.rs:render_voices`, `start_note`).
- `src/sampler.rs` (Program: play modes Legato/Mono/Poly, glide, transpose, velocity tracking), `Svf` filter
  (`engine.rs`), `note_slide`/porta/glide pipeline, `FxState` per-slot DSP pattern, `preset.rs:Kind` preset system.
- Roll lanes (velocity/pan/modX/modY) = free mod sources.

### (3) Architecture (fits `Processor`)

```
TideVoice (per note): osc phase(s) -> warp -> mix -> filter(Svf x2, reuse engine Svf) -> amp env -> pan
TideEngine (Processor): params snapshot (UI thread writes versioned Params, RT reads lock-free) +
  wavetable bank (udur-side computed mips) + mod-matrix eval per block + FX micro-chain (dist/filter/delay)
```

- **Oscillators**: 2 wavetable oscs (256-frame tables, mip-mapped, spectral import: additive re-synthesis from
  formula, not from Serum tables) + 1 spectral/granular osc (looped single-cycle FFT morph + grain cloud from
  built-in noise/partials — no samples needed) + sub (sine/tri) + noise.
- **Warp modes** (per-osc, cheap math, no FFT on RT): Bend±, Sync (phase reset from osc A), PWM (table mirror),
  Quantize (bit-depth), Mirror, FM (osc B → A phase). One warp knob + amount per osc (usability > Serum's menu maze).
- **Filters**: reuse engine `Svf` (LP/HP/BP/Notch/Allpass already proven) ×2 serial/parallel + keytrack + drive.
- **Mod matrix drag-to-assign**: UI: drag from any modulator chip (LFO1/2, Env2/3, Vel, Key, ModWheel, Random, NoteLane)
  onto any knob → creates route (knob ring shows depth arc). Model: `Vec<Route{src, dst_param, amount, bipolar}>`
  (≤16 routes; same shape as `sampler::ZoneParams.matrix` — reuse `ModSource/ModTarget` pattern).
- **LFO/env shapes**: LFO: sine/tri/saw/square/S&H + smoothing + loop/one-shot; Env: ADDAHDSR with draggable curve
  per stage (UI draws curve — same painter code as future EQ curves, shared widget later).
- **FX rack**: mini-chain per preset (Drive → Chorus → Delay, reuse `fx::Effect` impls — no new DSP).
- **Macros + preset browser**: 4 macros (color-coded like TE addresses) → each maps to ≤8 params with depth;
  presets `.kfxtide` via `preset.rs:Kind` extension (new variant) + browser section + favorite stars.

**MVP stages (one engineer, shippable slices)**:
1. **S — 1 osc + warp + filter + amp env + macros**: audible, playable, preset save/load. (Proves Processor wiring + UI.)
2. **M — 2nd osc + sub/noise + 2 LFOs + mod matrix drag-drop + 4 macros.**
3. **M — spectral osc + FX micro-chain + preset browser section + 32 factory presets (original).**
4. **M — env curves editor + MPE/pitch-bend + polyphonic LFO phase + CPU polish (SIMD/lookup, voice cap like MAX_VOICES).**

RT rules (from `engine.rs`/`fx` precedent): no allocation in `process`, tables precomputed, param smoothing at block
rate, denormal guards, `validate()` bounds on all params (project files are untrusted input).

### (4) Effort: MVP stage 1 **M**; full design **L** (4 stages above).

---

## B10. Anything else missing vs FL (ranked, ≤15)

| # | Gap | FL reference | Notes |
|---|---|---|---|
| 1 | No undo for mixer/plugin/pattern-length ops in places (some paths `checkpoint`, some don't) | `html/basics_shortcuts.htm` (Ctrl+Z everywhere) | Audit: make every mutating path go through `checkpoint/publish`. M |
| 2 | No metronome/recording-audio path (REC armed, no takes comp) | Mixer disk recording, Edison | `record.rs` exists — finish takes→playlist. L |
| 3 | No PDC (delay compensation) | Mixer PDC | Plan exists in `docs/plugin-bridge-plan.md` → Status. L |
| 4 | Fixed 16 inserts, no add/delete/reorder | Mixer add/delete | Needs `remap_inserts` (see B5.3). M |
| 5 | No templating: default project, template picker on New | `html/envsettings_project.htm`, New from template | S |
| 6 | No song/pattern time-signature, no markers | Playlist markers/time-sig | Partly covered B3.4/B4. M |
| 7 | No Edison-style audio editor / playlist audio clips | Playlist audio clips, Edison | Playlist clips are pattern-only today. L |
| 8 | No MIDI out / hardware controller link (knobs show "Arrives with MIDI") | MIDI out, Dashboard | After B4 links. M |
| 9 | No render queue / per-pattern render, no MP3/OGG export options beyond WAV/stems | Export dialog | `export.rs` + `render::stems` exist; extend formats. S |
| 10 | No note humanize / groove templates (only swing %) | Piano roll groove quantize | `note_ops` extension, S |
| 11 | No channel grouping / color / icons in rack | Channel rack groups | `Instrument` needs `group/color` fields. S |
| 12 | No detached multi-touch / typing-keyboard velocity | Typing keyboard piano | exists basic (`typing_piano`); add velocity+octave shift. S |
| 13 | No A/B preset compare, no parameter randomize-all | Wrapper preset tools | Small: snapshot params, S |
| 14 | No project info/notes, no unsaved-changes depth (single dirty flag) | Project info | S |
| 15 | No accessibility pass on new widgets (focus order, screen-reader labels) | — | ui-rules §1.8; ongoing S per widget |

---

*Report written to `research_notes/feedback-2026-11-15/report-muse.md` (this file). No source files edited.*
