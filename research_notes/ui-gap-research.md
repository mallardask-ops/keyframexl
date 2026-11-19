# INTERFACE GAP ANALYSIS — KeyFrameXL vs FL Studio 25

Research only. No code in `src/` was touched.
Date: 2026-11-15. Repo: `/home/mallar2d/.t3/projects/keyframexl`.

Grounding:

- Shared context: `research_notes/brief-2026-11-15.md`; prior inventory: `research_notes/fl-toolbar-inventory.md`; rules: `docs/ui-rules.md`.
- Our screenshots: `artifacts/*.png` (all 1366×768 except `rack-1180x680.png` at 1180×680; captured with `pixels_per_point = 1.0`, `src/ui/mod.rs:127`).
- User's FL Studio 25 screenshots (`/home/mallar2d/.t3/userdata/attachments/`): full window `…-0c282899-…ca1.png` (1887×1080), detached Playlist `…-82792ee0-…dcdc.png` (1551×1000), detached Piano roll `…-8ff13803-…f9ca.png` (1551×1000).
- Pixel numbers for FL below were measured from those three screenshots with numpy row/column edge detection (method: mean-brightness step between adjacent rows/columns inside a band; strongest steps reported). They are approximate (±2 px) and assume the screenshots are at 100 % UI scale — Image-Line does not print the scale in the screenshot, so treat every absolute FL number as "at whatever scale the user runs" and use the *ratios* as the stable truth. Marked where uncertain.
- Every FL behaviour claim carries its manual URL. Anything I could not verify in the manual is marked UNVERIFIED.

Manual base: `https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/`

---

## 1. Scale and density

### 1.1 How the numbers were obtained

Our side: constants in code (no guessing). FL side: edge detection on the three user screenshots named above, cross-checked visually against enlarged crops (`/tmp/opencode/fl-top.png`, `fl-rack.png` — working files, not committed).

### 1.2 Top chrome (menu + toolbars), FL vs ours

| Element | FL measured (px, at 1887×1080 shot) | Ours (code) | Verdict |
|---|---|---|---|
| Main menu bar (FILE … HELP) | ~26 (strong edge at row 25; text ~12–13 px) | **does not exist** | add; target 26 |
| Transport row (PAT/SONG, Play, tempo LCD, toggles) | ~40–48 (band rows ~28–80) | deck `DECK_H = 60.0` total for *everything* (`src/ui/mod.rs:27`) | split deck into rows; see targets |
| Second toolbar row (window buttons, snap, pattern selector, Shift/Ctrl/Alt, hint bar) | ~28–32 (dark band rows ~84–108) | **does not exist** | add as deck row 2; target 32 |
| Total top chrome | ~100–106 (menu 26 + transport ~46 + row2 ~30) | 60 | grow to ~104–108 at 1080p |

FL toolbar reference: `toolbar_panels.htm`. Menu bar: `menu.htm`.

Our deck packs transport + LCD + encoders + file buttons into one 60 px strip (`src/ui/deck.rs:9-53`). At 1366×768 our deck already overflows into a `File` overflow menu below 260 px (`deck.rs:35-38,56-78`) — that overflow is a symptom: FL never overflows because it has two rows plus an editable toolbar (`toolbar_edit.htm`). The fix is rows, not a narrower menu.

### 1.3 Per-window density

| Element | FL measured | Ours (code) | Ratio / verdict |
|---|---|---|---|
| Channel-rack row pitch | **30 px** (edges at 141/171/201/231/261, pitch exactly 30) | `ROW_H = 50.0` (`rack.rs:15`) | ours is **1.67× too tall** |
| Step pad | ~22×22, pitch ~26 (431→851 px = 420 px for 16 pads incl. 3 group gaps ⇒ ~25–26 pitch), gaps ~4, group gap ~8–10 | `PAD_H = 42.0`, `MIN_PAD_W = 28.0`, `PAD_GAP = 5.0`, `TRAY_GAP = 12.0` (`rack.rs:11-14`) | pads ~1.9× too tall, gaps ~right; trays already group 4-4-4-4 correctly |
| Channel name button | ~110–130 wide × ~24 tall (text ~12 px) | name block `NAME_W = 176.0` wide, label `medium(14.0)` (`rack.rs:10,258`) | ours ~1.5× too wide; font one step too big |
| Per-row pan/vol knobs (FL) | ~16 px mini-knobs + mixer-target selector ~24 px wide | **absent** (no pan/vol/mixer-target per row at all) | missing controls, not just scale |
| Mute LED + channel selector | ~14–16 px LED + number box ~24 px | M/S squares 26×26 (`rack.rs:135-136`) | size OK; behaviour differs (see §4) |
| Playlist track height | **~45 px pitch** in detached Playlist shot (edges 205/250/295 = 45 pitch); main-window tracks look ~30. UNVERIFIED which is the default — manual says track height is user-resizable with per-track dividers and vertical zoom (`playlist.htm`, "Vertical Zoom") | `LANE_H = 56.0`, `HEAD_W = 120.0`, `RULER_H = 26.0`, `MIN_BAR_W = 96.0` (`playlist.rs:9-12`) | ours ~1.25–1.8× too tall; header texts `sans(13)`/`mono(12)` OK |
| Piano-roll key strip width | ~42–50 (right edge of keys at x≈42 in shot S3) | `KEYS_W = 60.0` (`roll.rs:10`) | ours ~1.3× too wide |
| Piano-roll ruler height | ~ ruler band to row ~70 in S2 incl. title+toolbar; grid rows at 100 % are ~12–16 px default (UNVERIFIED exact default) | `RULER_H = 24.0`, rows `ROW_MIN..ROW_MAX = 10..30`, default fit `ROW_MIN..18` (`roll.rs:10-17,199`) | ruler OK; row range OK, keep |
| Piano-roll toolbar icons | ~16–20 px icons in a ~30 px bar | our header is `CONTROL_H = 34` chips + knob (`roll.rs:142-183`) | height compatible; content differs (see §3) |
| Browser tree row | **~23 px pitch** (edges 214/237/260/283/306 = 23 pitch) | `ROW_H = 34.0` (`browser.rs:9`) | ours **1.48× too tall** |
| Browser tabs | small icon+text tabs, text ~11–12 px | chips 34 px + section caps | ours oversized; FL tabs are ~24–28 px |
| UI font | menu/tree ~11–12 px; headers ~12–13 px | Body `sans(14)`, Small `sans(12)`, Mono `mono(13)`, Heading `semibold(20)` (`theme.rs:184-188`); ui-rules wants 20/15/14/13/12 (`docs/ui-rules.md:46-60`) | body text one step too big for dense lists; keep 14 for buttons, use 12–13 in trees/racks |
| Mixer strip width | ~60–70 (column edges cluster per ~65 px; exact pitch UNVERIFIED from this crop) | **no mixer exists** | target 64; see §6 P-list |
| Dock tab bar | ~ (FL uses native-ish ~24–28 window title bars) | tab bar height 32 (`workspace.rs:393`), body margin 14/10 (`workspace.rs:417`) | tab bar OK; body margins eat dense views — reduce to 10/8 in rack/roll |

### 1.4 Concrete targets for 1920×1080 @ 100 %

Scale factor from our 1366 captures to 1920: 1.405. Our constants are in `px at ppp=1`, so the targets below are stated directly in code px (ppp = 1.0) for a 1080p screen. At 1366×768 everything must still fit (scroll, not shrink — FL scrolls racks and zooms playlists rather than shrinking controls; `playlist.htm` "Horizontal Zoom/Timeline Zoom").

