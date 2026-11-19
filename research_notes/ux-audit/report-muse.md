# KeyFrameXL — UX/UI research audit (research only, no code changed)

Date: 2026-11-15. Scope: `src/ui/*.rs`, `src/theme.rs`, `docs/ui-rules.md`, `docs/plan-2026-11-15.md`,
screenshots `research_notes/ux-audit/screens/01–25`.
FL Studio is the good example throughout. Every FL claim cites the official online manual
(`https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/...`)
or a Teenage Engineering guide. No source was edited.

## 0. Method + sources

1. Opened all 25 PNGs at 1366×768 (plus 24/25 at 1920) and mapped every visible
   element to the function that paints it (`deck.rs`, `menu.rs`, `browser.rs`,
   `files.rs`, `rack.rs`, `roll.rs`, `playlist.rs`, `mixer.rs`, `instrument.rs`,
   `sampler.rs`, `settings_ui.rs`, `plugins_ui.rs`, `export.rs`, `kit.rs`,
   `workspace.rs`, `theme.rs`, `mod.rs` shortcuts).
2. Compared each area against the FL Studio online manual and TE guides below.
3. Wrote concrete egui specs (exact px at 1366×768, scaling, which egui primitive).

### FL Studio manual (Image-Line, canonical URLs)

- Workflow / main windows (Rack F6, Roll F7, Mixer F9, Playlist F5, Browser Alt+F8,
  Ctrl+Shift+H arrange):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_workflow.htm
- Interface: window menus (tiny ▾ top-left of every window), hint bar, number displays:
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_interface.htm
- Channel Rack (rows = Mute/Pan/Vol/Mixer/Channel button/Selector/Steps-or-preview,
  Graph editor Ctrl+K, Alt+↑↓ move, Alt+C clone, Ctrl+L route, preview L-click/R-click):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/channelrack.htm
- Piano roll (target channel, tools P/B/D/T/C/S/E/Z, ghost Alt+V, snap, ruler seek):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm
- Piano roll menu (copy/paste/quantize/glue/chop, export MIDI):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm
- Playlist (clip tracks, double-click opens Roll / Alt+double-click Rack, track mode):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/playlist.htm
- Mixer functions (500 inserts + Master + Current, docks, routing, Ctrl+L / Shift+Ctrl+L):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/mixer.htm
- Effect slots 1–10 (load/reorder with wheel, mix knob, bypass, slot menu):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/mixer_plugin.htm
- Mixer menu (render armed/selected Alt+R / Shift+Alt+R, routing, layout):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/mixer_mixermenu.htm
- Mixer track properties (integrated EQ last in chain):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/mixer_trackprops.htm
- Browser (Current project / Recent / Plugin database / Plugin presets / Channel presets /
  Mixer presets / Packs / Recorded / Rendered / Sliced / Soundfonts / Templates,
  R-click File menu, Plugin Picker F8, Project picker Ctrl+F8):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/browser.htm
- Channel settings (Save channel state as .fst, Route to free mixer track Ctrl+L,
  Rename F2, Replace):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings.htm
- Plugin wrapper (plugin menu, preset arrows, Save preset as .fst draggable,
  Detached, scaling, typing priority):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/plugins/wrapper.htm
- State files (.fst = channel/plugin/mixer preset):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/fformats_other_fst.htm
- File menu (New/Open/Save/Save as/Ctrl+N save-new-version/Recent Alt+1–0/Exit):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_file.htm
- Add menu (Channel / Effect / Plugin picker / Browse presets / Refresh fast scan):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_add.htm
- Export dialog (Master render, Mode Song/Pattern, Tail, WAV 16/24/32f, FLAC 16/24,
  MIDI via Prepare-for-MIDI-export macro, Split mixer tracks = stems, Enable insert/master FX):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/fformats_save_export.htm
- Stems how-to (Ctrl+L then Split mixer tracks, WAV 24-bit):
  https://majormixing.com/how-to-export-multitrack-in-fl-studio
- Shortcuts master list (1–0 mute, Ctrl+1–0 solo, Alt+C/G/M/Z/U, Space/Ctrl+Space,
  L/R/Ctrl+M/Ctrl+T/Ctrl+H/F5–F12/Alt+F8/Ctrl+Shift+H, mixer S/F2/Alt+←→):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm
- General settings = Display scaling (Main GUI / popups / toolbars / cursor / legacy,
  125–200% at 4K–5K, restart to apply):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm
- MIDI settings (F10 page):
  http://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_midi.htm
- Audio recording (arm disk button, Playlist Audio track ↔ Mixer insert):
  https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/recording_audio.htm

### Teenage Engineering + skeuomorphism

- OP-1 field (anodized aluminum, 4 colour-coded endless encoders blue/ochre/gray/orange,
  push = confirm, Shift = fine, colour LCD, Help key, tape metaphor, undo added late):
  https://teenage.engineering/products/op-1 —
  guide https://teenage.engineering/_img/676030a8ef599a74cad84372_original.pdf
- OP-XY (black, grayscale + red = record/param locks only, LEDs on keys, 8 tracks ×
  64 steps, step components):
  https://teenage.engineering/products/op-xy
- TX-6 field mixer (6 stereo in, 3-band EQ, filter, compressor, 48×64 mono OLED,
  select knob + button, aluminum + leather):
  https://teenage.engineering/_img/6346db43ea5f4d74cad84372_original.pdf
- EP-133 K.O. II (12 pads + 4 groups, 999 slots, pressure-sensitive combinable
  Punch-In FX 2.0, fader, icon-map screen):
  https://teenage.engineering/guides/ep-133
- Field system (shared language across OP-1/OP-XY/TX-6/TP-7):
  https://teenage.engineering/products/field-system
- Inside design at TE (playfulness, history borrowing, testing):
  https://jdsimcoe.com/writing/inside-design-at-teenage-engineering
- Why skeuomorphism survives in music UI (hardware = quality cue, identity):
  https://uxdesign.cc/why-is-skeuomorphism-hard-to-die-in-music-production-interface-design-a634efa3e089
- What skeuomorphism is / why it matters in music:
  https://splice.com/blog/what-is-skeuomorphism
- Skeuomorphic vs flat plugin controls (knobs waste space, dials map poorly to mouse):
  https://matthewminer.com/2023/03/19/skeuomorphism-in-audio-software
- Knobs-in-GarageBand critique (circular controls are hard to drive with a mouse):
  https://theoutline.com/post/2157/why-are-there-so-many-knobs-in-garage-band
- How design affects what you hear (LANDR):
  https://blog.landr.com/skeuomorphism-plugins

---

## 1. Global layout & docking

### (a) Ours — `screens/01-main-arrange.png`, `24-main-1920.png`
`mod.rs:39-42` (`BROWSER_W 208`, `STATUS_H 28`, `PAGE 16`) + `menu.rs:9`
(`MENU_H 26`) + `deck.rs:9-10` (`ROW1_H 48`, `ROW2_H 34`) + `workspace.rs`
(dock style, `Cmd::Show/Toggle/Float/Dock/Preset/Reset`, floating sizes,
`Viewer::context_menu` Float/Dock only). Chrome total at 1366×768:
26 + 48 + 34 + 28 = **136 px = 18% of height** before any music.
Left column (`browser.rs`) mixes 4 jobs: Project/Files tabs, WORKSPACE
presets, WINDOWS list, PATTERNS, CHANNELS, Search. Centre is `egui_dock`:
tab bar 32 px, body `PANEL_BOTTOM` + 1 px `EDGE`, separators 6 px `BG`,
overlay `BRAND` 22%. Floating windows are **inside** the main viewport
(`egui_dock` surfaces), cascaded +24 px, Mixer 94%×62%, Instrument
86%×92%, others 62%×66% (`workspace.rs:261-283`).

### (b) FL — manual
“Main windows … movable (with overlap), resizable, zoomable and switchable …
open it using the Shortcut toolbar (or F-keys)” —
`basics_workflow.htm`. “Windows will snap together when dragged close …
hold Alt while dragging” to override — `basics_interface.htm`.
Reset via Ctrl+Shift+H — `basics_workflow.htm`.
Detached plugins are separate OS windows that can go to another monitor —
`plugins/wrapper.htm` (“Detached … moved to a monitor that does not have
the FL Desktop stretched across it”).

### (c) Problems
1. **Four redundant navigations**: deck icon row (`deck.rs:68-107`) +
   browser WINDOWS list (`browser.rs:140-186`) + VIEW menu (`menu.rs:148-191`) +
   dock tabs all do F5/F6/F7/F9. At 1366 px this costs ~208 + 5×34 px of chrome
   for one job.
2. **Floating ≠ detached.** `07-floating-roll.png` / `05-mixer.png` /
   `06-instrument-sampler.png`: the Mixer (94% wide) and Sampler (86%×92%)
   cover the rack/playlist they edit. Users cannot put the mixer on a second
   monitor (FL can). Inside-dock floating also breaks focus: which window do
   F5–F7 act on? `focused()` picks one, but two Rolls can be open with no
   visible focus ring except tab highlight (`workspace.rs:200-203`).
