# FILE TREE / BROWSER and CONTROL UX (knobs, sliders, scrolling, keyboard)

Research date: 2026-11-15. Scope: our future file Browser + all continuous-control UX.
Grounding: FL claims cite the Image-Line online manual page URL. Anything else is
marked UNVERIFIED. Code refs are `path:line` in this repo (egui 0.31.1, egui_dock 0.16).

Manual URLs used throughout (shorthand):
- `[BROWSER]` = https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/browser.htm
- `[FILES]` = https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_files.htm
- `[GENERAL]` = https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm
- `[SHORTCUTS]` = https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm
- `[SAMPLER]` = https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_sampler.htm
- `[ABLETON-KEYS]` = https://www.ableton.com/en/live-manual/11/live-keyboard-shortcuts/
- `[ABLETON-PREVIEW]` = https://help.ableton.com/hc/en-us/articles/209068529-Previewing-clips-in-browser-doesn-t-sync-to-tempo
- `[BITWIG-BROWSER]` = https://www.bitwig.com/userguide/latest/common_browser_elements
- `[REAPER-UG]` = https://www.reaper.fm/userguide/ReaperUserGuide740c.pdf (§4 Media Explorer)

---

## 1. Browser

### 1.1 FL's tabs

Default tabs along the top: **All, Current project, Plugin database, Online content
(Presets/Library), Starred (Favorites)**. Since FL 21/24 there is also a **Sounds**
(FL Cloud) tab. (`[BROWSER]` §"Browser", §"Tabs & Tab Menu", §"Sounds (FL Cloud)").

Per-tab settings (right-click a tab): Rename/color/icon, Hide/Show hidden/Delete (user
tabs), Clone, Move left/right, **Save search result as tab**, View Zoom
(Compact/Normal/Large/Huge/Massive), Show-only-one-folder, Show folder icons / images /
**preview / sample waveform inline / file extensions / unknown file types**, **Frozen**
(freezes current state as the default view at next startup), **Sort by Name/Type/Date/
Group**, View layout Standard / Columns / Samples / Plugins. (`[BROWSER]` §Tabs & Tab Menu.)

Our screenshot (task brief) shows exactly this: "Plugin database > Effects / Generators /
Installed …" tree with highlighted favourites, TAGS + search at the bottom.

**What this means for us:** tabs are not hardcoded views, they are saved queries/places
with display options. Our Browser should be a list of user-editable roots, not a fixed
panel. "Frozen" is FL's word for what we would call a workspace-pinned browser state.

### 1.2 The tree

Single-column tree by default; "Columns" layout opens a second column for folder content;
"Samples" layout = columns + playable audio rows; "Plugins" = grid. Left-click a folder
expands/collapses it. Hovering a folder name shows a description in the **hint bar**.
Collapse-all and Refresh (reread) buttons sit in the browser header. Scrollbar:
click-drag, click-to-focus then mouse-wheel; **Shift+wheel = Quick Zoom** (font/spacing
zoom, not content zoom). (`[BROWSER]` §Browser Controls, §Tabs & Tab Menu.)

**File rows:** mouse-over a file reveals a star (favorite) at the row end and,
for audio, transport/preview controls. (`[BROWSER]` §Starred, §Preview Player.)

### 1.3 Adding user folders

Two ways, both verified (`[BROWSER]` §Adding your own folders, `[FILES]` §Browser extra
search folders):
1. Drag-and-drop any OS folder onto the FL Browser — it becomes an extra search folder.
2. Options > File settings > **Browser extra search folders**: up to **30** custom folders,
   each with an optional display **Name**; unnamed rows show the target folder name.
   Custom folders render in **green** so they read as user content.

Consequences FL documents: these folders are the *only* places FL searches when loading
projects (missing-sample search), and the manual warns against setting a drive root
(it makes project load + startup re-index slow). There is a startup option
"Re-index changed folders on startup". (`[BROWSER]` §Browser Options Menu, `[FILES]`.)

**What this means for us:** same two affordances (OS-drop onto Browser + a settings list),
same 30-ish cap reasoning, same warning about huge roots. Our settings live wherever our
File settings go; root list must be persisted (our config dir, same place as
`workspace.ron`).

### 1.4 Search and tags

Search bar lives at the **bottom** by default (movable to top). Controls (`[BROWSER]`
§Search):
- **Type** filter (only show selected content kinds), **Any/All** tag-logic switch,
  **Tags** toggle.
- Per-file: star = favorite; **Tags** = pick existing; **Add a tag** (comma-separated,
  e.g. `Kick,808,C#` adds 3 tags); Edit tags; removing = right-click > Delete tags.
- **Find** (Alt+F, exact phrase), **Browser find** (Ctrl+F, wildcards `sytr*`, `s*us`,
  boolean AND/OR/NOT), **find in selected folder** (Shift+F), **Find previous/next**
  (F2/F3). Esc cancels. Search options: show parent folders, auto-expand folders,
  include folders or not, search box top/bottom, type-to-filter, load plugin tags,
  **Enable indexing** (off = only the selected folder is searched).
- Saved searches: "Save search result as tab" (per-tab menu).

**What this means for us:** tags are per-file sidecar data. FL stores them God-knows-where
(UNVERIFIED — presumably the user data folder); we should store ours explicitly:
`<name>.kfxl-meta` sidecar or one `tags.ron` per root. Decide at implementation; the doc
below proposes sidecar RON next to a per-root index cache. Until then: filename search
only (P1), tags P3.

### 1.5 Sorting

Per-tab Sort by **Name / Type (extension) / Date / Group** (Group = order from `XYZ.nfo`
`SortGroup=` files — FL-specific, we ignore). (`[BROWSER]` §Tabs & Tab Menu.)

### 1.6 Previewing samples on click

Verified behavior (`[BROWSER]` §Preview Player, `[GENERAL]` §Advanced):
- **Left-click** an audio item (`.mp3 .m4a .ogg .wav`) plays it. Default: **first 5 seconds
  only**; Browser menu > **Full sample preview** (or the Preview Player's Play button, or
  **Alt+click**) plays to the end. Clicking transport Stop ends preview early.
- **Shift+click**: on release, playback starts **from the mouse position** in the file.
- **Volume**: mouse-over the preview player reveals a volume control.
- **Tempo-synced preview**: `.wav` files can carry tempo/tempo-sync metadata; if
  `[GENERAL]` "Read sample tempo information" is on, preview is **time-stretched to the
  project tempo**. Wrongly-tagged files stretch wrongly — FL's own manual calls this out
  and points at Edison's File properties dialog to fix tags. "Read sample root note"
  similarly retunes pitched preview/loading.