| Element | Target @1080p | Our current | Change |
|---|---|---|---|
| Main menu bar | 26 | — | new |
| Deck row 1 (transport + LCD + tempo) | 46 | 60 (all-in-one) | restructure, not just resize |
| Deck row 2 (windows, snap, pattern, modifiers, meters, hint) | 32 | — | new |
| Status bar | 24–28 | 28 (`mod.rs:29`) | keep |
| Rack row pitch | 30 | 50 | −20 |
| Step pad | 22 × 22, pitch 26, gap 4, group gap 10 | 42 h / 28 min-w | shrink; keep 4-4-4-4 trays |
| Rack name button | ~120 × 24, `medium(13)` | 176 wide, `medium(14)` | narrow |
| Playlist track lane | 32 default (user-zoomable 24–64) | 56 | −24; add vertical zoom |
| Playlist header col | 110–120 | 120 | keep |
| Playlist ruler | 24 | 26 | keep/−2 |
| Roll key strip | 48 | 60 | −12 |
| Roll ruler | 24 | 24 | keep |
| Browser row | 24, text `sans(12–13)` | 34 | −10 |
| Browser column width | 200–220 | 208 (`mod.rs:28`) | keep |
| Dock tab bar | 30–32 | 32 | keep |
| Buttons / controls | 34 high (kit rule, `kit.rs:15`) | 34 | keep — matches FL toolbar height language |
| Transport buttons | 44×44 min hit (kit rule `docs/ui-rules.md:66`) | 48×40 (`deck.rs:83`) | make square 44, keep red Play |
| Mixer strip (when built) | 64 wide | — | new |
| Fonts in dense lists | 12–13 | 14 | step down only inside rack/browser/tree rows |

Net effect for the user's complaint ("scale is off"): our rack rows are 67 % too tall, our pads nearly 2× too tall, our browser rows 48 % too tall, and we spend 60 px on a single deck row where FL spends ~104 on menu + two functional rows. The fix is **more rows, smaller cells** — not a global shrink (buttons, tab bar, status bar stay).

### 1.5 UI zoom (FL parity)

FL behaviour: FL Studio follows OS/DPI scaling; toolbar/layout presets are user-editable (`toolbar_panels.htm`, `toolbar_edit.htm`). There is no single in-app "UI zoom %" slider I could verify in the manual — **UNVERIFIED** whether FL 25 adds one; do not claim parity with a specific FL control.

Our proposal (data-model change): a global `ui_scale: f32` (0.8–1.5, default 1.0) in workspace settings, applied as `ctx.set_pixels_per_point(ui_scale)` at startup plus `Ctrl + = / Ctrl + -` live step (0.1) and `Ctrl + 0` reset, persisted in `workspace.ron` next to layout. Acceptance: all hit targets stay ≥24 px grid / ≥44 transport at 1.0 on 1080p; layout scrolls instead of clipping at 1.5. This satisfies "say how they scale" without copying an unverified FL control.

---

## 2. Top of window: menu, transport row, second toolbar row

Manual pages: `menu.htm`, `menu_file.htm`, `menu_edit.htm`, `menu_add.htm`, `menu_patterns.htm`, `menu_view.htm`, `menu_options.htm`, `menu_tools.htm`, `menu_help.htm`, `toolbar_panels.htm` (+ `toolbar_edit.htm`). The full item inventory with shortcuts already exists in `research_notes/fl-toolbar-inventory.md` (§1–8); I do not duplicate all ~150 items here — this section maps them onto **our deck**.

### 2.1 Main menu — what must exist (FL canon)

- FILE (`menu_file.htm`): New, New-from-template, Open `Ctrl+O`, Save `Ctrl+S`, Save-as `Shift+Ctrl+S`, Save-new-version `Ctrl+N`, Import, Export (Wave `Ctrl+R`, mp3/ogg `Shift+Ctrl+R`, MIDI `Shift+Ctrl+M`, stems, ZGame, zipped loop, bones, data files), Recent `Alt+1..0`, Revert-to-backup, Exit.
- EDIT (`menu_edit.htm`): only 4 items in FL — Undo `Ctrl+Z`/`Ctrl+Alt+Z`, Cut `Ctrl+X`, Copy `Ctrl+C`, Paste `Ctrl+V` — target depends on focused window (Rack vs Roll vs Playlist).
- ADD (`menu_add.htm`): Channel, Effect, Plugin-picker, Browse-plugin-database, Refresh-plugins, Manage-plugins, Automation-for-last-tweaked, Pattern.
- PATTERNS (`menu_patterns.htm`): Find-first-empty `Shift+F4`, Find-next-empty `F4`, …no-naming `Ctrl+4`, Select-in-playlist, Rename-and-color `F2`, Random-color, Open-in-browser `Shift+Ctrl+P`, Group/Ungroup, Time-signature, Transpose, Insert-one `Shift+Ctrl+Ins`, Clone `Alt+C`, Delete `Del`, Move-up/down `Shift+Ctrl+Up/Down`, Split-by-channel, Quick-render, Render-as-audio, Render-and-replace.
- VIEW (`menu_view.htm`): Playlist `F5`, Piano-roll `F7`, Channel-rack `F6`, Mixer `F9`, Browser `Alt+F8`, Project-picker `Ctrl+F8`, Plugin-picker `F8`, Tempo-tapper, Touch-controller `Alt+F7`, Script-output, Toolbars-submenu (Edit, Separators, Hint-bar, Flat-buttons, Hide `Ctrl+F11`), Performance-monitor, Welcome, Close-all-windows `F12` family, Arrange-windows, Background, Browser-submenu, Downloads/News/Notifications.
- OPTIONS (`menu_options.htm`): MIDI `F10`, Audio, General, File, Theme, Manage-plugins, Project-info `F11`, Project-general, Debug-log, MIDI-remote/output/sync toggles, Typing-to-piano `Ctrl+T`, Metronome `Ctrl+M`, Precount `Ctrl+P`, Start-on-input `Ctrl+I`, Blend `Ctrl+B`, Step-edit `Ctrl+E`, Loop-record, Auto-scroll `ScrollLock`, Enable-groups, Multilink `Ctrl+J`, Cloud-accounts.
- TOOLS (`menu_tools.htm`): Browser-find `Ctrl+F`, One-click-record, Remix, Riff-machine, Chord-tool, Control-creator, Macros/Panic `Ctrl+H`, FL-Cloud, Clipboard, Score-logger, Audio-logger, Last-tweaked submenu, External-tools.
- HELP (`menu_help.htm`): Index `F1`, Gopher `Alt+F1`, Tutorials, Product/Buy/Unlock/Register/Account, Diagnostics, Graphics-fixes, What's-new, Links, About.

What we have today: **no menu bar at all**. File ops live as three deck buttons (`deck.rs:39-50`) plus a narrow-window `File` popup (`deck.rs:56-78`). Shortcuts exist only for `Ctrl+S`/`Ctrl+O`/`L`/`Space`/`F5-F7`/`P`/`S`/`1-3`/`Z` (`mod.rs:476-543`).

Proposal: a real menu bar (26 px, `egui::MenuBar`) implementing the trees above, **but** — per §7 — with our look (kit raised/sunken, Plex, no native styling). Everything in §2.1 rows 1–8 that has no engine behind it yet (mixer, MIDI, recording, plugins) still gets a menu entry, disabled with a "comes later" hint, so shortcuts and muscle memory are stable from day one. That is the FL-canon-wins point: menu *structure* is layout/behaviour, not decoration.

### 2.2 Transport row (deck row 1)

FL transport (`toolbar_panels.htm#panel_transport` + time/volume/pitch panels): PAT/SONG `L` (right-click PAT→Rack, SONG→Playlist), Play/Pause `Space`/`Ctrl+Space`, Stop `Space` (double-click = panic, right-click = remember-seek), Record `R` (+ filter submenu: Automation/Notes/Audio/Clips, starts-playback, disarm-on-stop, markers, quantize, stepseq, automations), Tempo 10–522 + fine 1/1000 + right-click automation + Tap (+ nudge/sync/mute/now/presets/half-speed), Song-position drag, Time panel Bar:Beat:Step:Tick vs Min:Sec:Cs + S/B switch, Monitor-volume, Master-pitch, window controls.