3. **Undiscoverable docking.** The only teacher is the deck hint bar
   (“Drag a tab to dock it anywhere…”, `mod.rs:1197-1207`), itself elided at
   1366 px (“…Alt+1..3 works…”). No drop-affordance until dragging; overlay
   appears only mid-drag.
4. **Empty-state asymmetry.** “All windows closed → message + Restore layout”
   (`ui-rules.md §4`) exists, but F12 + closing floats one by one is easy to do
   by accident (× on every tab, 16 px hit), recovery is a tiny `Reset` in the
   browser WORKSPACE section — far from the empty dock.
5. **Fixed columns waste 1920 px.** `24-main-1920.png`: rack trays clamp at
   `MAX_PAD_W 34` (`rack.rs:14`), leaving ~600 px dead right of BEAT 4.
   Browser stays 208, inspector 320, `NAME_W 230` — zoom (`Ctrl+=`) scales
   text but not these constants, so at 150% they overflow instead of reflowing.

### (d) Fix spec (egui)
- Keep `egui_dock`, but: (1) make the left column **resizable** (`egui::SidePanel`
  with `resizable(true)`, default 208, range 160–320, persisted); (2) delete the
  browser WINDOWS section — window switching lives in deck + VIEW + tabs only;
  (3) replace tab-drag hint with a first-run coachmark (one `Window` with
  “Drag tabs to split · right-click a tab for Float/Dock/Close”, dismissible);
  (4) add real OS-level detach later via `viewport` secondary windows; until then
  cap floating defaults to 720×480 and never cover the window they edit
  (offset to free quadrant); (5) tab context menu (`workspace.rs:395`) gains
  **Close** (egui `Button`, not only Float/Dock); (6) at ≥1600 px let trays grow
  past 34 px (change clamp to fill: `tray_w = available/beats`, pads stretch to
  max 44, not 34) so 1920 has no dead field.

---

## 2. Top transport / “deck” (`deck.rs`, `screens/01–03`)

### (a) Ours
Row 1 (48 px): wordmark 15 px semibold + 11 px caps over the browser column,
Play 48×40 red Primary, Stop 48×40, Record 40×40, Pattern|Song `chips`,
LCD 252×42 sunken with `seg_text` ghost segments (BAR:STEP + BPM + 4/4),
TEMPO knob (blue) + Tap. Row 2 (34 px): 5 icon-only 34×26 `tool_button`s with
1.5 px LED, Snap `dropdown` 110 px, pattern `−/dropdown/＋`, hint bar (elided),
CPU/VOICES caps right.

### (b) FL
Transport = Play/Stop/Record + Pattern/Song + LCD (`BAR:STEP:TICK`, PPQ) + tempo
+ tap; `Space` play/stop-with-rewind, `Ctrl+Space` pause-in-place, `R` record,
`L` PAT/SONG, `Ctrl+M` metronome — `basics_shortcuts.htm` + `ui-rules.md §3`.
Number displays: drag up/down, top/bottom-edge click ±1, hold + type a number —
`basics_interface.htm`. “Hold the mouse pointer over everything and look at the
Hint Bar” — `basics_interface.htm` (FL hint bar is in the toolbar, shows the
control under the cursor — **not** a tutorial sentence).

### (c) Problems
1. **Two tempos side by side.** LCD BPM (drag/wheel/double-click/type,
   `deck.rs:258-338`) + TEMPO knob (`deck.rs:341-371`) do the same thing 12 px
   apart. Screenshot 01 shows “120.0” twice. One must go (keep LCD, delete knob).
2. **LCD gestures are invisible.** No affordance that BPM drags, wheels ±1,
   double-click types, R-click = 120. Hover shows a `LINE` rect + vertical
   cursor only (`deck.rs:265`). New users will never find it; FL LCDs look
   editable (raised field + arrows).
3. **Icon-only window buttons fail.** `icon_playlist/rack/roll/mixer` 20-px
   1.5-stroke glyphs with no text (`deck.rs:80-95`). At 1366 even engineers
   cannot tell rack-grid from playlist-bars at a glance; tooltips help only on
   hover. FL toolbar has text + icons.
4. **Hint bar misused.** Ours shows a static tutorial per focus
   (`mod.rs:1198-1207`); FL hint bar shows **the control under the cursor**.
   Result: permanent truncated noise (“Click a step · Shift+click cycles…”)
   while actual tooltips duplicate it. Both should not exist.
5. **Wordmark wastes a column.** “KeyFrameXL / PATTERN SEQUENCER” sits over the
   208 px browser column in row 1 — ~200×48 px for branding that is already in
   the OS title (`Title * — KeyFrameXL`, `project.rs`). FL puts the logo small
   left and gives the space to transport.
6. **Record affordance weak.** Record is a 6.5 px circle in a 40×40 neutral
   button (`deck.rs:194-214`); armed = `BRAND`, taking = `BAD` + ring. No text
   “REC”, no time, no take list. FL record is a red circle with explicit
   armed-track wiring (see §8).

### (d) Fix spec
- Row 1 (keep 48 px): `[Play 40×32][Stop 40×32][Rec 40×32][Pattern|Song chips]
  [LCD 300×40: BAR:STEP · BPM field · 4/4][Tap 40×32][Metronome toggle]`.
  Delete TEMPO knob and wordmark (move wordmark 12 px into menu bar right side
  next to filename, or drop it — OS title already brands).
- LCD BPM becomes an `egui::DragValue` (range 40–300, speed 0.2, Shift 0.02)
  visually: sunken well + visible up/down stepper on hover + “BPM” caption.
  Keep wheel/double-click/type, add stepper buttons (FL parity: top/bottom edge
  click). All tempo edits = one undo (already).
- Row 2 (keep 34 px): text buttons `Playlist F5 / Rack F6 / Roll F7 / Mixer F9 /
  Browser` (26 px high, `tool_button` with label, LED kept), then Snap
  `egui::ComboBox`, pattern selector, **cursor hint** (FL behaviour: show
  hovered control’s help, else focused view’s one-liner — reuse `hint()` only
  as fallback, never truncated tutorial).
- All deck buttons: standard `egui::Button` with `min_size(vec2(40,32))`,
  text + shortcut in tooltip (`Space`, `L`, `R`, `Ctrl+M`). Delete custom
  `button_sized` icon-only variants here.

---

## 3. Main menu bar + every menu (`menu.rs`, `screens/08–11`, `22`)

### (a) Ours
26 px bar, 8 `menu_button`s (medium 12 +0.8 spacing). `item()`: fixed
`min_size(vec2(240,26))`, sans 13 + mono-12 shortcut, disabled rows stay with
“Arrives with …” hover (`menu.rs:12-22`). FILE has New Ctrl+Shift+N / Open Ctrl+O /
Open recent (10) / Recover / Save Ctrl+S / Save as Ctrl+Shift+S / Save new
version Ctrl+N / Export▸ (Audio Ctrl+R, MIDI Ctrl+Shift+M) / Import▸ (dwp, MIDI) /
Exit. EDIT: Undo/Redo + dead Cut/Copy/Paste. ADD: Channel▸ / Effect(F9) /
Plugin instrument▸ / Pattern. PATTERNS: Find next empty F4 / Rename F2 /
Clone / Delete / dead Split. VIEW: toggles + Workspace▸ + Close F12 +
Interface size▸. OPTIONS: 4 settings pages + Metronome Ctrl+M / Typing Ctrl+T /
Record-input / dead Count-in / Song mode L. TOOLS: Tap / Panic Ctrl+H / dead
Last-tweaked. HELP: Shortcuts F1 + version only.

### (b) FL
File menu: New / New from template / Open Ctrl+O / Save Ctrl+S / Save as
Shift+Ctrl+S / Export▸ Wave Ctrl+R, MP3 Shift+Ctrl+R, MIDI Shift+Ctrl+M /
Recent Alt+1–0 (10, +More to 50) / Revert to last backup / Exit —
`menu_file.htm`. Add menu: Channel / Effect / Plugin picker F8 / Browse presets /
Refresh fast scan — `menu_add.htm`. `Ctrl+N` = **save new version**
(`name_2`, never overwrite) — shortcut cheatsheets. F10 = settings, F1 = help
for the active window, F3 = tool menu — `basics_shortcuts.htm`.

### (c) Problems
1. **Dead items as design.** EDIT Cut/Copy/Paste are *always* disabled
   (`menu.rs:114-116`) even when the roll has `roll_sel` and Ctrl+C/X/V works
   in code (`roll.rs` clipboard). This teaches users the menu lies. Same for
   “Split by channel / Last tweaked / Count-in”. FL never shows permanently
   dead verbs; it hides or disables *contextually*.
2. **Odd labels.** “Effect (opens the mixer)” reads like a tooltip, not a verb.
   “Save new version” on Ctrl+N breaks 30 years of Ctrl+N = New (FL gets away
   with it because F-users learn it; newcomers will lose work — ours even
   swaps New to Ctrl+Shift+N, the *least* standard binding in the app).