- **Sounds-tab extras** (FL Cloud content only, `[BROWSER]` §Sounds): per-preview
  **BPM / Loop / Key / Sync** controls; with **Sync** on, samples dropped on the Playlist
  keep the preview tempo/key. One-shots ignore Sync. (`[BROWSER]` §Menu Bar > Audio:
  "Enable auto time stretch", "Enable auto pitch shift".)
- Latency: `[GENERAL]` "Fast sample preview" streams instead of fully loading
  (for large files / slow media).
- Stepping: **Shift+Up/Down** steps through folder items **with preview** into the
  selected channel (see 1.8).

**What this means for us:** click-preview is the single most auditioned interaction in a
DAW browser. Requirements: click = instant sound (<50 ms to first audible output is the
bar; FL streams rather than loads for exactly this reason), volume control, stop on
transport stop, tempo-sync preview (needs our time-stretch — we have none yet; preview
without stretch first, stretch later), preview through the **same audio device** as the
engine (cpal stream we already own in `src/audio.rs`).

### 1.7 Drag & drop targets in FL

Verified (`[BROWSER]` §Working with files, `[SAMPLER]`):
- **Instrument presets** → Channels, or **Mixer tracks** (auto-routes the channel there).
- **Effects** → Mixer tracks / FX slots.
- **Samples** → Playlist (creates Audio Clips; onto a **track header** activates/loads),
  or instruments that accept samples (Sampler, Slicer, Granulizer…); multi-select via
  OS file browser → drop on Playlist track headers or Channel rack.
- **Projects** → dropped on the FL desktop to open.
- **Hold Alt** while dropping content = the Channel window does **not** auto-open.
- Middle-click an item = load onto the selected channel; right-click > "Send to selected
  channel or focused plugin" / "Open in new … channel" variants.
- OS files dropped on FL: same handling (our `dropped_files` in
  `src/ui/instrument.rs:61-77` already mirrors a subset: `.dwp` → focused/new sampler,
  `.kfxl` → open).

**What this means for us:** internal drag (browser row → rack/playlist/roll/mixer slot)
is the core gesture. egui 0.31.1 gives us the whole primitive set — verified in vendored
source `egui-0.31.1/src/ui.rs:2772` (`dnd_drag_source`), `:2824` (`dnd_drop_zone`),
`response.rs:456` (`dnd_release_payload`), `drag_and_drop.rs:19-140`
(`DragAndDrop::set_payload/payload/take_payload/has_payload_of_type`, Esc aborts,
auto-clears on mouse release, `Grabbing` cursor while dragging). Payload is
`Any + Send + Sync`, i.e. our own `BrowserDrop { kind, path }` enum travels for free.
Drop zones highlight via the passed `Frame` (active/inactive/grayed-out styles handled
by egui itself).

### 1.8 Context menus

Verified per content type (`[BROWSER]` §File Menu, §Folder Menus):
- File: Send to selected channel / Open in new channel / Open in new Sampler|Granulizer|
  Slicer|Slicex channel / Edit in audio editor|pitch corrector|time warper /
  Add to plugin database (flag as favorite) / Favorite / Tags / Add/Edit/Delete tags /
  Windows shell menu / Open parent folder / Locate file / **Delete file** (with warning).
- Folder: Expand all / Collapse all / Open in new tab / Add folder to tab / Change color /
  Change icon / shell menu / Open (in OS browser).
- Project files: Open (right-click), middle-click, or DnD onto desktop; `.zip` = with
  samples.

**What this means for us:** right-click on our rows must at minimum offer: load/preset
into focused-or-new channel, reveal in OS browser, favorite/star, delete (P3+). The
automation-linking half of FL's menus (create automation clip, link to controller) only
becomes relevant once we have automatable params + a Current-project equivalent (P4).

### 1.9 Keyboard navigation

Verified (`[SHORTCUTS]` §Browser action, §Window Navigation):
- **Shift+Up/Down**: step through folder items with preview (= audition while stepping;
  the documented swap-samples-while-playing workflow is: select channel → click first
  item → Shift+Up/Down). (`[BROWSER]` §Working with files.)
- **Enter**: replace channel with browser item. **Ctrl+Alt+Click**: same via mouse.
- **Right-click / Shift+Left-click**: start playback from cursor position.
- **Backspace / Shift+Backspace**: browser navigation history prev/next.
- **Ctrl+F / F2 / F3**: find / prev / next. **Ctrl+Up**: collapse structure. **Ctrl+R**:
  reread structure.
- **Alt+F8**: show/hide browser. **Ctrl+F8**: project picker. **F8**: plugin picker.
- Arrow-key treewalk (Up/Down move, Left/Right collapse/expand, type-ahead jump):
  UNVERIFIED in the manual (not in the shortcuts table) but standard tree behavior; adopt
  it — nobody will file a bug that arrows work like every other tree.

### 1.10 File types, "snapshots"