Ours (`deck.rs`): Play/Pause 48×40 + Stop 48×40 (`80-101`), Pattern/Song chips (`26-29`), LCD 252×46 Bar:Step + BPM (`104-133`), Tempo + Swing knobs + Tap (`135-188`), Open/Save/Export-WAV buttons (`39-50`).

Mapping:

- Keep in row 1: Play, Stop, PAT/SONG, position LCD, tempo + Tap, song-position. Add: **Record (arm, `R`)**, **time-format LCD**, **metronome + precount + blend + step-edit + loop-record** as small toggle icons (FL shortcut-icons panel, `toolbar_panels.htm#panel_shortcuticons`), **monitor volume + peak meter** (FL `panel_vol`/`panel_output`), **MIDI-activity LED**.
- Move out of row 1: Open/Save/Export-WAV buttons → FILE menu (keep `Ctrl+O/S/R` shortcuts; keep at most one overflow `File` button for narrow windows). Swing knob → rack header area next to group filter (it is per-pattern swing; FL shows Global Swing on the rack: `channelrack.htm` §3). Tempo knob stays (FL tempo is transport).
- Missing engine items (record filter, count-in bars, half-speed, nudge, master pitch, time Min:Sec) go to menus first, toolbar when the engine exists.

### 2.3 Second toolbar row (deck row 2)

FL contents (`toolbar_panels.htm` + user screenshot 1): window-toggle buttons (Playlist/Rack/Mixer/Piano-roll/Browser), Global Snap selector (Line/Cell/(none)/Steps/Beats/Bar, Alt = temporary off), Pattern selector with `+`/`−` and right-click list to 999, virtual Shift/Ctrl/Alt modifier latches, hint bar, CPU % + graph + polyphony voices + memory, chord panel, online/communications, Tap-sync/mute etc.

Ours: nothing. Our "windows" live in the left Browser (`browser.rs:77-121`); our snap selector lives *inside* the piano-roll header (`roll.rs:162-167`); pattern switching lives in the Browser pattern list (`browser.rs:127-150`).

Proposal: deck row 2 (32 px) holds, left→right: window toggles `F5/F6/F7/F9/Alt+F8` (icon+key, LED for open — same idiom as `browser.rs:100-107`), Global Snap selector (replaces per-window snap; local snap in roll/playlist becomes "Main + override", exactly FL: `toolbar_panels.htm#panel_globalsnap`), Pattern selector `− [Pattern n ▾] +` with right-click list + `F2/F4` actions, virtual Shift/Ctrl/Alt latch buttons (FL `panel_shortcuticons`; needed for touch/trackpad users and discoverability), flexible hint bar (our status-bar hint text `mod.rs:581-590` moves *up* here live, status bar keeps results), CPU % + voices + memory + MIDI LED (our status right side `mod.rs:574-578` shows only patterns/kHz — extend). Chord panel: skip (P4). Online/news: skip entirely (P4, FL-marketing).

What moves into row 2 from elsewhere: window toggles (from Browser — Browser keeps the list too, FL has both), snap (from roll header; roll keeps a compact mirror), pattern switching (Browser list stays as the full manager). Nothing is deleted, paths stay two-deep max.

### 2.4 What we have that is extra and should go

- Open/Save/Export-WAV as permanent deck buttons (`deck.rs:39-50`): **move to FILE menu** (FL has Save-As `Ctrl+Shift+S` as a blinking toolbar reminder at most). Keep shortcuts. Priority P1 (cheap, big FL-canon win).
- Swing encoder on the deck (`deck.rs:159-180`): **move to rack header** next to All/group filter (FL Global Swing lives on the rack, `channelrack.htm`). Priority P2.
- Velocity encoder inside roll header (`roll.rs:143-160`): keep — FL has per-note velocity via event editor, our lollipop default is a superset idiom; but move to the *event-lane header* when the lane editor grows (P3).
- `Tap` button: keep in row 1 (FL has it), wire the full tap dialog later (P2).
- TE-style micro-legends/serials (`KFXL v0.4 · SN…`): keep exactly one in the status bar, nowhere else (ui-rules §2.1.6 caps).

---

## 3. Per-window header bars

FL rule, verified on all three screenshots and every window page: each window (docked, floating, or detached) carries **the same chrome**: a title bar with (left) window-menu arrow button + breadcrumb title (`Playlist - Arrangement > Pattern 1`, `Piano roll - 808 Kick`, `Channel rack`, `Mixer - Master`, `Browser`), (right) minimize/maximize/close + a Detached toggle state; and directly under it a toolbar row with the window-local tools. The arrow menu is the *primary* command surface — toolbars are shortcuts to it.

Manual pages: Playlist controls/menu `playlist.htm` (+ `playlist_menu.htm` anchors); Piano-roll controls/menu `pianoroll.htm`, `pianoroll_menu.htm`; Channel-rack title bar + options `channelrack.htm` (§1, §3–6, §15–16); Mixer controls/menu `mixer.htm`, `mixer_mixermenu.htm`; Browser controls/menus `browser.htm`.

Ours today: no title bars at all. `egui_dock` draws the tab (`workspace.rs:337-378`) and our `panel_header` draws a caps-context + right-side tools with an edge seam (`mod.rs:603-617`). No arrow menu, no Detached toggle in-tab (only tab right-click Float/Dock, `workspace.rs:369-378`), no breadcrumb beyond `caps` text.

Proposal (uniform, all four windows + instrument/plugin windows): extend `panel_header` into a **window header row**: `[≡/▾ arrow] breadcrumb-title … tools … [detached-pin]`. Arrow opens the full menu tree below; pin toggles Float/Dock in place; tab right-click menu stays as a redundant path. This is layout/behaviour canon (FL wins); the painting (kit raised arrow, caps breadcrumb, pin icon) is ours.

### 3.1 Playlist header (`playlist.htm`)

Toolbar icons (left→right in FL): menu-arrow, then Draw `P`, Paint `B`, Delete `D`, Mute `T`, Slip `S`, Slice `C`, Select `E`, Zoom `Z`, Playback `Y`, Play/Pause; plus Clip-focus tabs (Audio/Automation/Pattern), Arrangements menu, Clip-source menu + Picker-panel toggle, time-ruler tools, track mute/solo in headers.

Arrow-menu tree (from user screenshot 2 + `playlist.htm#Playlist_Menu`): Edit ▸, Tools ▸, View ▸, Snap ▸, Select ▸, Group ▸, Zoom ▸, Time markers ▸, Picker panel ▸, Current clip source ▸, Edit patterns in ▸, Performance mode `Ctrl+P`, Quantize song jumps ▸, Center `Shift+0`, Detached.

- We have: Add-clip / Make-unique / Delete buttons (`playlist.rs:55-77`), track headers with no mute LED, no ruler tools, double-click opens **Rack** (`playlist.rs:263-267` — FL default opens Piano roll; `Alt+double-click` opens Rack; `Edit patterns in` switches it: `playlist.htm`).
- Add (P1): arrow menu with the tree above (disabled items where no engine yet), Draw/Paint/Delete/Mute/Slip/Slice/Select/Zoom tool strip, ruler with bar numbers + timeline selection (`Ctrl+drag`), track mute/solo LEDs, clip focus tabs, clip menus per clip (`playlist.htm#ClipMenus`), arrangements menu (stub, one arrangement), Detached pin.
- Behaviour fixes: double-click → open Piano roll for that clip's pattern (FL default); `Alt+double-click` → Rack; right-click clip → clip menu (Make-unique `Ctrl+Shift+V`, Delete, Open-in-roll/rack, clip color/name later).