3. **Nested `menu_button` fragility.** Export▸/Import▸/Channel▸/Workspace▸/
   Interface size▸ are menus inside menus; egui hover gaps close them (try
   moving diagonally from Export to “MIDI file…” in `08-menu-file.png` — the
   submenu is 240 px wide, gap-intolerant). FL uses native cascading menus.
4. **HELP is a stub.** One dialog (F1) + version string. No manual link, no
   “About”, no per-window F1 context (FL F1 opens the active window’s page).
5. **No mnemonics / no Esc discipline.** Alt+F does nothing (egui `menu_button`
   has no `Alt+letter` access); Esc closes dialogs inconsistently (settings yes,
   export yes-as-Cancel, rename yes, menus yes, floating dock windows no).

### (d) Fix spec
- EDIT: wire Cut/Copy/Paste to `roll_sel`/playlist selection when non-empty
  (enablement = `!roll_sel.is_empty() || clip.is_some()`), else hide the three
  rows entirely until selection exists (standard egui `add_enabled` is fine —
  the bug is the constant `false`). Delete other “Arrives …” rows until the
  engine lands; keep the shortcut *documented* in the F1 sheet so it does not move.
- Rename “Effect (opens the mixer)” → “Effect… F9”; move “Save new version” off
  Ctrl+N for now? **Keep FL parity** (Ctrl+N = new version) but add a first-run
  note + make New (Ctrl+Shift+N) show the unsaved-changes guard prominently
  (`project.rs:165` dialog is already correct: Save / Don’t save / Cancel,
  Enter = Save, Esc = Cancel — keep exactly).
- Replace nested `menu_button`s deeper than one level with flat items
  (“Export audio… Ctrl+R”, “Export MIDI… Ctrl+Shift+M”, “Import MIDI…”,
  “Import preset…”) — one level only, standard `egui::menu::menu_button`.
- HELP: “Manual (opens browser)”, “Keyboard shortcuts F1”, “About KeyFrameXL”.
  F1 opens the F1 sheet; Shift+F1 opens the manual URL for the focused window
  (map Kind → manual URL from §0).
- Add Alt+F/E/A/P/V/O/T/H access keys (egui: `button.shortcut_text` + manual
  `Alt+letter` handling in `shortcuts()`), Esc closes topmost menu/float first.

---

## 4. Right-click / context menus everywhere

### (a) Ours
- Channel button: `context_menu` 3 items (Instrument window / Piano roll /
  Delete) — `rack.rs:623`. Name-button hover: “click for window, right-click
  for more”.
- Step pads: **no menu** — `secondary_clicked` just clears (`rack.rs:272`).
  `13-rclick-step.png` proves it: R-click shows nothing (user expected a menu).
- Roll grid/notes: R-click/R-drag erases (FL parity), no menu. Lane caption
  R-click lists controls (`roll.rs:1538` — one of the few correct uses).
- Clips: body R-click **deletes** (`playlist.rs:590`); header 14 px ▾ marker
  opens a *custom* `popup_below_widget` (Add/Make unique/Mute/Delete/Song mode),
  not a `context_menu` (`playlist.rs:424-437`).
- Mixer strip lamp: Ctrl+click/R-click solo; slot: `context_menu` picker +
  click-empty also opens a *second* `popup_below_widget` with the same list
  (`mixer.rs:429-437`).
- Browser rows: `context_menu` (Load into selected/new, Remove root, Copy path)
  (`files.rs:461`); mixer-box in rack uses custom `popup_below_widget`
  (`rack.rs:576`); Snap/pattern/tool dropdowns all use custom `kit::dropdown`
  (`kit.rs:293-310`).
- Tabs: Float/Dock only (`workspace.rs:395`).

### (b) FL
- Browser File menu (R-click) is content-sensitive: Send to selected / Open in
  new channel / Open in new Sampler–Slicer–Granulizer / Edit in Edison–Pitcher–
  NewTime / Open parent / Windows shell — `browser.htm`.
- Channel button R-click → Piano roll + Replace + Clone + Delete + Route +
  Rename/Color; preview L-click show/hide, R-click open centred on notes —
  `channelrack.htm` §11–12.
- Effect slot: L-click name opens editor, R-click renames, wheel reorders,
  slot menu = Replace/Delete/Patcherize + Save preset as (.fst, draggable) —
  `mixer_plugin.htm`.
- Mixer track R-click: File (Open/Save state, Browse states), routing, group,
  layout — `mixer_mixermenu.htm`.
- Playlist track header R-click → Track mode > Instrument/Audio —
  `recording_audio.htm`; clip R-click = menu (never blind delete).

### (c) Problems
1. **R-click deletes is data loss.** Playlist body R-click deletes a clip;
   step R-click clears a step. FL reserves R-click-delete for *notes/tools*
   where the tool metaphor is explicit (paint/delete tools) — never for
   arrangement clips. No confirm, one undo (which users will not trust).
2. **Two popup systems.** `context_menu` (native, Esc/right-click-out,
   OS-feel) vs `popup_below_widget` + `toggle_popup` (custom, CloseOnClick,
   manual positioning) are mixed arbitrarily: clip header, mixer-box, snap,
   pattern, tool, knob typing all use the custom one. Result: different
   outside-click/Esc behaviour per control, popups that do not follow the
   invoker on scroll, and the mixer slot opening *two* identical pickers
   (R-click menu + click-empty popup).
3. **Channel menu is 20% of FL’s.** Missing: Rename (F2 exists but not here),
   Clone, Route to free (Ctrl+L exists but not here), Replace/Reload sample,
   Colour, Select-all/move hints. Users learn 3 verbs where FL teaches 12.
4. **Hit targets < 24 px.** Clip ▾ 14 px, slot mix 18 px, mixer-box 24×18,
   LEDs 16–18, selector bar 10×20 — all below the project’s own 24 px rule
   (`ui-rules.md §1.4`) and below FL’s chunky slot arrows.

### (d) Fix spec
- Clips/steps: R-click **always opens a `context_menu`**, never deletes.
  Clip menu (standard `context_menu`, min width 220): Open in piano roll
  (double-click) / Open in rack (Alt+double-click) / Make unique / Mute /
  Rename pattern? no — / Delete (Del, with the word Delete, not blind).
  Step menu: Velocity 100/70/127 / Delete / Select channel. Keep R-drag erase
  in roll (FL canon) but add `R-click empty = menu` there too.
- Delete every `popup_below_widget` that mimics a menu (clip header, mixer-box,
  snap, pattern, tool, channel) and use either `context_menu` (R-click) or
  `egui::ComboBox` (L-click choice). Keep `kit::dropdown` only where a
  non-menu popover is genuinely needed — nowhere today. **(Reinvented wheel.)**
- Channel `context_menu` → Rename F2 / Piano roll / Instrument window /
  Load preset (samplers) / Replace… / Clone Alt+C / Route to free Ctrl+L /
  Delete Alt+Del. Same verbs in browser channel rows.
- Tab `context_menu` → Float / Dock / Close. Mixer slot: single
  `context_menu` (Select/Replace/Delete/Move up/down/Save preset as .fst
  when presets land); click-empty opens the same menu (no second popup).

---

## 5. Browser — must become folders/presets/instruments only

### (a) Ours — `screens/14-browser-files.png`, `browser.rs` + `files.rs`
Left column, 208 px: Project/Files `chips` on top; Project tab = WORKSPACE
presets + WINDOWS + PATTERNS (×N uses) + CHANNELS (drm/smp/vst badges) +
bottom Search. Files tab = Add folder / Refresh / SORT Name▾ dropdown /
24 px rows with 9 px caps ext tags / click = preview (re-click/Esc stop) /
double-click/Enter = load / drag (`FileDrag`) to channel/rack/window /
R-click menu / arrows + ←/→ + Esc keys. Shown exts: wav flac ogg dwp kfxl mid
(`files.rs:16`). Roots default = audio_dir + home; `index` scans ≤20k dirs/100k
files in background. Search sits at the **bottom**.

### (b) FL — `browser.htm`
Tree: **Current project / Recent files / Plugin database (favorites + Installed) /
Plugin presets / Channel presets / Mixer presets / Packs / Recorded / Rendered /
Sliced audio / Soundfonts / Templates** + user “Browser extra search folders”
(green, ≤30, never a drive root). Preview on click; R-click File menu is
content-typed (Send to selected / Open in new channel–Sampler–Slicer–Granulizer /
Edit in Edison / Open parent / shell menu). Plugin Picker (F8) is the visual
array (generators left, effects right; type to filter; drag or double-click to
load); Project picker Ctrl+F8. Downloaded content lands under Packs/
`…/Documents/Image-Line/Downloads`.

### (c) Problems
1. **Wrong content, wrong place.** The user asked: browser = folders, presets,
   instruments. Today the browser column is the project outline (patterns,
   channels, windows, workspaces) *and* the file tree. Patterns/channels already
   live in rack/playlist/selector; duplicating them in 208 px forces tiny rows
   and a bottom search that nobody finds.
2. **No preset system to browse.** No Channel presets, Mixer presets, Plugin
   presets, no .fst anywhere (`instrument.rs` has Load but no Save; `mixer.rs`
   has neither). So “normal export of instruments” is impossible — the exact
   complaint. Files tab shows raw OS folders (`Музика`, `mallar2d` = home!),
   no Packs/Recorded/Rendered/Sliced/Templates categories, no favorites/star.