- FL shows audio, `.flp/.zip` projects, `.fst` plugin presets, `.dwp` (DirectWave),
  scores, and optionally unknown types (`[BROWSER]` §Tabs & Tab Menu "Show unknown file
  types", §File Menu project rows).
- "Snapshots": FL has no feature literally called browser snapshots. The closest
  verified things: per-tab **Frozen** state (`[BROWSER]` §Tabs & Tab Menu — "Makes the
  current state the default view… at the next startup") and **Save Mixer track state**
  (`[SHORTCUTS]` §Mixer: Ctrl+Shift+S). This doc treats "snapshots" as (a) frozen/pinned
  browser tabs and (b) restorable window layouts — the latter already exists
  (`workspace.ron`, `src/ui/workspace.rs`). If the requester meant mixer snapshots, FL
  does not have them; defer.
- Current-project tab contents (verified, `[BROWSER]` §Current project): History (undo
  list — click jumps to an undo point), Patterns (+ Initialized controls & Automation),
  Effects/Generators (per-plugin **all automatable controls**; left-click-hold = quick
  tweak popup; right-click = Create automation clip / Edit events / Link to controller /
  Override generic link), Remote control, Initialized controls, Samples. This is the
  future home of our automation UI — not P1.

### 1.11 Where FL is weak — Ableton / Bitwig / Reaper compared

| Area | FL | Ableton Live | Bitwig | Reaper (Media Explorer) | Read for us |
|---|---|---|---|---|---|
| Places/roots | 30 extra folders, green, drag-OS-folder to add | **Places** (user folders) + Packs + User Library | Locations + **Smart Collections** (saved multi-source queries) | Shortcuts + **Databases** (indexed catalogs) | FL's model is fine; adopt Bitwig's saved-query idea via FL's own "Save search result as tab" |
| Favorites | Starred tab (star on hover) | **Collections** (7 color slots, key-mappable) | Collections + Smart Collections | Favorites in DB | Colored collections beat a single star list; P3 |
| Search | Ctrl+F + wildcards/boolean, opt-in indexing | Live text filter, fast | Full-text + device/content-type facets | Filter + pitch/tempo columns | FL's opt-in indexing is the weak point: default should be indexed, not "selected folder only" |
| Preview sync | 5 s default; wav-meta stretch if enabled; Sounds-tab Sync/Key | Preview syncs when transport playing (`[ABLETON-PREVIEW]`) | Preview player with modes (`[BITWIG-BROWSER]`) | **Tempo match Off/On/Half/Double** + pitch knob + preserve-pitch toggle, all in preview (`[REAPER-UG]` §4.6) | Reaper is best-in-class here; our preview bar should grow a tempo-match + root/pitch control (P3) |
| Preview audition flow | Click plays; Shift+Up/Down steps with preview | Click/arrows preview | Click previews | Autoplay toggle, start-on-bar | Ours: click + arrows + autoplay toggle |
| Metadata shown | Type filter; BPM/key only in Sounds tab | BPM/key shown, warpable | BPM/key/length facets | Tempo, pitch, length, gain columns | Show duration + detected BPM for local files (P2/P3; needs analysis) |
| Plugin DB | Plugin database tree + Installed + favorites + thumbnails; F8 picker; type-to-filter | Built-in devices + Plug-Ins labels | Package-style browser, devices first | FX browser separate | Our Plugin database = sampler presets (+ later generators); keep Installed-vs-Favorites split |

Honest summary: FL's browser is powerful but discoverability-poor (30-folder cap,
indexing off-by-default surprises, preview-shortening default, metadata features gated
to the Cloud tab). We copy its structure (tabs/roots/preview/step-audition/DnD) and
skip its traps: index everything by default, full preview by default with a visible
length cap, tempo-match controls on local files from day one of the preview bar.

### 1.12 Proposal: our Browser

**Model.**
```rust
struct BrowserRoot { name: String, path: PathBuf, kind: RootKind }
// RootKind: Project | Samples | Presets | Favorites(virtual) | CurrentProject(virtual)
struct FileEntry { path, name, ext, kind: FileKind, size, mtime, duration: Option<f64>, starred: bool }
enum FileKind { Project, PresetDwp, Sample, Midi, Unknown }
struct BrowserState { roots: Vec<BrowserRoot>, expanded: HashSet<PathBuf>, selected: Option<PathBuf>,
                      filter: String, type_filter: TypeFilter, sort: SortBy, preview: PreviewState }
```
- Roots persisted in config (`~/.config/keyframexl/browser.ron`), same pattern as
  `workspace.ron`. Ship with 2 defaults: project dir + a `Samples` dir (create if
  missing); "Favorites" and "Current project" as virtual roots (Current project =
  patterns/instruments/clips of `Song`, mirroring FL's tab — cheap since it's our own
  data; doubles as the future automation surface per 1.10).
- **Accepted types now:** `.kfxl` (open), `.dwp` (sampler preset, existing
  `import::dwp`), `.wav` (existing `read_wav` via hound), `.mid` later (no MIDI import
  today — show but disabled, or hide behind "Show unknown file types" default-off like
  FL). `.flac/.ogg/.mp3`: **decode missing** (only hound wav today — `src/import.rs:69`);
  show them greyed with "unsupported" tooltip until a decoder lands (P2), do not pretend.

**Scanning/caching.**
- No `notify` in `Cargo.lock` today — add `notify = "8"` (P1 dependency) for
  create/remove/rename events; debounce ~200 ms; manual Refresh button + Ctrl+R always
  (matches FL's Reread).
- Lazy: read one directory level on expand only; never walk the whole tree at startup.
  Cache per-directory listings + mtime in memory; persist tag/star sidecars only
  (`tags.ron` per root — human-editable, no binary index until search demands it).
- Threading: scan on a worker thread, ship `Vec<FileEntry>` over the existing
  `mpsc` pattern (same as `importing` in `src/ui/mod.rs:104`); UI never blocks on IO.
  Cap: skip hidden dirs, skip roots gracefully on permission error (row shows
  "Unreadable" — ui-rules §1.7 error-near-field).

**Filtering.** Bottom search field (FL parity), filename substring first; `*` wildcards
P2; tags P3 (sidecar). Type filter chips: All / Samples / Presets / Projects.
Sort: Name/Type/Date (FL parity minus Group/nfo).

**Preview playback through our engine.**
- New `PreviewBus` in audio layer: `preview(path) / preview_stop() / preview_volume() /
  preview_from(offset_secs)`; stereo, mixed post-song-gain at low fixed priority so it
  never clips the mix; transport Stop halts it (FL parity). Decode: wav sync via hound
  (small files <8 MB fully; larger streamed in chunks — FL's "Fast sample preview"
  rationale, `[GENERAL]`); flac/ogg/mp3 blocked on decoder (P2).
- UI: click row = preview (full by default — deliberately not FL's 5 s trap; show a
  thin progress/length readout instead); Alt+click = also full (same thing, keep the
  binding for FL muscle memory); Shift+click = play from click-x mapped to duration;
  volume mini-slider on hover (FL parity); Esc or second click stops.
- Tempo-sync preview: P3 (needs a stretcher; until then show "sync off" honestly).

**Drag payloads (egui 0.31.1, all verified in vendored source).**
```rust
#[derive(Clone)] enum BrowserDrop { Sample{path: PathBuf}, PresetDwp{path: PathBuf}, Project{path: PathBuf} }
```
- Source rows: `ui.dnd_drag_source(id, payload, add_contents)` (`ui.rs:2772`).
- Targets: `ui.dnd_drop_zone::<BrowserDrop, _>(frame, …)` (`ui.rs:2824`) on: Channel-rack
  channel buttons + empty rack space (→ load sample/preset, Alt = no auto-open window);
  Playlist track-header area (sample → audio clip placeholder / preset → new channel
  clip — audio clips don't exist yet, so P1 = create channel + pattern note, honestly
  labeled); Sampler window (→ load preset, reuses `load_preset`, `instrument.rs:49`);
  desktop/empty dock (project → open, reuses `open_path`, `mod.rs:412`).
- `dnd_release_payload` (`response.rs:456`) returns the drop on release frame; Esc
  ablates (built into `drag_and_drop.rs` begin_pass). While dragging, egui sets the
  `Grabbing` cursor itself.
- OS-file drops keep working through the existing `dropped_files` path
  (`instrument.rs:61`, overlay `mod.rs:637`); internal DnD is additive, no conflict
  (different payload channel: OS = `ctx.input().raw.dropped_files`, internal = egui DnD).

**Layout in our design language** (docs/ui-rules.md): left column stays 208 px
(`BROWSER_W`, `mod.rs:28`); header = caps "BROWSER" + collapse-all + refresh +
add-folder icons (kit `icon_plus`, kit.rs icons §); tabs row = `chips` (kit.rs:279);
tree rows reuse the existing `row()` thumb pattern (`browser.rs:12-30`) with 34 px rows,
mono index/counts, `led` for "loaded in project" state; bottom = search field + type
chips + preview mini-bar (sunken well + seg-text time, kit.rs:443). No panel-in-panel:
tree is flat rows on `SIDE`, preview bar is one sunken strip. Focus ring only after
keyboard nav (`show_focus`, kit.rs:129). All strings follows ui-rules §1.9 tone
(state + next step, no "!").

---

## 2. Knob and slider UX in full

### 2.1 FL behaviour

Verified from the manual:
- Right-click on a control / Current-project parameter gives: **Create automation clip,
  Edit events, Link to controller / Override generic link** (`[BROWSER]` §Current project);
  **Init song with this position / Delete initialization / Delete event** for song-start
  values (`[BROWSER]`, `[GENERAL]` "Initialize controls automatically").
- **Undo knob tweaks** exists but defaults **OFF** for performance (`[GENERAL]` §Undo
  History). FL's undo list itself is browsable per-item (`[BROWSER]` §History).
- Mouse wheel over held notes changes velocity; right-hold + wheel changes tools
  (`[GENERAL]` §GUI Input "Click-and-hold & special gesture functions").
- Hint bar shows the value/description on hover (`[BROWSER]` §Browser Folders hover text;
  toolbar hint-bar convention throughout).

UNVERIFIED against the manual pages fetched (standard FL UI knowledge, treat as
needs-in-app-check): vertical drag is the primary knob gesture; mouse wheel over a
control nudges it; double-click resets to default; right-click menu additionally offers
Type in value / Copy / Paste value; Ctrl+drag (or right-drag) = fine adjust. These are
all long-standing FL behaviors but I could not pin a manual page, so the P-list below
adopts them as *our* spec regardless — they are the cross-DAW consensus (see 2.2).

### 2.2 Ableton / Bitwig / Reaper consensus (and where FL is weak)

- **Ableton**: vertical drag; **Shift+drag = fine increments** (verified: instrument
  reference "Hold Shift while dragging the slider to adjust the start point in fine
  increments", Move manual "Hold Shift … to adjust in fine increments"); **double-click
  resets** (verified: Multiband Dynamics "Double-clicking within the region resets the
  volume to its default"); **Up/Down arrows increment/decrement** focused control
  (verified `[ABLETON-KEYS]` "Adjusting Values"); click value display = type-in.
  Browser-preview sync requires transport running (`[ABLETON-PREVIEW]`) — a wart, FL's
  always-preview is better.
- **Bitwig**: same drag/Shift-fine/double-click-reset/type-in family; browser has a
  first-class **Preview Player with modes** (`[BITWIG-BROWSER]`) — ahead of FL's local
  preview (FL only puts BPM/Key/Sync on Cloud content).
- **Reaper**: Media Explorer preview has **tempo-match off/on/half/double + pitch knob
  with range/behavior options + preserve-pitch toggle** (`[REAPER-UG]` §4.6) — ahead of
  everyone for loop auditioning; mixer/knob behavior is preference-driven
  ("ignore mousewheel on faders" exists because wheel-over-fader accidents are real).
- **FL's weak spots:** (1) no visible fine-adjust affordance documented — Shift-fine is
  undiscoverable; (2) no type-in discoverability (buried in right-click); (3) single
  shared undo toggle defaulting off means knob edits silently escape undo unless the
  user opts in — we should do per-gesture undo always (cheap for us: `checkpoint()`
  clones `Song`; FL's perf worry is plugin-automation scale we don't have).

### 2.3 Faders, step pads, velocity, tempo LCD, number fields

- **Faders** (FL mixer: volume faders, pan/stereo-sep knobs): vertical drag, wheel,
  double-click reset (UNVERIFIED for FL specifically; universal). Touch: faders are the
  accessible alternative to knobs — every knob param must also work as a horizontal
  slider/number for keyboard + screen-reader users (see §4).
- **Step pads / wheel over steps**: FL piano-roll note property = Alt+wheel
  (`[SHORTCUTS]` "Alt/Opt+Mouse-wheel: Change selected note property"); playlist =
  Shift+Alt+wheel nudges clips; channel rack = Shift+wheel moves channel / changes
  pattern (`[SHORTCUTS]`). Rule: **wheel over a stepped control changes the stepped
  value only with its modifier held**, otherwise it scrolls. Our rack pads: wheel =
  nothing (pads are buttons), Shift+wheel = cycle velocity (mirrors FL's velocity-on-
  wheel), document in hint.
- **Note velocity**: FL holds + wheel on notes (`[GENERAL]`); Ableton: Shift+drag on
  velocity markers = fine (`[ABLETON]` editing MIDI). Our velocity lane keeps drag;
  add wheel-over-marker = ±1 (Shift = ±10? no — Shift already = fine elsewhere;
  use plain wheel ±1, no modifier) — decide in P2, keep consistent with knob wheel step.
- **Tempo LCD / number fields**: FL tap-tempo exists (we have Tap, `deck.rs:182`);
  numpad +/- = ±1 BPM is an FL transport convention (`[SHORTCUTS]` pattern selector is
  patterns; numpad +/- BPM is the transport LCD behavior — actually `[SHORTCUTS]`
  Transport lists numpad 0/*/…; +/-1 BPM on the tempo display is standard FL behavior,
  mark UNVERIFIED). Our spec: tempo knob + Tap + type-in field (P2); per-digit LCD
  drag (à la hardware) is anti-pattern — one field, arrows ±1, Shift+arrows ±0.1,
  double-click reset 120. Number fields everywhere else: `DragValue`-style
  (drag ↔ / type ↔ arrows), never custom spinners.

### 2.4 Wheel-over-knob vs scroll-container conflicts (how DAWs resolve it)

The problem: a panel full of knobs inside a scrollable area — does the wheel adjust the
hovered knob or scroll the panel? Resolutions in the wild:
- **Hover-wins (FL/Ableton/Bitwig default)**: wheel over a control adjusts it; wheel over
  gaps scrolls. Cost: accidental tweaks while scrolling a dense panel (Reaper users
  complain enough that Reaper added a preference to ignore wheel on faders).
- **Modifier-gated (Reaper option, many plugins)**: wheel adjusts only with Alt/Ctrl
  held, otherwise always scrolls. Safest for dense panels (our Sampler pages are exactly
  dense knob grids).
- **Focus-gated**: wheel adjusts only the focused control. Rare; bad for mouse users.

**Our rule (P1): hover-wins OUTSIDE scroll containers, modifier-gated INSIDE them.**
Concretely: a knob claims the wheel only if (a) hovered AND (b) no ancestor `ScrollArea`
  would scroll — implement via the mechanism in §3 (zero the consumed delta). Sampler
  pages (dense, will scroll vertically on small windows): require **Ctrl+wheel or plain
  hover-wheel?** Decide: plain hover-wheel adjusts (FL parity, muscle memory), BUT the
  knob must eat the delta so the page doesn't also scroll (§3). Revisit with a setting
  if accidental-tweak reports appear (Reaper precedent). Step pads and buttons never eat
  the wheel.

### 2.5 egui 0.31 scrolling/zoom API (verified in vendored source + our code)

- `InputState::raw_scroll_delta: Vec2` (pub field, unprocessed this frame) and
  `smooth_scroll_delta: Vec2` (pub field, smoothed over frames) —
  `egui-0.31.1/src/input_state/mod.rs:136,150`. `zoom_delta() -> f32` (pinch or
  ctrl-scroll synthesized, `:452`); `zoom_delta_2d()` for axis pinches (`:475`).
- `ScrollArea` **consumes** `smooth_scroll_delta` during its own `show` (reads then
  zeroes the scrolled axes — `scroll_area.rs:958-979`). It never touches
  `raw_scroll_delta`.
- Consequence (the pitfall): our `kit::knob` reads **`raw_scroll_delta`**
  (`kit.rs:361`) — inside any future `ScrollArea` BOTH the knob and the scroller will
  react. And `roll.rs:206` reads `smooth_scroll_delta` directly on a custom (non-
  ScrollArea) grid — correct there, but it never zeroes it, so a parent ScrollArea
  would also scroll. Rule: **whoever acts on the wheel zeroes the axis it consumed**
  (`ctx.input_mut(|i| i.smooth_scroll_delta.x = 0.0)`), exactly like ScrollArea does.
- Touch: pinch arrives as `zoom_delta` + touch `zoom_delta_2d`; two-finger scroll
  arrives as scroll deltas. eframe on Linux/Wayland/X11 forwards what the backend
  gives; trackpad smooth deltas need `smooth_scroll_delta` (not raw) to feel right.

---

## 3. Scrolling and zoom everywhere

FL conventions (verified `[SHORTCUTS]`, `[BROWSER]`):
- Wheel = vertical scroll; **Shift+wheel = horizontal** (browser Quick Zoom is the
  exception: Shift+wheel zooms font); **Ctrl+wheel / Ctrl+right-click-drag = zoom to
  selection or cursor** (roll/playlist); **PgUp/PgDn = zoom in/out**; **middle-drag =
  pan**; Shift+0 centers on playhead; Shift+1..5 zoom presets; scrollbars have zoom
  handles ("scroll+zoom bars" — FL playlist/piano-roll scrollbars include +/- and
  zoom-slider affordances; exact manual page UNVERIFIED, visible in-app).
- Playlist: Shift+wheel reorders tracks when over headers; Shift+Alt+wheel nudges clips.

Our spec (all P1 unless noted):
- Wheel = scroll the hovered axis; Shift+wheel = the other axis; **Ctrl+wheel = zoom
  anchored at cursor** (roll: row height already does this, `roll.rs:204-213`; playlist:
  P2 horizontal zoom; browser tree: font zoom like FL's Quick Zoom, P3).
- Zoom must anchor at cursor: capture pointer pos before changing scale, adjust offset
  so the content point under the cursor stays put. Roll's `top += dy/row` + `row +=
  signum` (`roll.rs:207-212`) does NOT anchor today — listed as a gap (§5/P2).
- Middle-drag pan wherever content overflows (roll grid, playlist); left-drag stays a
  gesture (draw/move), never pan.
- Scrollbars: egui ScrollArea bars are thin; FL-style zoom handles are custom paint —
  P3 (needs custom scrollbar widget; do not fight egui's, paint our own only in roll/
  playlist gutters).
- Keyboard paging: PgUp/PgDn zoom (FL parity), arrows scroll focused view, Home = to
  start. Touch pinch → `zoom_delta()`; two-finger → scroll deltas via smooth channel.

---

## 4. Keyboard and accessibility

### 4.1 Focus order / tab navigation
ui-rules §1.8: everything keyboard-reachable, focus order = screen order, focus ring
1.5px `FOCUS` + 2px gap **only after keyboard nav** (Tab), icon buttons always get
tooltip + accessible name. Already implemented: `track_keyboard_nav` (`kit.rs:111`),
`show_focus` (`kit.rs:129`), `widget_info` on rows/buttons/knobs
(`browser.rs:16`, `kit.rs:243,305,384`). Keep: every new widget (browser rows, preview
bar, sliders) gets `widget_info` + focus ring + arrow-key handling. Knob arrows exist
(`kit.rs:367`) but knob is not focusable-by-Tab today (custom paint + `interact`, no
`focus()` call — gap §5).

### 4.2 FL global shortcuts relevant to us (all verified `[SHORTCUTS]`)

Have today (`mod.rs:476-543`): Space, Ctrl+Space, Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y,
Ctrl+S, Ctrl+O, F5/F6/F7 (+toggle semantics), Alt+1..3, L, P/S in roll, Ctrl+1..3 mute,
Del in playlist.
Missing, adopt P1/P2:
- **F8** plugin picker → our preset/channel picker (P2; no picker exists yet).
- **Alt+F8** browser show/hide (P1 — trivial, high value).
- **F12** close all windows (P1 — trivial; "close" = collapse dock to empty state,
  we already render `empty_workspace`, `mod.rs:619`).
- **F9 / F10 / F11**: mixer / MIDI settings / song info — none exist; reserve, do not
  bind to placeholders.
- **Ctrl+T** typing-keyboard-to-piano toggle (P2 — needs the typing-keyboard feature
  itself; single-letter tool keys P/D/S/T/C/Z/E must then yield when typing-keys is on,
  exactly like FL's NOTE).
- **Ctrl+M** metronome (needs audio metronome; reserve), **R** record (reserve; no
  recording yet — do NOT bind R to anything else), **Ctrl+H** panic/stop-sound (P1:
  kill preview + voices; trivial), **Backspace** toggle Line/None snap in roll
  (P2; we have Step/Beat/Bar — map Backspace = Step↔Bar toggle? FL toggles Line/None;
  ours: cycle snap off/on, P2), **Enter** toggle max playlist (P2),
  **Ctrl+Alt+Z** step-by-step undo vs Ctrl+Z toggle (FL alternates undo/redo on repeat
  Ctrl+Z unless "Alternate undo mode"; ours: Ctrl+Z walks back, Ctrl+Shift+Z/Ctrl+Y
  forward — already saner, keep).
- **Esc**: closes window in FL; in egui Esc aborts DnD automatically
  (`drag_and_drop.rs` begin_pass) — also wire Esc = stop preview + close popup (P1).
- Channel rack: **1..9/0 mute, Ctrl+1..0 solo, Up/Down select, PgUp/PgDn group,
  Ctrl+C/X/V steps, Alt+C clone, Alt+Del delete, Shift+wheel move/reorder,
  Ctrl+Enter focus playlist** — adopt as rack matures (P2; mute keys exist).
- Do NOT take: F1 (help — fine to bind to docs later), F3 (tool selector — adopt when
  tools grow), Tab (egui focus + FL nested-window cycle — keep egui's).

### 4.3 Typing keyboard to piano
FL: Ctrl+T toggles; single-key shortcuts die while on (`[SHORTCUTS]` NOTE). Our P2:
off by default; when on, A–K row plays white keys etc., octave via Alt+numpad
(FL convention, `[SHORTCUTS]` misc). Until then, single-letter shortcuts stay global
inside each window (current behavior, ui-rules §3).

### 4.4 Tooltips / hint-bar conventions
FL: hover = description/value in the hint bar (browser folders, controls). Ours:
`on_hover_text` everywhere (already the norm) + status-bar hint line per view
(`hint()`, `mod.rs:582`) must document the gesture AND its shortcut
(ui-rules §3: "Statusbar-хінт з шорткатом в hover"). Knob hover text already shows
`label value\n hint` (`kit.rs:385`) — extend the pattern: `…\nDouble-click resets •
Shift fine • Ctrl+wheel …` so the control teaches itself.

---

## 5. Audit: our `kit::Knob` and other widgets (file:line gaps)

Reference implementation: `kit::Knob` struct `kit.rs:331-341`, `knob()` `kit.rs:346-420`.
Callers: deck tempo/swing (`deck.rs:137-181`), roll velocity (`roll.rs:~140-160`),
sampler `pknob` incl. log mapping (`sampler.rs:120-149`), envelopes (`sampler.rs:572-596`),
mod-matrix amount uses raw `egui::Slider` (`sampler.rs:498-507`).

| # | Gap | Evidence | Fix in §6/P-list |
|---|---|---|---|
| G1 | Wheel uses `raw_scroll_delta`, never consumes it | `kit.rs:360-366` | Inside any ScrollArea the panel scrolls too (§2.4). Switch to smooth + zero consumed axis. P1 |
| G2 | No Ctrl/Alt fine or coarse modifier on wheel or drag | `kit.rs:352`: Shift only, ×0.2 | Add: Shift = ×0.1 fine (FL/Ableton parity), Ctrl+drag = ×0.02 ultra-fine? Keep two levels max. Wheel step = fixed `per_px*4` ignoring Shift scale for arrows? arrows do scale (§2: wheel/arrow both ×scale — actually arrows use `* scale` too, `kit.rs:371`). Unify: one `step()` fn. P1 |
| G3 | No typed entry anywhere | no TextEdit/parse path for any param | Right-click/double-… menu "Type in value" → small popup field (P2). Double-click stays reset (FL/Ableton parity). |
| G4 | No right-click menu at all | knob `Sense::click_and_drag`, no `context_menu` | Param menu hook: Reset / Type in value / Copy / Paste (P2); automation items reserved (P4). |
| G5 | No Copy/Paste of values | — | `ctx.copy_text` / paste parse (P2, trivial with menu). |
| G6 | Hint text exists but doesn't teach gestures | `kit.rs:385` shows label+value+hint only | Append gesture line (double-click reset etc.) to `hint` at call sites or centrally (P1, trivial). |
| G7 | Undo: sampler envelope knobs NEVER checkpoint | `sampler.rs:543-598`: `out=(e,false)` always; callers pass `false` | Every gesture must open max one undo step. Fix: return real `started` (P1, bug). |
| G8 | Undo granularity inconsistent: `pknob` checkpoints inside, chips checkpoint at call | `sampler.rs:133-135` vs `sampler.rs:279` (`edit_preset(id,true,…)`) | Centralize: gesture-start token from control, single `checkpoint()` in one place (§6). P1 |
| G9 | Knob not Tab-focusable; arrows dead unless focused, focus unreachable | `kit.rs:367-373` needs `has_focus`, but nothing calls `request_focus`/sets focusable | Add `r.request_focus()` on click? Custom widgets need `ui.memory_mut` focus handling; minimum: click focuses so arrows work (P1), full Tab order P2. |
| G10 | `per_px` hand-tuned per call site, log mapping duplicated | `deck.rs:145` (0.5), `sampler.rs:128,141` | Derive from range: `per_px = span/200` already done in sampler; deck hardcodes — unify in Param (§6). P1 |
| G11 | Wheel step ignores range scale sanity: `per_px*4` per tick; high-dpi smooth wheels | `kit.rs:364` uses `signum()` — trackpad smooth deltas collapse to ±1 steps, fast flick = same as slow tick | Accumulate fractional wheel (standard: `acc += delta; steps = acc/step_size`) — §6. P1 |
| G12 | LCD tempo has no drag/wheel/type path | `deck.rs:103-133` paints only | Tempo LCD read-only today; P2: click-drag vertical = tempo, wheel, double-click tap? Keep Tap button. |
| G13 | Mod-matrix amount uses stock egui Slider (style break) + `drag_started` checkpoint but `edit_zones(id,false)` on change — same song-clone-per-frame cost as knobs | `sampler.rs:498-507` | Route through Param path (§6) so it gets gesture accounting + theming; or theme egui style globally. P2 |
| G14 | Roll zoom not cursor-anchored; wheel zoom uses `signum` (trackpads suffer) | `roll.rs:204-213` | Anchor math + fractional accumulation (§3). P2 |
| G15 | No Ctrl+wheel anywhere except roll row-height; no pinch handling | grep §"zoom" | Wire `zoom_delta()` for pinch (P2). |
| G16 | Browser has NO file tree at all | `browser.rs:1-209`: workspace presets, windows, patterns, channels only | The whole §1.12. P1 scaffold → P2 preview/DnD. |
| G17 | Drop overlay knows only `.dwp/.kfxl` | `mod.rs:637-654`, `instrument.rs:11` | Extend filter text as decoders land; internal-DnD overlay uses same strings. P2 |
| G18 | No preview bus; auditioning a sample requires loading it as a preset | `audio.rs` = song render only | `PreviewBus` (§1.12). P1 infra (wav), P2 formats. |

---

## 6. Concrete API proposal: one Param path

Goal: every continuous control (knob, fader, slider, number field) shares value math,
gesture accounting, wheel accumulation, and the future automation hook. One undo step
per gesture, zero song-clones per frame beyond the one checkpoint.

```rust
// kit.rs (new) — all numbers f64 in *display* units; mapping lives here.
pub struct Param<'a> {
    pub label: &'a str,          // "Cutoff"
    pub unit: &'a str,           // "Hz", "dB", "%", "ms", "" — rendered mono after value (ui-rules §2.1.9)
    pub min: f64, pub max: f64, pub default: f64,
    pub curve: Curve,            // Linear | Log | Pow(f64) — replaces P::log bool (sampler.rs:82)
    pub fmt: &'a dyn Fn(f64) -> String,  // value text, no unit inside (unit appended by painter)
    pub hint: &'a str,           // one line, no gestures (gestures appended centrally)
    pub color: Color32,          // address color (ui-rules §2.1.4)
}
pub enum Curve { Linear, Log, Pow(f64) }

pub struct Gesture { pub started: bool, pub changed: bool, pub typed: bool }
// started: exactly once per press/wheel-tick-series/focus-edit → caller checkpoints once.
// changed: value differs → caller applies + publish().
// typed: came from the type-in field → caller checkpoints once (same rule, distinct menu path).

impl Param<'_> {
    pub fn span(&self) -> f64;
    pub fn per_px(&self) -> f64;              // span/200, vertical drag (replaces hand-tuned per_px)
    pub fn wheel_step(&self) -> f64;          // span/100, ×0.1 with Shift
    pub fn clamp(&self, v: f64) -> f64;
    pub fn to_normalized / from_normalized   // for faders/sliders sharing the same value
}

// One interaction core used by knob(), hfader(), vfader(), number():
pub fn param_gesture(ui: &Ui, r: &Response, p: &Param, value: &mut f64) -> Gesture {
    // - drag: value -= drag_delta.y * per_px * fine(Shift ×0.1)
    // - wheel (hovered): acc += smooth_scroll_delta.y (per-Id accumulator in ctx data);
    //   while |acc| >= px_per_step(=8): value += sign * wheel_step * fine; acc -= …;
    //   zero consumed smooth axis via input_mut (ScrollArea coexistence, §2.5).
    // - arrows (focused): ±wheel_step (Shift ×0.1); click focuses (G9).
    // - double-click: default (FL/Ableton parity). Type-in lives in the context menu (G3/G4).
    // - NO checkpoint here: pure function on value. Caller owns undo (current pknob pattern
    //   checkpoints inside AND callers pass started — split brain, G8).
}
```

Call-site shape (replaces `pknob` + ad-hoc knob code):

```rust
let g = kit::knob(ui, &param, &mut v);          // knob() internally calls param_gesture
if g.started { self.checkpoint(); }             // ONE place, every caller, including envelopes (fixes G7)
if g.changed { self.edit_preset(id, |p| …); self.publish(); }
r.context_menu → param_menu(ui, &param, &mut v) // Reset / Type in value / Copy / Paste (P2);
                                                 // automation items call hooks below (P4).
```

Rules:
1. `started` fires once: on `drag_started`, first wheel tick of a tick-series
   (arm when 300 ms without ticks), focus+first-arrow, menu-type confirm. Never per frame.
2. `edit_*` takes no `started: bool` anymore — delete the flag (touches `sampler.rs`
   `edit_preset`/`edit_zones` signatures). undo = `checkpoint()` at gesture start only.
3. Log-mapped params (freq/time/rate) express `curve: Log`, no parallel `x.exp()` math
   at call sites (deletes `sampler.rs:121-136` special case; `to_normalized` handles it).
4. Right-click menu is a kit function so every param gets it free; automation entries
   (`Create automation clip… / Edit events / Link to controller… / Init song with this
   position`) are `Option<AutomationHooks>` — `None` until the automation system exists,
   menu shows them disabled with "needs automation" tooltip rather than hiding (honest UI,
   ui-rules §1.7). P4 wires them to the Current-project browser surface (§1.10).
5. Painters (`knob`, future `fader`, `number`) share: caps label + mono value+unit,
   focus ring via `show_focus`, hover via `hover_amount`, gesture-teaching tooltip line.
   Stock `egui::Slider` (mod-matrix) gets replaced by `kit::slider` on the same core (P2).

---

## 7. Prioritised list with acceptance tests

### P1 — now (browser scaffold + control correctness)

1. **Param core + knob rewrite** (§6 minus menu/type-in). Wheel via `smooth_scroll_delta`
   with per-Id fractional accumulation + axis zeroing; Shift ×0.1 fine; click focuses;
   double-click reset; gesture-teaching tooltip; `per_px`/`wheel_step` derived.
   - *Accept:* hover knob, one wheel notch = span/100; Shift+notch = span/1000; drag 200 px
     = full span; inside a ScrollArea the page does not move while the knob turns;
     trackpad flick scales with distance (no signum steps); double-click = default.
2. **Undo discipline**: `started`→single `checkpoint()` at every caller incl. envelopes
   (fix G7/G8); remove `started` params from `edit_*`.
   - *Accept:* one drag = one Ctrl+Z step back; knob tweak then unrelated edit then
     Ctrl+Z undoes only the edit; envelope drags undoable.
3. **Browser file scaffold**: roots list (persist `browser.ron`), lazy one-level scan on
   worker thread + mpsc, rows with FL-parity context basics (load/open/reveal), bottom
   search (substring), sort Name/Type/Date, add-folder via OS-drop + settings row,
   collapse-all + refresh + Ctrl+R, keyboard (arrows walk, Enter loads, Esc cancels
   search). No preview yet — click selects and shows metadata (size/duration-if-wav).
   - *Accept:* 10k-file dir opens expanded in <100 ms UI time (listing streams in);
     unplug-missing root shows "Unreadable" row, no crash; search filters as you type;
     Enter on `.dwp` loads into focused sampler; Enter on `.kfxl` opens project.
4. **PreviewBus (wav)**: click = full preview, second click/Esc/transport-stop stops,
   hover volume, Shift+click from position, streamed >8 MB. `notify` dep added for later.
   - *Accept:* click wav → sound in <50 ms (small file); 200 MB wav starts instantly
     (streamed); preview + song play simultaneously without dropout; Stop kills both.
5. **Shortcuts**: Alt+F8 browser toggle, F12 close-all, Esc stop-preview/close-popup,
   Ctrl+H panic (preview+voices). Reserve R/Ctrl+M/F9-11 (no placeholder binds).
   - *Accept:* each works from any focused window; status bar documents them.

### P2 — next (parity where it counts)

6. **DnD internal**: `BrowserDrop` payload; sources = browser rows; zones = rack
   (sample→new/existing channel, preset→load, Alt = no auto-open), playlist header
   (honest placeholder behavior), sampler window, empty dock (project open).
   - *Accept:* drag sample → rack channel gets it with name = filename (Auto-name,
     `[GENERAL]` parity); Esc mid-drag aborts; incompatible zone grays out (egui does it,
     verify visually).
7. **Param menu**: Reset / Type in value (popup field, Enter commits = one undo step,
   Esc cancels, invalid = Bad status + no apply) / Copy / Paste (parse-checked).
   Faders/sliders/number field on same core; replace mod-matrix `egui::Slider`;
   tempo LCD draggable + wheel + type-in.
   - *Accept:* type "abc" → error status, value unchanged; paste "12.5 kHz"/"12.5" both
     parse per unit; copy→paste across two knobs transfers value.
8. **Decoders**: flac + ogg (+mp3 if cheap) for preview *and* preset samples; rows
   un-gray; overlay text updated.
   - *Accept:* flac/ogg preview + load into sampler plays correctly; corrupt file →
     "Unreadable sample" status, no panic.
9. **Zoom/scroll**: cursor-anchored Ctrl+wheel zoom in roll (fix G14) + playlist X zoom;
   middle-drag pan; PgUp/PgDn zoom; pinch via `zoom_delta()`; Backspace snap toggle;
   F8 preset picker (needs a picker UI — small); typing-keyboard-to-piano behind Ctrl+T
   with single-key yielding.
   - *Accept:* zoom keeps content under cursor within 2 px; pinch on trackpad zooms;
     Ctrl+T then A plays a note and P no longer switches tool.
10. **Browser depth**: `*` wildcards + in-folder (Shift+F) + F2/F3 walk; Shift+Up/Down
    step-audition into selected channel; Enter-replace semantics (replace vs new);
    middle-click load; duration column for decoded audio.
    - *Accept:* the FL swap-while-playing flow works: select channel, click kick,
      Shift+Down steps with sound each step.

### P3 — later (differentiation + polish)

11. **Tags + saved tabs**: sidecar `tags.ron` per root; Any/All logic; star row control;
    "Save search as tab"; frozen/pinned tabs; colored roots (user folders green, FL parity).
12. **Preview intelligence**: tempo-match off/on (±half/double, Reaper parity) + root-key
    shift for preview; needs stretcher — implement stretch once, reuse for playlist
    audio clips later; BPM/key autodetect display for local files.
13. **Smart collections** (Bitwig parity): saved multi-root queries as virtual tabs.
14. **FL-style zoom scrollbars** in roll/playlist gutters; font Quick-Zoom in browser
    (Shift+wheel); Current-project virtual root (patterns/instruments/clips — the
    automation surface foundation).
15. Light theme pass over all new widgets (ui-rules demands both themes).

### P4 — skip / far future

- Full automation system (clips, event editors, link-to-controller, init values) — the
  Param menu carries disabled hooks (§6.4) so nothing is painted into a corner.
- Mixer + FX slots (the other half of FL's DnD matrix), Edison-style editors, FL Cloud /
  online content, `.mp3` if licensing/decoder cost bites, `XYZ.nfo` group-sort,
  plugin thumbnails DB, multi-touch gesture tuning beyond pinch.
- Per-digit LCD scrubbing, circular knob mode, pencil-mode sensitivity curves
  (`[GENERAL]` tablet options) — no hardware to test against.

---

## Appendix: what was NOT verifiable (do not cite as FL fact)

- Vertical-drag / wheel-nudge / double-click-reset / Ctrl-fine on FL native knobs:
  adopted as our spec on cross-DAW consensus, not on a manual page.
- Arrow-key treewalk + type-ahead in FL browser: adopted as standard behavior.
- ±1 BPM numpad behavior on tempo display; "Type in value / Copy / Paste" menu labels:
  standard FL UI, no manual page located.
- Bitwig/Ableton fine-grained claims beyond the cited pages (Shift-fine, double-click
  reset, preview-sync articles) — verify in-app before quoting in user-facing copy.