### 3.2 Piano-roll header (`pianoroll.htm`, `pianoroll_menu.htm`)

Toolbar icons: menu-arrow/wrench (Tools), magnet (snap; right-click = default note length), note (snap-to-scale; right-click = scale/root), stamp (chords/scales/percussion/special), Draw `P`, Paint `B`, Paint-drum `N`, Delete `D`, Mute `T`, Interpolate `I`, Slice `C`, Select `E`, Zoom `Z`, Playback `Y`, Play; Target-channel dropdown, Target-control (event editor) dropdown, slide `S` / porta `O` toggles, note-color selector, ghost-notes panel toggle.

Arrow-menu tree (user screenshot 3 + `pianoroll_menu.htm`): File ▸ (Open-score, Save-score-as, Import-MIDI `Ctrl+M`, Export-MIDI `Shift+Ctrl+M`, MIDI-clipboard, Score-sheet), Edit ▸ (Cut/Copy/Paste `Ctrl+X/C/V`, Duplicate `Ctrl+B`, Delete `Del`, Shift/Rotate, Transpose `Shift+↑/↓`, octave `Ctrl+↑/↓`, Discard-lengths `Shift+D`, Allow-resize-from-left, Change-color `Alt+C`, Mute, Insert/Delete-space, Turn-into-automation), Tools ▸ (Riff `Alt+E`, Chord-progression `Alt+P`, Legato `Ctrl+L`, Articulate `Alt+L`, Quick-quantize `Ctrl+Q`, Quantize `Alt+Q`, Chop `Ctrl+U`/`Alt+U`, Glue `Ctrl+G`, Arpeggiate `Alt+A`, Strum `Alt+S`, Flam `Alt+F`, Claw `Alt+W`, Limit `Alt+K`, Flip `Alt+Y`, Randomize `Alt+R`, Scale-levels `Alt+X`, LFO `Alt+O`, Scripts, Stamp), Stamp ▸, View ▸ (grid color/contrast/invert, time-segments, keep-labels, shadow, rounded, note-colors, incremental-scroll, time-indicator, mini-preview, note-grid-highlights `Alt+B`, scale-highlighting, note-length-in-properties, ghost-channels `Alt+V`, editable-ghosts `Ctrl+Alt+V`, background-waveform `Alt+N`, keyboard-style/labels, swap-panels, flip-separator `Shift+Enter`), Snap ▸ (Main/Line/Cell/(none)/Steps/Beats/Bar/Events/Markers), Select ▸, Group ▸ (`Shift+G`/`Alt+G`), Zoom ▸, Time-markers ▸, Target-channel ▸, Target-control ▸, Auto-smoothing, Preview-notes-during-playback, Center `Shift+0`, Detached.

- We have: Velocity knob + Snap chips (Step/Beat/Bar) + Draw/Slide chips + instrument combobox (`roll.rs:142-183`); ghost rendering (solid only, `roll.rs:311-319`); velocity lollipops (`roll.rs:571-616`); Alt-bypass (`roll.rs:436`).
- Add (P1): arrow menu (File/Edit basics live; Tools/Stamp submenus present but mostly disabled until engine allows), full Snap list incl. Main/Line/Cell/(none) wired to the global selector, Target-channel dropdown (= our instrument combobox, keep, add tick marks for channels with notes + `Ctrl+wheel` to show used/unused per `pianoroll.htm`), Target-control dropdown (Velocity/Pan/… — our lane gets a header with this), slide/porta/note-color segmented control (we have Draw/Slide only), ghost toggle `Alt+V` + editable-ghosts later, keyboard styles later (P3), Detached pin.
- Missing P1 behaviours: `Ctrl+Q` quick-quantize (trivial: snap starts to grid), `Ctrl+B` duplicate, `Del` delete selection, `Shift+D` discard-lengths, `Alt+C` change-color (needs color groups first — P2), insert/delete-space, double-click-note properties dialog (P2+), `Shift+Enter` lane toggle (we auto-hide by height — add manual toggle too).

### 3.3 Channel-rack header (`channelrack.htm`)

Title bar (left→right): options-menu arrow, Loop-Starter icon, group/display-filter dropdown ("All"), Global Swing knob, Pattern-length control, Loop controls, step/piano-roll view toggle, Graph-editor toggle `Ctrl+K`, mute/solo display, "Step sequencer" caption, channel-count, `+` add (`channelrack.htm` overview + §1–6, §15–16).

Arrow(=options)-menu tree (full, from `channelrack.htm`): Add-one ▸, Loop-Starter, Select-unused-channels, Clone-selected `Alt+C`, Delete-selected `Alt+Del`, Move-selected-up/down `Alt+↑/↓`, Sort-by ▸ (Color/Name/Track/Type), Group-selected `Alt+G`, Color-selected ▸ (incl. Gradient/Random), Mute/Unmute-selected, Transpose-selected, Set-swingmix-for-selected ▸, Set-truncate-swing ▸, Assign-to-free-mixer `Ctrl+L`, Change-declicking ▸, Zip-selected `Alt+Z` / Unzip-all `Alt+U`, Restretch-all `Ctrl+Alt+R`, Group-playlist-tracks-option, Auto-resize ▸, Auto-switch-display-filter, Channel-button-width ▸, Channel-name-alignment ▸, Focus-selected-on-playback, Mute-removed-steps, Show-mixer-track-selectors `Alt+M`, Show-complete-piano-roll-preview, Show-mute-button-on-preview, Detached, Switch-audio-clip-channels ▸.

- We have: pattern caps + beat captions (`rack.rs:106-115`), playhead LEDs (`116-120`), `+ Add channel` button (`228-242`), per-row M/S + name + pads + sampler strips.
- Add (P1): arrow menu (live: Add, Select-unused, Clone `Alt+C`, Delete `Alt+Del`, Move `Alt+↑/↓`, Mute/Unmute, Transpose, Assign `Ctrl+L` stub, Zip/Unzip, Mute-removed-steps toggle, Show-mixer-selectors `Alt+M` toggle, Detached pin), group-filter dropdown (All + user groups; groups themselves P2), swing knob (moved from deck), pattern-length control, view toggle steps/preview, graph-editor toggle `Ctrl+K` (opens lane under rows — the P1 editor is velocity-only; full Note/Release/Pitch/ModX/ModY/Shift/Rep per `channelrack.htm` §6 is P2).
- Keep ours: 4-4-4-4 trays + TE legends (look), LED row (behaviour = FL playhead), M/S squares (size OK).

### 3.4 Mixer header (`mixer.htm`, `mixer_mixermenu.htm`)

Title bar: menu-arrow, multitouch toggle, meter-wave view `Alt+W`, extra-properties toggle, inspector/FX-panel toggle, `+` add-track; right inspector: input/output selectors, 10 FX slots (Replace/Delete/Patcherize/Presets/Browse/Save-as/Favorite/Spare-state/Link-all/Browse-params/Threading/Smart-disable/Move/Help), EQ graph, output routing; strip: name (vertical), LED, pan, stereo-separation, fader, peak meters, route arrows, record arm.

We have no mixer. The header spec above becomes the build spec (P2, see §6). Note FL's "impossible to clip insert tracks, only Master clips" (`mixer_levelsandmixing.htm`, via `mixer.htm`) — behaviour canon for our future meters.

### 3.5 Browser header (`browser.htm`)

Header: menu-arrow (Search opts, Navigation-history, Actions, View incl. Auto-hide/Hide-while-dragging/Auto-contrast/Icons-text/Full-preview, Structure Collapse/Reread, Content extra-folders/Refresh), Collapse-structure, Refresh `Ctrl+R`, Minimize, Close `Alt+F8`, Search `Ctrl+F` + F3 next; tabs with right-click menu (Rename/color/icon, Hide/Show-hidden/Delete/Clone/Move/Search-as-tab, View zoom/columns/samples/plugins, Sort, Layout); bottom TAGS + search field.