3. **Search at the bottom.** Every file browser since 1984 puts search on top;
   ours pins it bottom (`browser.rs:82-104` bottom-up layout) so the list scrolls
   above it. Progress (“Searching… N folders read”) appears mid-list, easy to miss.
4. **Preview is bare.** Click toggles, no volume, no position, no loop, no
   auto-preview on arrow keys (FL: arrows + Enter replace channel, Shift+↑↓
   steps through folder sending each to the channel). Our Enter loads, but
   Shift+↑↓ (FL audition-stepping) is missing.
5. **No Plugin Picker.** FL F8 is the fastest loader (visual grid, type-ahead,
   drag). Ours: F8 does nothing; ADD → Plugin instrument is a text submenu.

### (d) Fix spec (egui)
- Split the column: left stays **Project** (patterns + channels only, search on
  **top**, 28 px field); Files becomes a real browser tab with sections mirroring
  FL, backed by real folders from day one:
  `Project / Favorites ★ / Channel presets (.fst) / Mixer presets / Plugin presets /
  Packs / Recorded (~/Music/KeyFrameXL/Recordings) / Rendered / User folders…`
  Each section is a collapsible `egui::CollapsingHeader` (standard, not custom
  triangles); rows 28 px (≥24 rule + touch), search field on top with
  `TextEdit + hint “Search presets, samples, folders”`.
- Implement `.fst` first (channel state = instrument + sampler preset ref +
  pan/vol/mixer/color; mixer state = slots + EQ + sends): **Save channel as
  preset… / Save mixer track state as…** in the wrapper/slot menus (draggable
  menu item like FL), files land in the presets folders above, instantly
  browsable + draggable back. Without this the browser has nothing to hold.
- Preview row: click = preview, speaker icon + progress while playing,
  volume follows preview bus (already ×0.8 — expose it), arrows move, Enter =
  send to selected channel (FL parity), Shift+↑↓ = step through folder.
  R-click menu = FL set: Send to selected / Open in new channel / Open in new
  sampler / Show in system browser / Copy path / Add to favorites ★.
- Add Plugin Picker (`F8`): standard `egui::Window` grid (generators | effects),
  `TextEdit` filter on top, drag payload = same `FileDrag`, double-click loads
  to selected channel / selected mixer slot. Reuses the browser index.

---

## 6. Channel rack (`rack.rs`, `screens/01–04`, `12–13`, `24–25`)

### (a) Ours
Header: ▾ (Add channel / Unmute all / Clear / Clone-Move-Delete-Route / Graph
Ctrl+K / Pattern length 1/2/4/8) + `16 STEPS` + SWING knob (green, 0–60%).
Rows: LED 18 px (green = audible, blue = solo) · Pan 18 (white) · Vol 18
(orange) · mixer box 24×18 (`M`/num) · name button (`raised`, medium 13) ·
selector bar 10×20 right end · 4-step trays (`sunken` r8) · pads 22 px h,
22–34 w, r6, red when on + 2 px velocity bar + step number only if ≥28 px ·
LED row 18 px (3 px LEDs = playhead) · off-grid channels get a note preview
strip instead of pads · + Add channel · optional velocity lane 72 px (Ctrl+K).

### (b) FL — `channelrack.htm`
Row = Mute, Pan, Volume, Mixer-track selector, Channel button (opens instrument),
Channel Selector (outer border) / Activity (inner), Steps or Piano-roll preview.
Graph editor (Ctrl+K): per-step Note/Velocity/Release/Fine/ModX/ModY/Shiftscale…;
ramp via R-drag, reset Alt+click. Preview: L-click show/hide roll, R-click open
centred on notes; bar blocks shade. Move via selector + Alt+↑↓. Select-all =
double-click selector. Mixer selectors Alt+M. “Mute removed steps” option.

### (c) Problems
1. **Pads teach nothing.** No step numbers at default width (22–34 px rarely
   reaches the 28 px legend threshold — screenshots show blank red keys).
   Velocity = 2 px bottom bar, invisible at a glance. Shift+click cycles
   127/70/100 — hidden (hint bar only, elided). R-click clears with no menu.
2. **The strip is 5 micro-controls in 230 px.** LED + 2×18 px knobs + 24×18 box +
   name: three hit targets below 24 px, two with hover-only values. Pan/Vol show
   no numbers (tooltip only); mixer-box wheel **reroutes while scrolling the
   page** (`rack.rs:553-571` steals `smooth_scroll_delta` on hover) — scrolling
   the rack with the cursor over “M” silently moves Kick to Insert 3.
3. **Mute/solo is colour-only.** Green = unmuted, dark = muted, blue = solo —
   no shape/letter change on the LED itself (the `M` box nearby is unrelated).
   Violates the project’s own “colour never the only signal” (`ui-rules.md §1.8`).
4. **Selector is a splinter.** 10×20 bar at the row’s far right end, 100s of px
   from the name it selects; FL’s selector is the row’s outer border. Select-all
   (double-click), Alt+↑↓, Alt+C, Alt+Del all hidden in ▾.
5. **Swing in the wrong place conceptually.** FL swing is a rack/global feel
   control, fine — but ours sits in the rack header while tempo lives in the
   deck and snap in row 2: three timing controls in three places.

### (d) Fix spec
- Row height 30 px kept; strip becomes: `[LED 24][Name flex][M-box 34×22]`;
  move Pan/Vol **into** the instrument window + graph editor (FL keeps
  channel knobs, but ours are unusable at 18 px — either make them 24 px with
  printed values under, or move them; recommend 24 px + value on hover +
  `DragValue` on R-click “Type value”). Fix the wheel-steal: mixer-box wheel
  acts **only with Ctrl held**, plain wheel always scrolls the rack.
- Pads: minimum 26 px wide so `01–16` legends always print (mono 10, 85%
  white on red); velocity = number on hover + bar 3 px; selected steps get a
  1.5 px `FOCUS` ring (keyboard nav parity). LED row: current step = white
  4 px + beat ticks; add beat numbers into tray headers (already “BEAT n” —
  keep, but sentence-case “Beat 1”, not caps).
- Selector = whole-row border: selected row gets 1.5 px `TEXT` outer stroke +
  name `thumb`; keep the right-end bar as a larger 16×24 grip with the same
  tooltip. Double-click selects all (FL parity, 3 lines in `select_channel`).
- Mute LED: unmuted = green dot + `M`? no — unmuted = lit green, muted = hollow
  ring (shape change), solo = blue **plus** “S” engraved under the LED
  (text backup, satisfies §1.8). Same for mixer lamps.
- Keep preview strips; add FL’s mute button on the preview + bar-block shading
  (already have beat grid — darken alternate bars).

---

## 7. Piano roll (`roll.rs`, `screens/02–03`, `07`, `25-roll-1920`)

### (a) Ours
Keys 48 px (white `KEY_WHITE`, blacks 62%, C labels), ruler 24 px, grid
cell 4–160 px (Ctrl+wheel X), row 8–30 px (Alt+wheel Y, Shift+wheel scroll),
lane 88/56/0 px (Vel/Pan/Release/Fine/ModX/Y lollipops), header: ▾ (Edit/Tools/
Snap/Tool/Note color/Target channel/Target control/Ghost/Fit) + Velocity knob
(white) + Tool `dropdown` + Channel `dropdown`. Gestures: click = note (last
len), drag = move (ghost), edges 7 px = resize, Shift+drag = copy, Ctrl+drag =
box select, R-click/R-drag = erase, Alt = skip snap, wheel = zoom, keys =
arrows/Ctrl+octave/Del/Ctrl+A/D/X/C/V/B/Q, tools P/B/D/T/C/E/Z/S, slide S,
porta white slash, note colors 6, ghosts outlines. `25-roll-1920` shows the
system working (coloured groups + muted outline + slide trajectory + lollipops).

### (b) FL — `pianoroll.htm` + `pianoroll_menu.htm` + `basics_shortcuts.htm`
Target-channel dropdown in title bar (wheel = used channels, Ctrl+wheel = all),
auto-locate, double-click clip opens roll scrolled to click (or per Edit-patterns
setting). Tools P/B/D/T/C/E/Z/Y/S; quantize Alt+Q tool vs Ctrl+Q quick;
Ctrl+L legato, Ctrl+U chop, Ctrl+G glue, Alt+V ghosts, double-click note =
properties; R-click deletes; ruler seeks; ghost channels; snap Main/Line/Cell.

### (c) Problems
1. **Tools behind a dropdown.** FL shows 8 tool buttons; ours shows one
   (`Paint B ▾`, 104 px) hiding 7. Switching Draw→Delete costs open+scan+click
   vs one key/click. The `Tool.key` shortcuts exist (P/B/D/T/C/E/Z/S) but are
   taught only in the F1 sheet and tooltips — the header itself teaches nothing.
2. **EDIT menu lies (again).** Roll has real clipboard (Ctrl+C/X/V/B, Del,
   Ctrl+A/D all work in `roll_keys`), yet EDIT → Cut/Copy/Paste are hard-disabled
   (`menu.rs:114-116`). Users will conclude copy is broken and never try Ctrl+C.
3. **Missing FL verbs users will reach for.** No Alt+Q tool (only Ctrl+Q quick),
   no Ctrl+G/U/L, no Alt+V ghost toggle (menu-only), no double-click properties,
   no Y (play-from-click) — Y is unbound; S is Slide (FL playlist S = slip, roll
   S = slide — matches, fine). No zoom buttons (only wheel chords) for trackpads.
4. **Two velocity controls.** Header VELOCITY knob (new notes) + lane lollipops
   (existing) + rack graph (steps) = three velocities with the same name. The
   header knob’s hint (“Edit existing ones in the lane below”) admits the confusion.
5. **Ghost/slide/porta/color encodings have no legend.** Yellow contour + “to C4”,
   white slash, hollow red/blue/green notes, amber trajectory — all unexplained
   in-app (docs only). First-time screenshot `02` looks like a wiring diagram.
6. **Keyboard-vs-typing-piano conflict.** Tool keys fire (`mod.rs:1125-1132`)
   whenever focus = Roll and `!ctrl && !alt` — even with Typing piano ON, so P
   (Draw) steals the P note? Actually KEYS map has no P… but R (record), L (song
   mode), Space (transport) all fire while playing the typing keyboard
   (`mod.rs:1119-1124` unguarded). FL disables single-letter transport/tools when
   typing-piano is on (`basics_shortcuts.htm` note).

### (d) Fix spec
- Header tools: replace Tool `dropdown` with 8 `tool_button`s (Draw Paint Delete
  Mute Slice Select Zoom Slide, 26 px, shortcut in tooltip + selected = `thumb`;
  overflow → `ComboBox` only under 900 px). Keep P/B/D/T/C/E/Z/S keys. Velocity
  knob stays but relabel “New-note vel.” (caps `NEW VEL`, value mono) to end the
  confusion; lane keeps “Velocity”.
- EDIT enablement = selection-aware (see §3). Add Alt+Q (quantize dialog — simple
  strength/start/duration sliders for now), Alt+V (ghosts), Ctrl+G/U/L
  (glue/chop/legato — legato = extend to next onset, 20 lines on tick grid).
  Double-click note = properties popover (vel/pan/release/fine — reuse lane
  editors). Y = play from click (seek + play).
- Lane caption keeps L-click cycle + R-click `context_menu` (already correct) —
  add the same menu to the header VELOCITY knob (“Apply to selected”).
- Gate all single-letter transport/tool keys behind `!typing_keyboard`
  (except Space/Ctrl+Space/Esc/F-keys — FL keeps Space live; document it).
- Add a one-time legend: first roll open shows a dismissible strip
  (“Hollow = other channel · amber line = slide · white slash = porta”).

---

## 8. Playlist (`playlist.rs`, `screens/04-playlist.png`)

### (a) Ours
Header: ▾ (Add/Make unique/Mute/Delete/Song mode) + Add clip / Make unique /
Delete buttons. Grid: head 120 px, ruler 24, lanes 32, bar 24–400 px
(Ctrl+wheel, consumed), clips exact start→end (r2, top strip 13 px with ▾
triangle + name + ×N, note previews, repeat ticks), drag move / edges trim /
Shift copy / R-click delete / double-click → roll, track mute LEDs + R-click
rename (Enter applies), ruler click/drag seeks in Song mode, playhead triangle +
1.5 px line, empty state (“No clips yet” + button).

### (b) FL — `playlist.htm`
Clip Tracks hold Pattern/Audio/Automation clips, unrestricted by default;
clips ≈ notes (playhead executes them). Track headers selectable (Ctrl+↑↓,
Ctrl+Alt extend); double-click clip → Roll (or per Edit-patterns setting:
Automatic/Rack/Roll/Event editor; Alt+double-click = the other editor);
scroll-to-position option; tools B/P/C/E/D/T/Z; Shift+4/5 zoom, Ctrl+Ins/Del
space, Shift+Q quantize starts, Alt+T markers, Shift+G group.

### (c) Problems
1. **R-click deletes arrangement.** See §4 — the worst instance: a full pattern
   clip (bars of work) vanishes on one misclick. FL clip R-click = menu.
2. **No tools, no markers, no groups.** Single gestural mode (place/move/trim)
   vs FL’s 7 tools; no time markers (Alt+T), no grouping, no Ctrl+Ins/Del
   space editing — arranging a song beyond looping one pattern is manual.
3. **Track headers are mute-only stubs.** No solo, no color, no Track-mode
   (Instrument/Audio link — the FL workflow that binds Playlist↔Rack↔Mixer),
   rename hidden in R-click. `04` shows 5× “Track N” — users must invent their
   own mapping and re-learn it every session.
4. **Zoom and snap hidden.** Ctrl+wheel only (no buttons/slider, no key
   alternative); snap follows the *global* deck snap with Alt-bypass — correct,
   but the header never shows which snap is active (must look up at row 2).
5. **Header buttons duplicate gestures.** Add clip (places after last — not at
   click), Make unique, Delete mirror the ▾ menu and R-click; three 26-px buttons
   burn header space that FL gives to tools.

### (d) Fix spec (egui)
- R-click clip/header/empty = `context_menu` (Open in roll / Open in rack /
  Make unique Ctrl+Shift+V / Mute Alt+M / Rename track F2 / Delete Del).
  Keep L-drag move, edge trim (8 px grabs), Shift copy, double-click roll,
  Alt+double-click rack (already correct — keep).
- Header: replace Add/Make-unique/Delete with tool `chips` (Select E / Paint B /
  Delete D / Mute T / Slice C / Zoom Z — same keys as roll, shared `Tool`),
  plus snap readout (mirrors deck) and Follow playhead toggle. Empty-state keeps
  one Add-clip button (единственный).
- Tracks: header gains solo (blue, `S`-style), color dot (pattern color),
  F2 rename inline (already), and Track-mode later (Instrument/Audio) — stub the
  menu item disabled with “Arrives with audio tracks” instead of nothing.
- Zoom: Ctrl+wheel kept + `−/＋` buttons + Shift+4 fit (FL parity), bar range
  shown (“7 bars”, already in sub-header — keep).

---

## 9. Mixer (`mixer.rs`, `screens/05`, `15`, `19`, `20`)

### (a) Ours — F9, floats 94%×62% by default
Strips 64+4 px: number + ● arm (16 px), name 12 px, lamp 18 (green/blue),
pan+sep 20 px side-by-side, fader + 6 px meter, dB mono-10, →M route (inserts).
Inspector 320 px pinned right: editable name (semibold 16), From: channels,
10 slots (24 px rows: LED 16, name, mix 18 px), slot params as knobs (PEQ2 =
band table: Freq blue / Gain orange / Q white + mono values), strip EQ
Low/Mid/High ±18 dB, sends (post-fader, higher-numbered inserts only, Add-send
`dropdown`). FX window per slot (`egui::Window`, On checkbox + Reset + knobs).
Solo = mute-others-keep-chain (blue lamp), arm + R + Play → WAV takes to
`~/Music/KeyFrameXL/Recordings`. `15` shows the slot picker popup;
`19/20` show built-in vs plugin (`evergreen`) slots.

### (b) FL — `mixer.htm` + `mixer_plugin.htm` + `mixer_mixermenu.htm`
500 inserts + Master + Current, 3 docks, scroll under Send switches or wheel over
names; select L-click, multi Ctrl+Shift; 10 FX slots top→bottom; slot menu =
Select/Replace, Delete (“None”), Move up/down (or wheel over name), bypass
(automatable), mix level (leave 100%, do it in-plugin), Patcherize, **Save preset
as .fst (draggable to another track!)**; name L-click opens editor, R-click
renames; sends UI = click destination Send switch, knob replaces it; routing
Ctrl+L / Shift+Ctrl+L; disk arm per track; Alt+R / Shift+Alt+R render armed/
selected; mixer states Open/Save/Browse.

### (c) Problems
1. **Floats over the work.** Default 94%-wide float covers rack + playlist
   (`05`); mixing while arranging = constant open/close (F9 toggle helps, but
   FL users dock the mixer beside the rack). Inside-viewport float also means no
   second-monitor mixer (FL standard).
2. **No presets = no recall.** No “Save mixer track state”, no .fst, no Browse
   states — every mix is trapped in the project. The user’s “normal export of
   instruments” complaint applies to chains most of all.
3. **Slot UX is cramped and double-wired.** 24 px rows with 16 px LED + 18 px
   mix knob leave ~100 px for names (“Parametric EQ 2” elides at 64 px strips?);
   R-click menu + click-empty popup duplicate (`mixer.rs:429-437`); bypass vs
   mix-vs-remove are three adjacent micro-targets with tooltips as the only
   teacher.
4. **Sends restricted without telling.** Only higher-numbered inserts (loop-proof
   by construction — good engineering) but the Add-send `dropdown` silently omits
   lower inserts; users will think routing is broken. FL refuses feedback loops
   with an explanation (“You don’t want that”).