We have: WORKSPACE/WINDOWS/PATTERNS/CHANNELS sections (`browser.rs:55-205`), no tabs, no collapse/refresh, no search, no preview, search-at-bottom missing. Layout note (this agent only): tabs row on top (`All | Current project | Plugin database | Sounds | Starred`), tree under it, TAGS + search pinned at bottom (matches user screenshot 1). File-tree depth and knob-UX are the other agent's scope.

### 3.6 Plugin windows (our Instrument window)

FL channel-settings/plugin windows carry the same title-bar idiom (name, preset arrows, `+` favorite, gear menu, Detached). Our instrument header (`instrument.rs:100-113`) has Piano-roll + Load-preset buttons — add arrow menu (Piano-roll, Load/Save-preset, Rename/color/icon `F2`, Cut/Copy/Paste, Delete-channel) + pin. Preset format list stays (`PRESET_EXTS`, `instrument.rs:11`).

---

## 4. Channel rack in full

Reference: `channelrack.htm` (esp. §§7–16). Numbers in this section are at the screenshot's scale (§1.1 caveat).

### 4.1 Per-row controls: FL vs `rack.rs`

| Control | FL (`channelrack.htm`) | Ours (`rack.rs`) |
|---|---|---|
| Mute LED | LED on/off, LMB; solo via `Ctrl+click`/`Alt+click`/`Right-click` menu; group mute `Alt+click`; lock `Shift+click` (§12) | M/S 26 px squares, LMB only; solo separate button (`20-40, 135-148`); `Ctrl+1..0` mutes (`mod.rs:530-536`) | keep squares (look), adopt FL gestures: RMB = solo, `Ctrl+LMB` = solo, `Alt+LMB` = group, `Shift+LMB` = lock |
| Pan knob | mini-knob pre-mixer, sent to plugin (§7) | absent | add mini-encoder 16–20 px (needs per-channel pan in song model) |
| Volume knob | mini-knob, samplers default 78 % (−5.2 dB) headroom (§8) | absent | add; default 0.78 for new sampler channels |
| Mixer-target number | routing selector, drag vertical, `Alt+M` show/hide; `Ctrl+L` assign (§14) | absent (address string `drm ch NN` only, `149-156`) | add selector (stub → Master until mixer exists) + `Alt+M`, `Ctrl+L` |
| Name button | opens channel settings; `Ctrl+click` = preview; RMB menu: Piano-roll, Graph-editor, Rename/recolor/icon, Load-sample, Cut-self, Show-waveform, Insert, Replace, Patcherize, Clone, Delete, Separate, Cut/Copy/Paste, Fill 2/4/8, Advanced-fill, Rotate `Shift+Ctrl+←/→`, MIDI-through, Receive-notes-from, Create-DirectWave, Burn-MIDI (§9) | click → Instrument window; RMB: Instrument-window, Piano-roll, Load-preset, Delete (`251-290`) | extend RMB toward FL list as engine allows; add `Ctrl+click` preview now (P1, trivial) |
| Channel selector / activity LED | outer border select + inner activity; click focus, RMB multi, double-click all, `Alt+↑/↓` move (§10) | none (selection = pattern list / focus only) | add selector column (P1): click/RMB-multi/double-click-all + `Ctrl+C/X/V` between channels/patterns, `Alt+C` clone, `Alt+Del` delete, `Alt+↑/↓` move |
| Graph editor | `Ctrl+K` lane: Note/Velocity/Release/Fine-pitch/Pan/ModX/ModY/Shift/Rep; bars drag, `Ctrl`-scale-all, RMB-drag ramp, `Alt`-reset (§6) | none | P1: velocity lane under rows (reuse roll lollipops); full targets P2 |
| Keyboard view | overlaid step↔piano toggle per channel (§6 "Step vs Piano roll Preview") | sampler note-strip preview (`208-225, 333-372`) | keep strip; add per-row mini-preview for drum rows with notes (P2) |
| Piano-roll preview | mini note preview; LMB show/hide, RMB open-at-click; "show complete preview" + "mute button on preview" options (§11) | sampler strip click → opens roll (`221-223`) | add RMB-open-at-click; mute-on-preview toggle (P2) |
| Swing knob | global multiplier 0–100 % × per-channel swing (§3) | deck Swing 0–60 % (`deck.rs:159-180`) | move to rack header; model per-channel swing mix later (P2) |
| Step count / pattern length | 1–512 steps (Auto or fixed), drag bottom-right to expose; right-click = bars; default from Project settings (§4) | fixed 16 (`STEPS`, `sequence.rs`) | P1: keep 16 but add length control stub; P2: variable length (song-model change) |
| Group filter | "All" + user groups, `PageUp/Down` cycle (§"Channel Filter Groups") | none | P1: "All" dropdown (single group); P2: real groups + `Alt+G` |
| `+` add | opens Add-one favorites menu (§15) | `Add channel` button adds a sampler (`228-242`, `instrument.rs:14-22`) | keep; grow RMB/list into Add-one menu as instruments arrive |

### 4.2 Selection rules (FL canon to adopt verbatim)

Click selector = select+focus; RMB selector = independent toggle; drag-down RMB = multi; double-click any selector = all (`channelrack.htm` §10). `Ctrl+C/X/V` steps between channels/patterns (`channelrack.htm` §13; EDIT menu `menu_edit.htm`). Our rack has no selector column and no step clipboard — both P1 (clipboard is `Song`-level copy of `[u8;16]` + undo checkpoint, no model migration).

### 4.3 `Ctrl+click` and friends (full gesture map)

Rack/pads: LMB step on/off; RMB off (we have: `198-203`); FL additionally: `Use both mouse buttons` option inverts this (`channelrack.htm` §13) — add as setting (P3). `Shift+click` ours cycles velocity (`173-186`) — FL has no such gesture (velocity lives in Graph editor); keep ours but document as intentional deviation (faster than FL for drums; does not conflict). Piano-roll preview `Ctrl+click` = preview sound without opening (`channelrack.htm` §9) — adopt P1. Channel-button `Alt+click` = open without closing others (`channelrack.htm` §9) — adopt when multi-instrument-windows exist (P2).

---

## 5. Browser: layout only