5. **Meters minimal.** 6 px bar, no peak-hold, no clip indicator, no numeric —
   recording takes (`Reverb armed…` in `05/19/20`) give no visible level confidence
   beyond the bar. Faders short at 1366 (body ~240 px → fader ~120 px travel).

### (d) Fix spec
- Default: dock the mixer (bottom split, full width, 280 px tall) instead of
  floating; Float remains via tab menu. At ≥1500 px allow side-dock. Inspector
  stays pinned right (keep 320, resizable 280–400).
- Slots: rows 28 px; LED 20 (bypass), name flex (L-click open, R-click
  `context_menu`: Replace▸ / Move up(down) / Bypass / **Save preset as .fst** /
  Remove), mix knob 22 px only on hover/selected (declutter). One menu system
  (delete the click-empty twin popup).
- Presets now (see §5): mixer-state files + drag slot→track (copy with settings,
  FL parity) + “Browse states” opening the browser Mixer-presets section.
- Sends: show all inserts, disable loop-closers with reason
  (“Insert 1 is upstream — sends go forward only”); keep post-fader.
- Meters: peak-hold line + clip tick (red, clears on click) + dB tooltip;
  fader travel ≥140 px at default height (raise docked mixer min to 300 px).

---

## 10. Plugin / instrument windows (`instrument.rs`, `sampler.rs`, `plugins_ui.rs`, `screens/06`, `19–21`)

### (a) Ours
Channel window floats big (86%×92%): ▾ (Load preset / Piano roll / Delete) +
Piano-roll + Load-preset buttons, 20 px semibold rename, empty state
(“No preset loaded… Drop .dwp here”), sampler editor pages
(Program/Zone/Filter & Env/Modulation/Zones — DirectWave names), drum voices =
static label, plugin channels = vendor/status + Show/Hide editor + path
(`21` adds a VST Grand Piano row with blue lamp). Built-in FX get an
`egui::Window` (title `Kind — Track`, vendor line, On + Reset, knobs; PEQ2 band
table). No preset arrows, no Replace, no Save, no Detach, no scaling.

### (b) FL — `plugins/wrapper.htm` + `chansettings.htm`
Every channel = Wrapper: plugin menu (Piano roll / Replace / Delete / Presets /
Browse presets / **Save preset as (.fst, draggable)** / Add to database /
spare state / params), preset browser arrows + name, typing-keyboard priority,
detailed settings (TRACK routing, pitch, arp), Detached, scaling, Help;
Channel menu adds **Save channel state as (.fst, draggable)** / Browse channel
states / Route to free Ctrl+L / Rename F2. Edison-style tools per content type.

### (c) Problems
1. **Load-only presets.** Load preset dialog + drop exist; Save does not —
   anywhere. Users can build a sound and never get it back except inside the
   project. This is the literal “normal export of instruments” gap.
2. **No wrapper chrome.** Missing: preset prev/next (musicians audition tens of
   presets — ours forces dialog-per-preset), Replace (try another synth in place),
   Detach (second monitor), per-plugin scaling (150% UI zoom will clip VST views),
   typing priority (typing-piano vs plugin keyboard fights).
3. **Sampler editor unjudged.** No screenshot of a *loaded* sampler (only `06`
   empty + drop hint) was provided; the 5-page DirectWave editor (581 lines in
   `sampler.rs`) ships without visual evidence. Zones tables + matrix + ADSR at
   440–720 px float widths risk the same micro-knob density as PEQ2.
4. **Drum voices are dead ends.** “Built-in drum voice… sound parameters arrive
   with the parameter system” (`instrument.rs:236`) = a window that says it does
   nothing. Either give it volume/pitch/decay now or don’t open a window.

### (d) Fix spec
- Wrapper header (all channel kinds): `[▾][Preset ◀ name ▶][Replace][Save…]
  [Detached][?]` — `▾` = FL plugin menu subset (Piano roll / Replace /
  Save channel state as .fst / Browse states / Route to free / Delete);
  arrows step through the channel-preset folder (loads .fst, one undo);
  Save writes .fst into Channel presets and selects it in the browser.
- FX slots: same arrows + Save preset as for effect presets.
- Detached = OS window (egui viewport) when available, else current float
  (label it “Float (OS detach arrives with multi-window)” — honest disabled
  state, not silent).
- Drum voice: temporary real panel (Volume / Pitch / Decay knobs + mute) so the
  window is never an apology.
- Capture a loaded-sampler screenshot at 1366 and 1920 before shipping the
  editor (DoD addition).

---

## 11. Dialogs & popups — export, settings, save prompts

### (a) Ours
Export (`export.rs`, `23-export-dialog.png`): OS save dialog **first**, then a
440 px `Window` anchored top-110: FILE (stem + Change… + folder), SOURCE chips
(Pattern “…” time / Song time + empty-playlist note), TAIL chips
(Leave/Cut/Wrap + hint), FORMATS chips (WAV Off/16/24/32f, FLAC Off/16/24),
OPTIONS (Normalize −1 dB, Dither 16), size estimate, error line, progress bar,
Start Primary + Cancel. Settings (`settings_ui.rs`, `16/17/18`): 480 px Window
top-90, page `chips`, per-page controls, Close only (live-apply), Restart audio
button, Esc closes. Save guard (`project.rs:165`): Save / Don’t save / Cancel,
Enter = Save, Esc = Cancel — correct. Rename (`menu.rs:303`): top-140 TextEdit +
Rename/Cancel — correct. Shortcuts (`menu.rs:254`): 420 px grid of 30 rows —
good content, no search/sections.

### (b) FL — `fformats_save_export.htm`
One Rendering dialog: Mode (Song/Pattern + hierarchy: selection > last marker >
last data > Tail), Length/Total/Disk readouts, Output formats multi-select
(WAV 16/24/32f, MP3, OGG, FLAC 16/24, MIDI), Quality (resampling/dither),
Extra (save tempo info, **Split mixer tracks + Include Master/Current**,
Trim PDC, Enable insert/master FX). MIDI export requires Prepare-for-MIDI macro
(replace channels with MIDI-Out, **save a copy first** — destructive, warned).

### (c) Problems
1. **Backwards file flow.** OS dialog before options means users pick
   `demo.wav` then learn the song is 0:02 and empty — then Change… and re-pick.
   FL picks format/options first, filename last (Start → file dialog).
2. **No stems.** No Split mixer tracks / All playlist tracks — the professional
   deliverable. Combined with no MP3/OGG, export covers demos only.
3. **No destructive-action guardrails on MIDI.** Ours exports MIDI directly
   (Ctrl+Shift+M, SMF format 1 — actually *better* than FL’s replace-channels
   dance), but nothing explains what is exported (song if clips else pattern —
   shown, good) or warns about GM mapping (FL solves via LSD + Prepare macro).
4. **Dialogs are modeless.** Export/Settings/Shortcuts are `egui::Window`s —
   clicks pass behind, transport keeps running during render setup, Esc on
   export = Cancel (destroys the render config). FL Rendering is modal.
5. **Settings hides scaling.** Interface size lives in VIEW ▸ (no % readout
   except Reset label) while FL puts Main/popups/toolbars/cursor scaling in
   General settings with restart notes (`envsettings_general.htm`).

### (d) Fix spec (egui)
- Export: single modal `Window` (grab focus, dim behind via full-rect
  `rect_filled(BLACK 40%)` — standard modal pattern): SOURCE / TAIL / FORMATS
  (add MP3 + OGG later; **add Split mixer tracks + Include Master now** —
  render loop already per-insert capable per `plan D2`) / OPTIONS + summary
  (bars, H:M:S, MB) / filename field + Browse… / Start–Cancel. Start → OS
  dialog (suggested name prefilled) → progress (modal, Cancel stops worker,
  config retained). Keep Enter = Start, Esc = Cancel (config kept, no render).
- Settings: move Interface size into General page as % slider (80–150, restart
  note honoured: egui zoom is live — say “applies instantly”) + keep VIEW
  shortcuts; add per-plugin scaling stub (disabled, “Arrives with hosted VST
  views”).
- Shortcuts sheet: group by section (Transport / Windows / Rack / Roll /
  Playlist / Mixer / Browser), add the missing keys from §13, add a filter
  `TextEdit` on top (30+ rows need it).
- All modal dialogs: `order(Order::Foreground)`, Esc = close-without-action,
  Enter = primary, focus trap (first field `request_focus`).

---

## 12. Interface scaling / zoom / DPI + minimum sizes

### (a) Ours
`Ctrl+= / Ctrl+- / Ctrl+0` → `ctx.set_zoom_factor` 0.8–1.5, persisted in
`workspace.ron` with layout + browser visibility (`workspace.rs:91-101`).
Min 900×620, start 1280×820, sidebar 200 (164 <1020), detail 320
(`ui-rules.md §1.4`). Type: sans 14 / button 14–13 / caps 10–11 / mono 12–13 /
heading 20 (`theme.rs:184-190`). Hit rule ≥24 px grid / 32 touch / 44 transport
(`ui-rules.md`) — widely violated (§4/6/8/9).

### (b) FL — `envsettings_general.htm`
System-following Main GUI scaling + **independent** popups / toolbars / cursor /
legacy (100/200/300/400%) scalings, restart-to-apply, 125–200% guidance at
4K–5K, PPI calibration, per-plugin Wrapper Scaling (native vs bridged+DPI-aware
vs external-window). Animations / transparency / high-visibility / thick-lines
separately tunable.

### (c) Problems
1. **One zoom for everything.** UI text, pads, popups, plugin views all scale
   together; at 125% the fixed constants (BROWSER_W 208, NAME_W 230,
   INSPECTOR_W 320, LCD 252, slot rows 24) overflow their containers instead of
   reflowing. FL separates Main / popups / toolbars / cursor for exactly this.
2. **Below-minimum type everywhere.** Caps 10–11 px mono-uppercase-with-tracking
   is the *dominant* label style (TRACK, BEAT 1, PATTERN 1, INSERTS, FROM:, SENDS,
   STRIP EQ, caps context in every header) — below the project’s own 12 px floor
   and far below FL’s vector UI at 125–200%. At 1366×768 screenshots the BEAT
   labels and `×6` badges are ~7 px tall — unreadable on HiDPI without zoom.
3. **No plugin scaling story.** Hosted VST views (`plugins_ui.rs`) have no
   Scaling menu; 150% UI zoom + unscaled plugin = clipped editors (FL Wrapper →
   Scaling exists precisely here).
4. **Minimum sizes untested.** 900×620 min vs 1366×768 screenshots already
   scroll two axes in rack + playlist + mixer-inspector simultaneously; at
   900×620 the dock shows ~2 lanes + 2 channels — composition impossible. No
   responsive collapse (e.g. browser auto-hide <1020 is specified but screenshots
   never show it).

### (d) Fix spec
- Keep egui zoom, add: (1) % readout in status bar right block
  (“UI 100%” next to `48.0 KHZ · PPQ 480`); (2) General-settings slider 80–150%
  + note “panels reflow, plugins scale separately”; (3) breakpoints: <1100 px
  browser collapses to icons (windows/patterns/channels badges only), <800 px
  height inspector becomes bottom sheet. Enforce min row 28 px / min hit 24 px
  in `kit` (assert in debug: `debug_assert!(size.y >= 24.0)` on buttons).
- Type floor: caps labels 10–11 → **12 px minimum** (tracking 0.08em at 12,
  0.12em only ≥13); prose stays 13–14 sans sentence-case. This single change
  fixes half the “inconvenient and ugly” complaint at zero layout cost.
- Plugin views: add Wrapper → Scaling submenu (System / 100 / 125 / 150 / 200%,
  stored per plugin) before 150% ships.

---

## 13. Keyboard shortcuts (ours vs FL)

### Ours today (`mod.rs:1023-1148` + `menu.rs:255-296`, sheet `22-shortcuts.png`)
Space / Ctrl+Space · L · R · F5 F6 F7 F9 · Alt+F8 · F12 · Alt+1–3 · Ctrl+Z /
Ctrl+Shift+Z / Ctrl+Y · Ctrl+Shift+N / Ctrl+O / Ctrl+S · Ctrl+N (new version) ·
Ctrl+Shift+S · Ctrl+R · Ctrl+Shift+M · F2 · F4 · Ctrl+1–3 (mute **first 3 only**)
· P B D T / C E Z S (roll tools, focus-gated) · Ctrl+Q / Ctrl+B (roll) ·
Ctrl+L / Ctrl+K (rack) · Alt-drag (skip snap) · Shift (fine) · double-click
(reset) · R-click knob (menu) · Ctrl+H · F10 · Ctrl+M / Ctrl+T · Ctrl+±/0 · F1.

### FL (`basics_shortcuts.htm` + cheatsheets)
1–9,0 mute / Ctrl+1–9,0 solo (first **10**); ↑↓ select, PgUp/Dn groups,
Alt+C/G/M/Z/U, Alt+↑↓ move, Shift+wheel move, Ctrl+Shift+←→ shift steps,
Ctrl+C/X/V channel steps, Ctrl+Enter focus playlist; transport Space /
Ctrl+Space / L / R / Ctrl+H / Ctrl+I / Ctrl+M / Ctrl+P / Ctrl+T / numpad;
windows F5–F12 + Tab + Enter max + Esc + **Ctrl+Shift+H arrange** + F8 picker +
Ctrl+F8 project picker + F10 MIDI + F11 info; mixer S / F2 / Alt+←→ / Ctrl+L /
Alt+R(+Shift); roll Alt+Q / Ctrl+Q / Ctrl+G/U/L / Alt+V / Shift+arrows /
Ctrl+↑↓ octave / Alt+arrows nudge / double-click props; playlist B/P/C/E/D/T/Z
+ Ctrl+B/A/D + Shift+C/G + Ctrl+Ins/Del + Shift+Q + Alt+T; browser Enter replace,
Ctrl+Alt+click replace, Shift+↑↓ step, F8/Ctrl+F8.

### Concrete gaps (fix column = §14 IDs)
1. **Ctrl+1–3 → 1–0 (10 channels)**, add Ctrl+1–0 solo (FL core loop skill).
2. Add **F8 picker** (new window, §5) + Ctrl+F8 project picker (recent + patterns).
3. Add **Ctrl+Shift+H arrange** (tile Playlist/Rack/Mixer like FL) + **Tab**
   cycle windows + **Enter** maximise playlist + **Esc** close topmost.
4. Roll: **Alt+Q** tool, **Alt+V** ghosts, **Ctrl+G/U/L**, **Y** play-from-click,
   double-click props (§7).
5. Playlist tools **B/P/C/E/D/T/Z** when focused (§8) + Shift+4/5, Ctrl+Ins/Del,
   Alt+T markers (markers after tools land).
6. Mixer: **S** solo current, **F2** rename, **Alt+←→** move, **Alt+R** render
   armed (§9); rack: **Ctrl+Enter** focus playlist, **Alt+G/Z/U** groups later.
7. Browser: **Shift+↑↓** audition-step, **Ctrl+Alt+click** replace (when mouse
   modifiers detectable), **F8/Ctrl+F8** focus jumps.
8. Fix gating: single-letter keys (R/L/tools) disabled while Typing piano is ON
   (except Space/F-keys/Esc); document Space-live exception in the sheet.

Proposed full default keymap (ship as the F1 sheet, sections + filter):

| Section | Key | Action |
|---|---|---|
| Transport | Space / Ctrl+Space / L / R / Ctrl+M / Ctrl+T / Ctrl+H / Ctrl+P* | Play-stop / pause / Pat-Song / record / metronome / typing / panic / count-in* (*when recording lands) |
| Windows | F5 F6 F7 F9 / F8 / Alt+F8 / F10 / F11* / F12 / Tab / Enter / Esc / Ctrl+Shift+H / Alt+1–3 | standard + picker / browser / settings / info* / close-all / cycle / maximise / close-top / arrange / workspaces |
| Rack | 1–0 / Ctrl+1–0 / ↑↓ / Alt+↑↓ / Alt+C / Alt+Del / Ctrl+C/X/V / Ctrl+L / Ctrl+K / Ctrl+Enter | mute / solo / select / move / clone / delete / steps clipboard / route / graph / focus playlist |
| Roll | P B D T C E Z S (+Y) / Ctrl+A/D/X/C/V/B / Del / arrows(+Shift/Alt/Ctrl) / Ctrl+Q / Alt+Q / Alt+V / Ctrl+G/U/L | tools / edit / move / quantize / ghosts / glue-chop-legato |
| Playlist | E B P C D T Z / Ctrl+B/A/D / Del / Shift+arrows / Shift+Q / Alt+T* | tools / duplicate-select / move / quantize-starts / markers* |
| Mixer | S / F2 / Alt+←→ / Ctrl+L / Alt+R* | solo / rename / move / link / render-armed* |
| Browser | ↑↓←→ / Enter / Shift+↑↓ / Esc | walk / load / audition-step / stop preview |

---

## 14. Export & presets of instruments/channels/mixer

### (a) Ours
Audio Ctrl+R (WAV 16/24/32f + FLAC 16/24, Leave/Cut/Wrap, −1 dB normalize,
TPDF-16, background + Cancel) — `export.rs`. MIDI Ctrl+Shift+M import/export
(SMF fmt1, 480 PPQ; ch10 → Kick/Snare/Hat; CC7/10 → vol/pan; Master limiter if
absent). Preset load: `.dwp` + wav/flac/ogg into samplers (dialog/drop/drag,
3-at-once background). No save-preset anywhere; no .fst; no stems; no MP3/OGG
out; no mixer-state files; no plugin-preset handling beyond in-project state.

### (b) FL
Wave/MP3/MIDI/Stems from one dialog (`fformats_save_export.htm`): Split mixer
tracks (one WAV per insert, Master optionally included, muted skipped, Master
FX only on Master) or Export-all-playlist-tracks; insert/master-FX toggles;
tempo-info/PDC-trim. Channel: Save channel state as .fst (draggable to Browser /
Rack / another instance); Mixer slot: Save preset as .fst (draggable to another
slot/track); Plugin database favorites (.fst + .nfo + .bmp) feed Picker/menus
(`browser.htm`, `mixer_plugin.htm`, `plugins/wrapper.htm`, `fformats_other_fst.htm`).