Reference: `browser.htm`. (File-tree mechanics, preview player, and knob UX are the other agent's scope — here only the layout contract.)

FL layout top→bottom: tab strip (`All | Current project | Plugin database | Online content/Sounds | Starred`, user-editable, right-click tab menu), tree with folder icons + favorite stars, inline waveform previews, preview player at bottom when audio selected, TAGS toggle + Type/Any-All filter + search field pinned at the very bottom, divider drag with snap-to-second-position.

Ours (`browser.rs`): WORKSPACE chips + WINDOWS list + PATTERNS + CHANNELS, rows 34 px, no tabs, no search, no tags, no preview, no collapse/refresh.

Layout deltas (no behaviour detail): add tab strip (~26 px tabs, horizontally scrollable) above the tree; tree rows 24 px with 16 px folder icons + star hit-zone on hover; bottom-anchored search block (~64 px: TAGS toggle + field + Type filter). Keep our sections as the content of an `All`-equivalent tab; add `Current project` (patterns/automation/undo-history per `browser.htm#Browser_CurrentProject`) when the model exposes it (P2). Drag targets: tree rows must be drag sources onto rack/playlist/mixer (P2, needs egui drag payload — note in model changes). Favourites: star toggles + Starred tab (P2, needs a `favorites` set in project or config).

---

## 6. Master table: missing / extra / wrong → action

`→` action ∈ {add, change, remove}; priority per §1–5 reasoning. File:line anchors are to the current tree.

| # | Element | FL (manual) | Ours (file:line) | Action | Pri |
|---|---|---|---|---|---|
| 1 | Main menu bar FILE…HELP | `menu.htm` + `menu_*.htm` | absent; `File` overflow popup only (`deck.rs:56-78`) | add menu bar 26 px, full trees, disabled-until-engine items | P1 |
| 2 | Deck row 2 (windows/snap/pattern/modifiers/meters/hint) | `toolbar_panels.htm` (snap, pattern, hint, cpu, chords, online) | absent | add 32 px row; relocate snap/pattern/windows there | P1 |
| 3 | Global Snap (Line/Cell/(none)/Steps/Beats/Bar + Alt-off) | `toolbar_panels.htm#panel_globalsnap`, `pianoroll_menu.htm#PianoRoll_Snap` | `Snap::{Step,Beat,Bar}` (`mod.rs:46-62`); local-only | change: add Main/Line/Cell/(none)/Steps/Beats, global store, Alt-bypass (roll already has Alt: `roll.rs:436`; playlist lacks it) | P1 |
| 4 | Record arm + filter + metronome/precount/blend/stepedit/loop | `toolbar_panels.htm#panel_transport`, `menu_options.htm` (`Ctrl+M/P/I/B/E`), shortcut-icons | absent; no `R` key | add buttons + menu items (armed state in song model; engine later) | P1 menu+arm; engine P2 |
| 5 | Per-window arrow menus + pin + breadcrumb | `playlist.htm`, `pianoroll_menu.htm`, `channelrack.htm` §1, `mixer_mixermenu.htm`, `browser.htm#Browser_Menu` | `panel_header` caps+tools only (`mod.rs:603-617`); Float/Dock only in tab menu (`workspace.rs:369-378`) | change: extend `panel_header` with `[▾][crumb … tools][pin]` in all 4 windows + instrument | P1 |
| 6 | Rack row pitch 30 / pads 22 / name 120×24 | `channelrack.htm` (measured §1.3) | `ROW_H 50 / PAD_H 42 / NAME_W 176` (`rack.rs:10-17`) | change constants + fonts to §1.4 targets | P1 |
| 7 | Pan + volume + mixer-target per row | `channelrack.htm` §§7–8,14 | absent (`rack.rs`) | add (model: `pan/volume/mixer_target` per instrument) | P1 UI+model; mixer engine P2 |
| 8 | Channel selector column + step clipboard | `channelrack.htm` §§10,13; `menu_edit.htm` | absent | add selector + `Ctrl+C/X/V`, `Alt+C`, `Alt+Del`, `Alt+↑/↓` | P1 |
| 9 | Swing on rack + pattern length + group filter "All" | `channelrack.htm` §§3–4 + filter groups | swing on deck (`deck.rs:159-180`); fixed 16; no filter | change: move swing; add length + filter stubs | P1 stubs; variable length P2 |
| 10 | Graph editor (velocity lane first) | `channelrack.htm` §6 (`Ctrl+K`) | absent | add velocity lane; full targets P2 | P1 lane; P2 full |
| 11 | Playlist tool strip (Draw/Paint/Delete/Mute/Slip/Slice/Select/Zoom) + ruler tools | `playlist.htm` controls | Add/Make-unique/Delete only (`playlist.rs:55-77`) | add strip; keep our three as clip ops in arrow menu | P1 |
| 12 | Playlist double-click→Roll, Alt+double-click→Rack; clip menus; track mute/solo; arrangements stub | `playlist.htm` (Clip Menus, Track Header menu, Arrangements) | double-click→Rack (`playlist.rs:263-267`); clip RMB menu 4 items (`233-250`); no track LEDs | change gestures; add LEDs + clip menu items + arrangements stub | P1 |
| 13 | Roll: full snap list, Target-channel w/ ticks, Target-control lane header, slide+porta+colors, ghosts `Alt+V`, `Ctrl+Q/B`, `Del`, `Shift+D`, `Shift+Enter` | `pianoroll.htm`, `pianoroll_menu.htm` | partial (`roll.rs` — see §3.2) | add controls + shortcuts; color groups P2 | P1 except colors P2 |
| 14 | Browser tabs + 24 px rows + bottom search/TAGS + collapse/refresh | `browser.htm` | 34 px rows, sections only (`browser.rs:9,55-205`) | change layout per §5 | P1 layout; search/favs/drag P2 |
| 15 | Pattern selector `−/+`, right-click list, `F2/F4/Alt+C/Del`, rename/color | `toolbar_panels.htm#panel_pattern`, `menu_patterns.htm` | Browser list + `new_pattern` only (`mod.rs:299-314`) | add selector in deck row 2; pattern rename/color (model: `name+color`) | P1 selector; color P2 |
| 16 | Open/Save/Export as deck buttons | FILE menu + `Ctrl+Shift+S` reminder (`menu_file.htm`, shortcut-icons) | permanent buttons (`deck.rs:39-50`) | remove from deck → FILE menu | P1 |
| 17 | Monitor volume + peak meter + CPU %/voices/memory + MIDI LED | `panel_vol`, `panel_output`, `panel_cpu`, hint/sync (`toolbar_panels.htm`) | status text only (`mod.rs:574-578`) | add to deck row 1/2 | P1 (meters read from audio backend when present) |
| 18 | Virtual Shift/Ctrl/Alt latches + full hint bar | shortcut-icons (`toolbar_panels.htm`) | hints only in status bar (`mod.rs:581-590`) | add latches + move live hint to row 2 | P2 |
| 19 | Mixer (strips, inspector, 10 FX slots, routing, EQ) | `mixer.htm`, `mixer_mixermenu.htm` | absent | add (biggest model+engine item) | P2 |
| 20 | Undo history in Browser + unlimited Ctrl+Z scope | `browser.htm#Browser_CurrentProject`, `menu_edit.htm` | 100-step song snapshots (`mod.rs:223-229`) | change: expose history list; keep snapshot approach | P2 |
| 21 | Time Min:Sec + S/B, song-start memory, half-speed, nudge | `panel_time`, `panel_transport` | Bar:Step LCD only (`deck.rs:104-133`) | add formats + behaviours | P2 |
| 22 | Typing-to-piano `Ctrl+T`, typing-keyboard toolbar | `menu_options.htm`, shortcut-icons | absent | add (needs key-routing in roll + audio) | P2 |
| 23 | Plugin picker `F8` + project picker `Ctrl+F8` + Add-one menus | `browser.htm#Plugin_Picker`, `menu_add.htm`, `channelrack.htm` §15 | `Add channel` sampler-only | add pickers when instruments exist | P2 |
| 24 | Riff machine, chord tool, stamp chords, scale snap, autos | `pianoroll_menu.htm` Tools/Stamp, `pianoroll.htm` scales | stamp absent; scale absent | add | P3 |
| 25 | Slip/slice-advanced, stretch handle, note-props dialog, key labels/styles, waveform view, time-sig per pattern | `pianoroll.htm`, `pianoroll_menu.htm` | absent | add | P3 |
| 26 | Playlist: slip/slice tools, timeline select, loop-record, consolidate/stems, track groups/modes, performance mode | `playlist.htm` | drag-move + add/delete only | add | P3 (consolidate needs render path) |
| 27 | Rack: per-channel swing mix, zip, sort, color/gradient, advanced looping, loop-starter, MIDI-through/receive, burn-MIDI, advanced-fill | `channelrack.htm` | absent | add | P3 |
| 28 | Browser: preview player, inline waveforms, indexing, extra folders, sounds/cloud, scales/columns views | `browser.htm` | absent | add (with file-tree agent's design) | P3 |
| 29 | FL Cloud, shop, news/RSS, Gopher, diagnostics, video export, Edison/macros specifics | `menu_tools.htm`, `menu_help.htm`, `panel_online` | absent | skip; replace licensing with our own scheme when needed | P4 |
| 30 | TE look details (trays, legends, LCD, encoders, caps) + BMS kit chrome | `docs/ui-rules.md` §§1–2 | `kit.rs`, `theme.rs`, deck/rack/roll painting | keep; do not "FL-skin" them (see §7) | — (guardrail) |

---

## 7. Where FL canon wins vs where our look wins

Rule from `docs/ui-rules.md` §0–3: BMS kit = mandatory base, TE = vibe adapted to dark, FL = canon for layout/behaviour/shortcuts. Applying it to this analysis:

**FL canon wins (copy layout, behaviour, shortcuts exactly):** menu structure + all shortcuts in §2.1; two-row deck + hint bar + meters; per-window arrow menus, pin/Detached, breadcrumb titles; rack row contents and every gesture in §4.3 (including `Ctrl+click` preview, RMB-erase-everywhere, Alt-bypass-snap, `F5/F6/F7/F9`, `L`, `R`, `Ctrl+T/M/P/I/B/E/J`, `F2/F4`, `Alt+C`, `Shift+G`, `Ctrl+Q/B`, `Shift+D`, `Shift+0`, `Shift+Enter`); double-click/Alt+double-click clip semantics; step clipboard; selector rules. Rationale: muscle memory transfers from FL; deviating here is pure cost.

**Our look wins (keep colour, chrome, typography):** kit raised/sunken with top-light + 2 px lip (`kit.rs:51-100`); TE encoders with address colours (tempo blue, swing green — already ours, `deck.rs:135-180`); 7-seg LCD with ghost segments (`kit.rs:438-510`); caps micro-legends + tabular numerals; brand-red reserved for Play/armed (`theme.rs:38-46`, ui-rules §1.2); Plex only, no FL's small Tahoma-ish UI font; dock tab chrome (`workspace.rs:382-427`); trays-as-hardware-keys rack styling. Rationale: ui-rules §§1–2 are explicit, and none of the user's complaints are about colour/chrome — they are about scale, rows, and missing window menus.

**Deliberate deviations to keep (document, don't "fix"):** `Shift+click` pad velocity-cycle (`rack.rs:173-186`) — faster than FL's graph editor for drums; sampler note-strip under drum rows (`rack.rs:208-225`) — FL shows piano-roll previews but not a full strip; velocity knob in roll header (`roll.rs:143-160`) — FL uses the lane; `×N` shared-data badge (`playlist.rs:196-198`) — clearer than FL's implicit sharing. Each must be one line in the future UI spec so nobody "aligns" them away.

**Never copy from FL:** flat grey menu styling, tiny 16 px knobs without labels/hit-zones (ours keep ≥24 px hit + hover value in hint bar), marketing surfaces (news/shop/Gopher), modal dialogs where FL uses them (rename `F2` should be inline like our channel rename, `instrument.rs:117-132`).

---

## 8. Prioritised list with acceptance criteria

Conventions: "engineer can test" = concrete steps + observable result. "Model/API change" = what changes in `Song`/song structs, `App`, or new modules. Undo policy: every mutation checkpoints (`mod.rs:223-229` pattern).

### P1 — now (usable FL-scale sequencer; no engine work except where noted)

**P1-1. Real menu bar (26 px) with full trees.** Acceptance: `Alt+F` opens FILE; every item in §2.1 renders; items without engine backing are disabled with hint "comes later"; `Ctrl+O/S`, `Shift+Ctrl+S` (stub → same as Save until Save-As path exists), `Ctrl+Z/X/C/V`, `F1` (opens manual URL in browser), `Alt+F4`/Exit path work. Model: none (UI only) + `menu.rs` module; shortcuts table moves out of `mod.rs:476-543` into it.

**P1-2. Deck row 2 (32 px): windows + Global Snap + pattern selector + hint + meters.** Acceptance: at 1920×1080 both rows visible without overflow; `F5/F6/F7/F9/Alt+F8` toggle with open-LEDs; snap changes move notes/clips in roll *and* playlist (single `App.snap` store replaces `mod.rs:95` enum — extend enum with Main/Line/Cell/None/steps/beats first); pattern `−/+` + right-click list + `F2` rename-inline + `F4` next-empty; live hint mirrors hovered control (move `mod.rs:581-590` text up; status keeps results); CPU/voices/memory/MIDI-LED render static-but-live values from audio backend or "—" with `startup` tooltip when no audio. Model: `Snap` enum extension; `pattern.color: u32` + `rename_pattern()` (needed by P1-7 too).

**P1-3. File buttons off the deck.** Acceptance: Open/Save/Export-WAV gone from deck row 1 at all widths; FILE menu owns them; shortcuts unchanged; narrow-window `File` popup (`deck.rs:56-78`) deleted. Model: none.

**P1-4. Rack density to §1.4 targets.** Acceptance: screenshot at 1366×768 shows 6+ drum rows fully without scroll where today 3 fit; pads 22 px min, rows 30 px pitch (`rack.rs:10-17` constants + `NAME_W` 176→120, label font 14→13); pads never narrower than 22 (scroll instead). Model: none (constants + layout only). Guardrail: trays, LEDs, M/S squares stay.

**P1-5. Rack header row: arrow menu + swing + length stub + "All" filter + pin.** Acceptance: arrow opens menu with §3.3 tree (live: Add-one, Select-unused, Clone `Alt+C`, Delete `Alt+Del`, Move `Alt+↑/↓`, Mute/Unmute, Transpose-dialog, Zip/Unzip, Mute-removed-steps toggle, Show-mixer-selectors `Alt+M` toggle, Detached pin toggles float); swing knob moved from deck and still writes `song.swing`; length control shows "16" (editing stubbed until P2-3); filter shows "All" (dropdown present, one entry). Model: none yet (stubs), except swing stays `song.swing`.

**P1-6. Per-row pan/vol/mixer-target (UI + model, engine later).** Acceptance: each drum row shows 16–20 px pan + vol encoders and a mixer-target readout; drag edits values with undo; hover shows value in hint bar; audio ignores them for now (documented "not yet wired" hint). Model: `Instrument` gains `pan: f32, volume: f32 (default 0.78 for samplers per `channelrack.htm` §8), mixer_target: u16`; `Ctrl+L` assigns selected to next free (stub → Master=0 until mixer).

**P1-7. Channel selector column + step clipboard.** Acceptance: selector LEDs per row; click/RMB-multi/double-click-all per §4.2; `Ctrl+C/X/V` moves steps between rows and patterns; `Alt+C` clone; `Alt+Del` delete; `Alt+↑/↓` reorder; `Ctrl+click` name = preview sound. Model: clipboard struct in `App` (`{steps:[u8;16], velocities…}`); row reorder = `instruments` reorder + drum-row index remap (careful: drum `row` binding in `sequence.rs` — reorder must remap `InstrumentKind::Drum{row}` or switch rows to stable ids; recommend the latter as part of this item).

**P1-8. Graph-editor velocity lane (`Ctrl+K`).** Acceptance: toggle opens a lane under rack rows reusing roll lollipop painting; click/drag sets step velocity for the focused row; `Alt+click` resets. Model: none (writes existing `drums[row][step]` velocities).

**P1-9. Playlist header: arrow menu + tool strip + ruler/track LEDs + gestures.** Acceptance: arrow menu with §3.1 tree (Edit basics + Snap + Select + Zoom live; rest disabled); Draw/Paint/Delete/Mute/Slip(disabled until P3)/Slice(basic cut)/Select/Zoom-to-selection tools switch cursor behaviour; ruler supports `Ctrl+drag` timeline select + click relocate; track headers gain mute/solo LEDs (state in model; solo affects audition only until mixer); double-click clip → Piano roll, `Alt+double-click` → Rack; clip RMB gains Make-unique `Ctrl+Shift+V`, Delete `Del`, Open-in-rack/roll, Select-in-playlist; arrangements label shows "Arrangement" (single, stub). Model: `Clip` gains `muted: bool`; track mute/solo arrays; `unique()` already exists (`playlist.rs:35-48`).

**P1-10. Piano-roll header parity + core shortcuts.** Acceptance: snap chips replaced by full Snap dropdown (Main default → global); Target-channel dropdown shows ticks for channels with notes; Target-control dropdown (Velocity live, Pan/others disabled); slide+porta+color segmented (porta/color disabled until P2); ghost toggle `Alt+V`; shortcuts `Ctrl+Q` quantize-start+end, `Ctrl+B` duplicate, `Del` delete-selection, `Shift+D` discard-lengths, `Shift+Enter` lane show/hide (manual override over auto-height `roll.rs:189`), `Shift+0` center. Model: note `muted` flag (mute tool `T`), slide `porta: bool` + `color: u8` fields (written by UI when enabled in P2, default 0).

**P1-11. Browser layout pass.** Acceptance: tab strip (All/Current-project/Plugin-db/Sounds/Starred stubs, right-click menu renders), rows 24 px, TAGS + search field pinned bottom (search filters the *current lists* — patterns/channels — as proof of wiring; full content search P3), collapse-all + refresh buttons (refresh re-reads patterns/channels now, fs later). Model: none.

**P1-12. Record arm + transport options menu.** Acceptance: red `R` arm button in row 1; right-click opens filter (Automation/Notes/Audio/Clips toggles stored in `App`/song prefs; recording itself P2); metronome `Ctrl+M` + precount `Ctrl+P` + blend `Ctrl+B` + step-edit `Ctrl+E` + loop-record toggles in OPTIONS + icon buttons (metronome clicks need audio engine — button toggles state now, clicks when engine can; state persisted). Model: `RecSettings{armed, filter:{auto,notes,audio,clips}, metronome, precount_bars, blend, step_edit, loop_rec}` in song or prefs.

### P2 — next (production sequencer)

**P2-1. Mixer v1.** 18 strips (Current+Master+16) per screenshot 1; strip 64 px: name, LED, pan, stereo-sep, fader, meters, arm; `+` add; inspector with input/output + 10 FX slots (empty-slot UI live; hosting real FX later); routing `Ctrl+L` from rack functional. Model: `MixerTrack{name,color,pan,vol,mute,solo,route_in,route_out,fx:[slot×10]}` + `Song.mixer`; `Instrument.mixer_target` wires in. Acceptance: rack→mixer routing audible; faders/mutes move audio; states persist in `.kfxl`.

**P2-2. Variable pattern length (16→512 steps).** Model: `Pattern.len_steps` + `sequence`/engine loop + rack/roll geometry (`STEPS` constant removal). Acceptance: length control in rack header edits 16/32/64/…; roll draws N steps; playlist clips scale. (FL: `channelrack.htm` §4.)

**P2-3. Channel groups + colors + pattern colors.** Model: `ChannelGroup{name,member_ids}`, `Instrument.color/icon`, `Pattern.color` (extends P1-2). Acceptance: filter dropdown lists groups, `PageUp/Down` cycles, `Alt+G` groups selection; rename `F2` + color picker on channels/patterns; playlist clips + rack names reflect colors.

**P2-4. Note color groups (16) + slide/porta semantics.** Model: `Note.color/slide-flags` (fields added in P1-10 get meaning); per-color slide routing; `Alt+C` recolor; ghost editing (`Double-RMB` jump). Acceptance: two overlapping colors slide independently (FL `pianoroll.htm` "Color Groups").

**P2-5. Full graph editor targets + event lane.** Release/Fine-pitch/Pan/ModX/ModY/Shift/Rep lanes in rack; Target-control lane in roll switches Velocity/Pan/… Acceptance: per-step values round-trip through save.

**P2-6. Browser depth: search/index, favorites, drag-drop, preview, Current-project.** With file-tree agent's design. Model: `favorites: BTreeSet<PathBuf>` in config; drag payload type in UI layer.

**P2-7. Recording v1 (notes + automation + audio-log).** Score-logger dump, audio-logger ring (FL `menu_tools.htm`), count-in, quantize-on-record, step-edit input. Needs audio/MIDI input path — biggest engine item after mixer.

**P2-8. Time formats + metronome clicks + typing-piano + tap dialog.** `Ctrl+T` piano, Min:Sec LCD, S/B switch, tap nudge/sync/half-speed.

**P2-9. Pickers (plugin `F8`, project `Ctrl+F8`) + Add-one menus + undo-history browser.** Needs plugin-host plan first (own research track).

### P3 — later (depth)

Stamp chords/scales, scale snap, riff/chord tools, LFO/randomize/articulate/chop/glue/quantize-dialog, slip tool + stretch handle + note-props dialog, keyboard styles/labels, waveform view, per-pattern time signatures, playlist timeline-select ops + loop-record + consolidate/stems + track groups/modes + performance mode, rack zip/sort/advanced-loop/loop-starter/MIDI-through/burn-MIDI/advanced-fill, browser preview/indexing/views, toolbar editing/presets, touch controller, macro panic/cleanup set.

Acceptance per item: mirror the FL manual section's observable behaviour; each ships with its shortcut and a disabled-until-ready menu entry promoted to live.

### P4 — skip (do not build)

FL Cloud/backup/sounds-subscription/AI-mastering/distribution, shop + trial locks, news/downloads/RSS + welcome marketing, Gopher LLM (revisit as own assistant later), homepage/forum/KB links, Image-Line licensing/product-info (replace with our scheme), diagnostics/reset/DPI/macOS-PBO fixes (own equivalents only if needed), "What's new" format, ZGame video (own video layer later), Edison-specific macros, Fruity-LSD/GM MIDI paths, Elastique/Realtime/Legacy stretch presets (own time-stretch names), 32-bit bridge/memory tooling.

---

## Appendix: measurement notes (so an engineer can re-check)

- FL shots: 1887×1080 full, 1551×1000 detached ×2. Our shots: 1366×768 (`pixels_per_point 1.0`).
- Rack pitch: column-band 350–850, rows 130–340 → steps at 141/171/201/231/261 ⇒ 30.0.
- Browser rows: band x0–330, rows 130–600 → 214/237/260/283/306 ⇒ 23.0.
- Playlist detached tracks: shot S2 band x150–260 rows 60–950 → 205/250/295 ⇒ 45 (may include OS scaling; treat as upper bound; main-window tracks visually ~30 — recommend default 32 + zoom).
- Playlist detached chrome: toolbar bottom edge at y≈30 (Δ89.9); title+toolbar+ruler ≈ 70 total.
- Step pads: row-band y200–230, cols 430–870 → pad pitch ≈ 26, width ≈ 22–24.
- Menu bar: band x0–330 rows 0–40 → strong steps at 13/14 and 25 ⇒ bar ≈ 26.
- Mixer strips: column steps cluster ≈ 60–70 pitch — coarse; target 64 chosen as midpoint, revisit when building P2-1 against a fresh 100 %-scale screenshot.
- Our constants: `DECK_H/BROWSER_W/STATUS_H` (`mod.rs:27-29`); `CONTROL_H` (`kit.rs:15`); rack (`rack.rs:10-17`); playlist (`playlist.rs:9-12`); roll (`roll.rs:10-17`); browser (`browser.rs:9`); tab bar (`workspace.rs:393`); text styles (`theme.rs:184-188`).

UNVERIFIED list (do not assert as FL fact): exact FL default playlist-track height at 100 %; mixer-strip exact pitch; FL25 in-app UI-zoom control existence; FL detached-window title-bar exact height (estimated 26–28 from S2/S3); FL piano-roll default row height; FL menu font size (estimated 12–13 from crops).