### (c) Problems — the user’s “normal export of instruments” verbatim
1. A sound cannot leave the project. No channel .fst, no mixer-state, no effect
   .fst, no drag-out. Collaboration = send the whole .kfxl + samples.
2. A mix cannot leave either. No stems (split inserts), no playlist-tracks
   render — the two professional deliverables.
3. No MP3/OGG out (sharing), no tempo-info in WAV (loops), no insert/master-FX
   toggles (FL has both; ours always renders everything — stem requests will
   print the Master limiter onto every file).

### (d) Fix spec
1. **Channel .fst now**: JSON/RON `{kind, params|preset-ref, pan, vol, mixer,
   color, name}` + Save/Load in wrapper ▾ + browser section + drag
   (`FileDrag` already exists — add `PresetDrag`). Effort M (2–3 d).
2. **Mixer-state now**: same for one track (slots + EQ + sends) + slot-preset
   files; slot menu Save/Replace; drag slot→track. Effort M.
3. **Stems now**: export Mode += “Split inserts (stems)”: one WAV per used
   insert (+ optional Master), muted skipped, Master FX excluded from stems
   (FL rule — document it). Effort M (render loop per-insert; UI = 3 radio rows).
4. Later: MP3/OGG out (decoder exists via lewton/claxon; encoder = new dep —
   L), tempo-info chunk (S), insert/master-FX toggles (S).

---

## 15. Skeuomorphism / Teenage Engineering — what TE actually does, what FL does, what we do, and the direction

### What TE actually does (guides above)
- **Physical truth first.** OP-1 field = anodized aluminum slab, low-profile
  keys, 4 endless encoders (blue/ochre/gray/orange) that are also push-buttons;
  Shift = fine, tap = confirm/default. TX-6 = aluminum + leather back, custom
  knobs/faders, select knob + button over a 48×64 mono OLED. EP-133 = plastic
  but honest: 12 pads + 4 groups + fader + mic/speaker, pressure-sensitive
  Punch-In FX that combine (filter+filter = bandpass), icon-map screen that
  never pretends to be more than icons + numbers.
- **Colour = address, red = record.** OP-XY is explicit: all-grayscale UI,
  bright red **only** for recording + parameter locks; vital keys get LEDs.
  OP-1 field muted the palette vs the original (colour off the key legends
  except Record/Mic). Screen text is small LCD type, values big; `HELP` + `SHIFT`
  combos are documented on-device, never hidden.
- **Mechanical honesty, no chrome.** Seams, screws-as-design (OP-1 velcro rings,
  TX-6 leather), no blur/glass, no glow except LEDs. Undo was *refused* for 15
  years on tape (“stay true to the 4-track”) then added as tape-undo — metaphor
  preserved, function delivered.
- **What TE never does:** 9 px ghost text as the only label; nested menus three
  deep; one knob duplicated 12 px from an identical field; destructive R-click
  with no menu.

### What FL does
Restrained skeuomorphism where it pays (channel buttons feel pressable, mixer
faders travel, slot LEDs, wrapper preset arrows) + flat vector chrome everywhere
else + per-element scaling to 400%. Knobs are the *exception*, not the rule —
most parameters are sliders/fields/menus. Colour marks state engineers need at
a glance (armed red, solo green/blue, selected green faders).

### What we do (`theme.rs`, `kit.rs:raised/sunken/knob/led/seg_text/caps`)
The kit is well-built (one light source, 2 depths, ghost segments, no-glow LEDs,
tracking caps) but misapplied: **caps-10px labels everything**, gradients on
hundreds of pads, red on every lit step *and* Play *and* faders, two tempos,
icon-only buttons, popups reinvented 5 ways. It feels like TE at thumbnail size
and like a puzzle up close — the user’s “inconvenient and ugly” in one sentence.

### Concrete visual direction (rules to implement, not vibes)
1. **Materials:** matte panels (`PANEL_TOP→BOTTOM` kept), 1 px `EDGE` seams
   between modules, sunken wells only for time/value displays (LCD, ruler,
   inspector wells). No blur, no glass, no outer glow except ● record/armed.
2. **Depth:** 2 levels only. Pressed = lip disappears + face −2 px (already) +
   optional 60 ms click (audible honesty, off by default). No shadow under pads.
3. **Controls:** pads square r4, 26 px min, printed `01–16`, travel on press;
   knobs endless with tick arc + value (already) but 24 px min + numeric under
   on hover; faders with unity mark (already) + peak-hold; LEDs = shape + colour
   (lit dot / hollow ring / blue+S).
4. **Colour:** red reserved: Play, Record/armed, taking. Lit steps = warm white
   (`ENC_WHITE` at 90%) with red **velocity bar only** — one red per screen
   becomes ~16 small bars, acceptable because they encode data, not state.
   Addresses: keep blue/green/orange/white encoder code (= which knob, never
   good/bad). Semantic green/blue/red keep FL meanings (on/solo/clip).
5. **Type:** Plex kept. Floor 12 px. Caps (0.08–0.12em) for module tags only
   (`Rack · Pattern 01`, `Beat 1` → sentence-case? keep caps but 12 px);
   everything else sentence-case sans 13–14; machine values mono tabular.
6. **Iconography:** keep the 20-px 1.5-stroke set, always **with text** in the
   deck (Playlist/Rack/Roll/Mixer/Browser). No icon-only chrome above 900 px.
7. **What NOT to do:** no new custom popup/menu/knob/slider primitives (use
   egui `ComboBox`/`context_menu`/`DragValue`/`Slider` skinned via `theme.rs`);
   no bottom search; no R-click delete; no dead menu rows; no sub-12px labels;
   no second tempo control; no covering floats as defaults.

---

## 16. Prioritized fix list (P0 = blocks music, P1 = daily pain, P2 = polish)

**P0 — do first, in this order (each shippable alone):**
1. P0-1 R-click never deletes (clip/step/mixer-box) — menus instead (§4). S (0.5 d).
2. P0-2 EDIT Cut/Copy/Paste reflect selection; remove/hide other dead rows (§3). S.
3. P0-3 Channel .fst save/load + browser Channel-presets section (§5/§14). M (2–3 d).
4. P0-4 Stems export (Split inserts + Master opt) (§11/§14). M (2–3 d).
5. P0-5 Single-letter gating with Typing piano (R/L/tools) (§7/§13). S.
6. P0-6 Replace custom menu-popups with `context_menu`/`ComboBox` (§4). M (2 d).
   *Marks the “reinventing the wheel” core: `kit::dropdown`, clip popup,
   mixer-box popup, snap/pattern/tool popups → standard egui.*
7. P0-7 Type floor 12 px + sentence-case prose (§12). S (1 d, biggest visual win).
8. P0-8 Deck: delete TEMPO knob, LCD becomes editable field + steppers, text
   window buttons, cursor-hint bar (§2). M (2 d).

**P1 — next:**
9. P1-1 Mixer docked by default + slot rows 28 px + one menu + presets (§9). M.
10. P1-2 F8 Plugin Picker + browser Favorites + Shift+↑↓ audition (§5). M.
11. P1-3 Piano-roll toolbar (8 buttons) + Alt+Q/V + Ctrl+G/U/L + Y (§7). M.
12. P1-4 Playlist tools + track solo/color + zoom buttons + header cleanup (§8). M.
13. P1-5 Rack strip: 24 px targets, wheel-reroute fix, row-border select,
   hollow-ring mute (§6). M.
14. P1-6 Full keymap: 1–0/Ctrl+1–0, Ctrl+Shift+H, Tab, F8, S/F2/Alt+←→, Esc
   discipline + grouped filterable F1 sheet (§13). S–M.
15. P1-7 Drum-voice real panel; wrapper preset arrows + Replace + Save (§10). M.
16. P1-8 Export modal flow (options→file), summary bars/H:M:S/MB (§11). S.
17. P1-9 Mixer-state + slot .fst + drag slot→track (§14). M.
18. P1-10 Resizable browser + breakpoints + status UI% (§12). S–M.

**P2 — after P0–P1:**
19. P2-1 OS-detach for mixer/wrapper (multi-monitor) (§1/§10). L.
20. P2-2 Meters peak-hold/clip, sends explanation, PEQ loaded-sampler screenshot DoD (§9/§10). S.
21. P2-3 Markers/groups/space-editing, MP3/OGG out, tempo-chunk, FX toggles (§8/§11). M–L.
22. P2-4 Per-plugin scaling + cursor/popup scaling separation (§12). M.
23. P2-5 Pressure/combine punch-in-style performance (TE-inspired, *after* basics
   are normal — never before). L.

Effort: S ≤1 d, M 2–4 d, L ≥1 wk. Total P0 ≈ 10–14 d; P0+P1 ≈ 5–7 wk with checks.

---

*End of audit. No code or commits were made. Reference screenshots:*
*`research_notes/ux-audit/screens/01–25`. FL/TE sources linked inline above.*
