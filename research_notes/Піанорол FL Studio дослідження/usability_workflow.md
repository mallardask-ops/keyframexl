# FL Studio Piano Roll — Usability & Workflow

## Default and customizable keyboard shortcuts

### Takeaway
FL Studio's piano roll exposes a large, context-sensitive shortcut set (single-letter tools, Alt/Ctrl/Shift modifiers, mouse-wheel property edits). Shortcuts are fixed by default but editable via Options > General settings / keyboard customization; single-letter keys only work when "Typing keyboard to piano keyboard" is off.

### Cited Findings
- Single-letter tool shortcuts: B Paint, C Slice, D Delete, E Select, P Draw (pencil), T Mute, Y Playback, Z Zoom, plus M keyboard view, O portamento, S slide, F show next property — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Tool/modifier combos: Alt bypasses snap; Backspace toggles Global Snap; PgUp/PgDn zoom; Double Left-click on note opens note properties; Double Right-click on ghost note swaps to ghost channel; Right-click deletes selected notes — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Alt+letter tool dialogs: A Arpeggiator, E Riff machine, F Flam, K Limit, L Articulate, O LFO, Q Quantize, R Randomize, S Strum, U Chop, W Claw Machine, X Scale Levels, Y Score Flipper; Alt+V ghost channels on/off; Alt+C change color; Alt+M mute / Alt+Shift+M unmute; Alt+G ungroup — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Edit shortcuts: Ctrl/Cmd+A select all, C copy, X cut, V paste, D deselect, B duplicate to right, G glue, L quick legato, U quick chop, M import MIDI, I insert current controller value, Insert/Space insert space, Delete delete space — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Selection: Ctrl+Shift+Left-click add to selection; Ctrl+Left-click select; Shift+click clone; Shift+I invert; Shift+C select by color; Shift+R random; Shift+M random groups; Shift+G group; Ctrl+D deselect; Ctrl+Alt+Home allow resizing from left — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Transpose/move: Shift+Up/Down semitone; Ctrl/Cmd+Up/Down octave; Shift+Left/Right move by snap; Alt+Arrows nudge (pixel); Ctrl+Arrows select time before/after selection; Ctrl+Return select time around selection — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Zoom presets: Shift+1/2/3 horizontal zoom levels, Shift+4 show all, Shift+5 zoom to selection; Zoom tool (Z) left-click drag zooms, left-click background zooms out; Ctrl+Right-click zoom on selection; Right-click toggles zoom/unzoom — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Mouse-wheel property edit: Alt+Mouse-wheel over a note changes the active note property (velocity/pan/etc.); Ctrl+Alt+Mouse-wheel fine-tunes; Shift+F cycles the active property; Shift+Alt+Mouse-wheel nudges notes on mouse-over — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm); [FL Studio vagon.io shortcuts](https://vagon.io/blog/fl-studio-keyboard-shortcuts).
- Middle-click+Right-click opens the Piano roll Context Tool Menu (local toolbar/main menu access); Middle-click drag pans; Middle-click (X1 button) on ghost note swaps channel — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Single-key shortcuts only work while "Typing keyboard to piano keyboard" (Ctrl+T) is off — [ShortcutsKeys FL Studio page](https://shortcutskeys.com/applications/fl-studio/shortcuts/).
- Cheat-sheet aggregators list 181+ shortcuts organized in 9 groups; assignments are context-dependent on active window/selection — [ShortcutsKeys](https://shortcutskeys.com/applications/fl-studio/shortcuts/); [flstudiopro.com](https://flstudiopro.com/fl-studio-shortcuts/).

### Inferences
- The shortcut density favors mouse+keyboard combos over pure keyboard editing; velocity editing is property-driven rather than a dedicated key.
- Context-dependency explains conflicting cheat sheets (same key does different things per window).

### Gaps
- No authoritative single "customizable keymap" editor documentation found beyond General settings; remapping individual piano-roll keys may require third-party tools (AutoHotkey) per community posts — [Reddit r/FL_Studio AutoHotkey workaround](https://www.reddit.com/r/FL_Studio/comments/1pxsiwd/fix_for_fl_studio_piano_roll_ctrlz_undoredo/).

## Mouse interactions (clicks, drag, modifiers, scroll/pan)

### Takeaway
Left-click edits (tool-dependent), right-click deletes notes or pans/quantizes depending on modifier, middle-click pans; Shift/Ctrl/Alt modify behavior; scroll zooms (Ctrl+wheel) or nudges properties (Alt+wheel); double-click opens note properties.

### Cited Findings
- Draw tool (P): Left-click add note; Left-click-and-drag reposition before release; Right-click delete; Right-click drag delete multiple; Shift add in stretch mode; Ctrl make selection; Alt add in Draw mode with snap set to none — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Slice tool (C): Left-click drag vertically to slice; Shift slice without dragging; Ctrl selection; Shift+Alt slice without dragging with snap none — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Standard selection: Ctrl+Left-click click/drag rectangle; Ctrl+Shift add to selection; Ctrl+Shift deselect individual; Ctrl+Left-click time ruler selects time range; Ctrl+Left-click preview keyboard selects pitch range — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Deselect all: Ctrl+click empty space; Invert: Shift+I; Magic lasso: Piano roll menu > Select > Magic lasso, move mouse in loop, no notes selected beforehand — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Add/resize notes: Left Shift+Left-click add and resize; Right Shift+Left-click slice; Left Shift+Right-click pan view; Right Shift+Right-click slice & delete smallest part — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Pan: Middle-mouse-click hold & drag; Left Shift+Right-click pan; two-dimensional scrolling via right-mouse on scrollbars or middle-mouse drag — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm); [Image-Line GUI manual](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/basics_interface.htm).
- Zoom: Zoom tool (Z) left-click drag zooms to selection, left-click background zooms out; Ctrl+Right-click zoom on selection; Right-click toggles zoom/unzoom; PageUp/PageDown zoom centered on cursor; Ctrl+Mouse-wheel horizontal zoom, Ctrl+Shift+Mouse-wheel vertical zoom — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm); [HomeMusicMaker zoom guide](https://www.homemusicmaker.com/zoom-in-fl-studio).
- Note move/resize: select note, left-click drag; resize from note end (cursor becomes double-headed arrow); vertical pitch lock (Shift) after first clicking note then dragging horizontally; horizontal timing lock (Ctrl) after first clicking then dragging vertically — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Stretch/compress: select group, drag pop-up handle OR Right-Shift drag right edge of a note; Alt+Right-Shift stretches in 25% increments — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Clone selection: Shift+Left-click drag (Shift+drag); duplicate: Ctrl+B — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Alt+Right-click audio preview (in draw mode); Right Alt+Right-click quantize selected; Left Alt+Right-click audio preview — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Click-and-hold functions (General settings): copy note by holding on it; cycle slide/porta by clicking-and-holding when adding; glue by placing cursor between neighbor notes and holding; snap note sides; mouse-wheel velocity while holding notes — [Image-Line general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Double-click note opens note properties (color, velocity, start, duration, pitch, pan); double-click time ruler selects time range; double right-click ghost note swaps channel — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm); [EDMProd tips](https://www.edmprod.com/fl-studio-piano-roll/).
- Right-click on note opens context menu (manual lists right-click a note opens context menu); right-click background deletes selected notes / deselects — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Community complaint: double-click drag gestures (e.g., double-middle-click drag scroll/zoom) require cursor to stay very still, with no documented hint — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=298381).

### Inferences
- Mouse-heavy, tool-modal workflow; modifiers are overloaded (Shift means clone/stretch/lock depending on context), which has a learning curve.

### Gaps
- Exact right-click-on-note context menu item list not fully enumerated in official manual; community/secondary sources only describe delete + properties — [FL Studio piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).

## Context menus (right-click notes, background, selection)

### Takeaway
Right-click deletes notes by default; middle-click+right-click opens the Context Tool Menu (local toolbar/main menu). Note properties are opened by double-click or right-click on a note; piano key right-click labels keys; context menu options mirror the main Piano roll menu (Edit/Tools/View/Snap/Select).

### Cited Findings
- Right-click deletes selected note(s); right-click drag deletes multiple notes; right-click on background opens slice tool when click-and-hold functions enabled (F10 > General settings) — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Middle-mouse-click + right-click anywhere in piano roll opens the Piano roll Context Tool Menu, providing local access to toolbar icon controls and main menu — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Right-click on a note opens the context menu (manual: "(Right+Click) a note to open the context menu") — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Right-click on preview keyboard key labels / custom labels (Right-Click key to label) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Right-click on target channel selector opens instrument interface; right-click event-editor area selects target; right-click marker edits marker; right-click separator flips event editor state — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Right-click note color selector customizes note colors; right-click zoom icon opens quick zoom options; right-click snap selector cycles snap resolution (community) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm); [Audeobox shortcuts guide](https://www.audeobox.com/learn/fl-studio/fl-studio-piano-roll-shortcuts-guide/).
- Piano roll main menu groups: File, Edit, Tools, View, Snap, Select, Zoom, Time Markers, Target channel/control, Auto Locate Channel, Auto Smoothing, Preview notes during playback, Center, Detached — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Edit menu includes Cut/Copy/Paste/Duplicate/Delete, shift/rotate/transpose, discard lengths, allow resizing from left, nudge with mouse wheel, change color, mute/unmute, insert/delete space, trim, insert controller value, turn into automation clip — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Tools menu: Riff machine, chord progression, quick legato, articulate, quantize (quick/quick-start/advanced), chop, glue, arpeggiate, strum, flam, claw machine, limit, flip, randomize, scale levels, LFO, scripts, stamp — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).

### Inferences
- Context menus are largely mirrors of the top menu rather than deep per-note property menus; per-note editing happens via double-click properties dialog or event editor.

### Gaps
- No exhaustive "right-click on selected multi-note selection" menu differences documented; community requests exist to add right-click key-label menus — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=335245).

## Customization (toolbar, colors, grid, piano keys)

### Takeaway
Extensive visual customization: 16 note-color groups (customizable palette saved as .ncp), grid color/contrast/invert, time shading, note shapes (shadow/rounded), keyboard styles/labels, mini preview; toolbar arrangement via floating/detached windows; snap/global snap; view options.

### Cited Findings
- Note color selector: 16 color groups; right-click selector to customize colors; View > Note colors > Edit palette saves .ncp to Documents\Image-Line\FL Studio\Presets\Note color presets; View > Grid color (Ctrl+Alt+G) changes grid; Shift+C selects by color — [flstudiopro.com colors](https://flstudiopro.com/fl-studio-colors/); [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- View > Grid: Grid Color, Grid contrast (High/Medium/Low), Invert grid, Time segments (default 4 Bars) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- View > Content: Keep labels on screen, Shadow, Rounded notes — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- View > Time: Incremental scrolling, Precise time indicator, Mini piano roll preview on scrollbar — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- View > Helpers: Note grid highlights (Alt+B), Scale highlighting (default/black notes/auto), Background waveform (Alt+N), Ghost channels (Alt+V), Editable ghosts (Ctrl+Alt+V) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- View > Piano roll: Keyboard style (Classic/Modern/Flat/List), Keyboard labels (presets/none/all notes/root/highlighted/white notes only), Swap panels, Flip separator (Shift+Enter) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- General settings: GUI scaling, click-and-hold functions, auto zoom in piano roll, small scrollbars, detached plugins, multi-touch optimization, animations (Don't distract me/Smooth/Ultrasmooth), invert pencil buttons — [Image-Line general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Snap customization: local snap selector (Main/Line/Cell/none/steps/beats/bar/events/markers) + Global snap; Alt temporarily sets snap to none — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Scale highlighting: root note + scale type; auto-detect scale; shading modes Default/Black notes/Auto — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Time signatures and key markers editable in piano roll (Alt+Shift+T set time signature) — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).

### Inferences
- Visual customization is deep but note color is functionally tied to MIDI channels/slides/mono groups, so "cosmetic" recoloring has side effects.

### Gaps
- Toolbar button rearrangement (drag-to-reorder) not explicitly documented; may be limited to show/hide.

## Multi-selection editing (velocity, pitch, length)

### Takeaway
Multi-note editing is well supported: rectangle/lasso selection, Shift+click add/remove, select by color/pitch/time range; transposition, glue, articulate, quantize, scale levels, randomize, mute, nudge, stretch; per-note properties editable via event editor for selections; group notes for linked edits.

### Cited Findings
- Selection methods: Ctrl rectangle, magic lasso, time-range via time ruler, pitch-range via preview keyboard, add with Ctrl+Shift, remove with Ctrl+Shift, invert Shift+I, select by color Shift+C — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Group notes (Shift+G) link selected notes so actions on one affect all; ungroup Alt+G; note/pattern grouping toggle on toolbar — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Multi-note tools: Quick legato (Ctrl+L), Articulate (Alt+L), Quick quantize (Ctrl+Q / Ctrl+Alt+Q), Quantize dialog (Alt+Q), Quick chop (Ctrl+U), Chop (Alt+U), Glue (Ctrl+G), Arpeggiate, Strum, Flam, Claw machine, Limit, Flip, Randomize (Alt+R), Scale levels (Alt+X), LFO (Alt+O) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Velocity editing: Alt+Mouse-wheel over note changes active property; Shift+F cycles properties (velocity/pan/release/mod X/Y/pitch); event editor displays velocity as vertical lines; scale levels can multiply/offset velocity; randomize can randomize velocity levels — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm); [vagon.io](https://vagon.io/blog/fl-studio-keyboard-shortcuts).
- No single universal keyboard-only velocity up/down shortcut; use event lane or Alt+Mouse-wheel with Velocity active — [vagon.io](https://vagon.io/blog/fl-studio-keyboard-shortcuts).
- Stretch/compress selections via pop-up handle or Right-Shift drag; Alt+Right-Shift 25% increments — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Insert/delete space for selection (Ctrl+Ins / Ctrl+Del); trim selection; discard lengths (Shift+D) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Select overlapping/stacked notes; select muted; select odd (swung) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Multi-note property editing in event editor: Shift+Left-click drag sets same value across; Alt+Left-click resets property — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).

### Inferences
- FL leans on mouse-wheel + modifier for per-note property edits rather than dedicated keyboard shortcuts, which suits mouse-driven workflows.

### Gaps
- Simultaneous editing of length across a selection via keyboard (beyond discard lengths) not clearly documented.

## Copy/paste behaviors

### Takeaway
Copy/paste supports within-pattern and cross-pattern via clipboard; paste lands at leftmost visible bar (not original position); duplicate (Ctrl+B) appends to right; Shift+drag clones; MIDI clipboard import/export; merge pattern clips merges scores.

### Cited Findings
- Copy (Ctrl+C), Cut (Ctrl+X), Paste (Ctrl+V): pastes notes starting from the leftmost bar visible in the piano roll — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Duplicate (Ctrl+B): duplicates selection to the right of current location; if no selection, all notes; repeat interval can be defined by an independent timeline selection — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Clone selection: hold Shift then drag selection with left mouse; copy+paste workflow: select, Ctrl+C, scroll, Ctrl+V — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Paste from MIDI clipboard (Shift+Ctrl+V); Copy to MIDI clipboard — enables cross-DAW/sequencer transfer — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Import MIDI file (Ctrl+M); Export as MIDI file (Shift+Ctrl+M) — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Merge scores across patterns: place pattern clips in playlist with target clip on top, select all, Playlist menu > Edit > Merge pattern clips — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Drag-and-drop score: drag score from piano roll menu to browser or another piano roll — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Ghost channels (Alt+V) show notes from other channels in same pattern; editable ghosts (Ctrl+Alt+V) allow direct interaction — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Community note: paste lands at visible leftmost bar, not original position, requiring re-alignment (FL 12 complaint) — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=170143).

### Inferences
- Cross-pattern editing relies on ghost channels or pattern-clip merging rather than a direct "paste into pattern X" command.

### Gaps
- No "paste special" (e.g., paste only velocity) documented in piano roll; not found in sources.

## Undo/redo granularity and history

### Takeaway
Undo is per-project edit history with configurable max levels; default Ctrl+Z is a toggle (undo then redo), multi-step via Ctrl+Alt+Z; Alternate undo mode swaps to sequential; History list in Browser allows jump-to-state; redo buffer RAM-only.

### Cited Findings
- Default: Ctrl+Alt+Z steps back one edit; Ctrl+Z steps forward, unless at latest step where it toggles undo/redo of last edit — [Image-Line edit menu](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/menu_edit.htm); [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).
- Alternate undo mode (Options > General settings) makes Ctrl+Z sequential undo and Ctrl+Alt+Z sequential redo — [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).
- History list: Browser > Current project > History; right-click toolbar Undo button opens it; click any entry to jump state; entries named by action (FL 2026.1.3 shows "Stretch" for audio clip stretch) — [Image-Line browser manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/browser.htm); [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).
- Settings: Maximum undo levels; Undo knob tweaks (off by default, can flood history with CC messages); Alternate undo mode; Put undone recordings in recycle bin; Show undo history in browser when undoing; Show recent changes at top — [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).
- Knob tweaks undo requires wrapper "Notify about parameter changes" for third-party plugins; some plugins don't report changes — [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).
- Patcher has no internal undo; Edison has separate undo stack; redo buffer lost on crash (RAM only); autosave doesn't preserve redo — [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).
- Piano roll-specific Ctrl+Z quirk: acts as toggle even with history settings; community workaround uses AutoHotkey to remap Ctrl+Z to Ctrl+Alt+Z when piano roll detached — [Reddit r/FL_Studio](https://www.reddit.com/r/FL_Studio/comments/1pxsiwd/fix_for_fl_studio_piano_roll_ctrlz_undoredo/).
- Undo levels default 100; raising recommended if history runs out — [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).
- Revert to last backup and Backup folder for deeper rollback — [flstudiopro.com undo/redo](https://flstudiopro.com/fl-studio-undo-redo/).

### Inferences
- Undo model is unconventional (toggle) and a frequent pain point; alternate mode and History list mitigate.

### Gaps
- Whether piano roll edits are granular per-note vs per-action not specified; "undo history" granularity likely per command.

## View options (zoom, fit, overview, follow playback, loop region)

### Takeaway
Rich zoom/pan: cursor-centered zoom, quick zoom presets, zoom to selection/timeline, mini preview on scrollbar, auto zoom to fit, center to playhead, follow/auto-scroll playback, loop region via playlist markers; vertical zoom via keyboard/boxes.

### Cited Findings
- Zoom presets: Shift+1/2/3, Shift+4 show all, Shift+5 zoom to selection (timeline selection required); PageUp/PageDown cursor-centered; Ctrl+Mouse-wheel horizontal; Ctrl+Shift+Mouse-wheel vertical — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm); [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Zoom tool (Z): left-click drag zooms to area; left-click background zooms out; right-click toggles zoom/unzoom; right-click zoom icon opens quick zoom menu — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Auto zoom in piano roll (General settings): vertical/horizontal zoom automatically set to fit note data — [Image-Line general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Center (Shift+0): scrolls grid so playhead appears in middle — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm); [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Auto scrolling toggle (Options menu) for Playlist and Piano roll — [Image-Line options menu](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/menu_options.htm).
- Mini piano roll preview on scrollbar shows context-aware note preview — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Middle-mouse vertical drag on preview keyboard changes vertical zoom centered on clicked note — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Horizontal/vertical zoom boxes: click and drag edges; small scrollbars option halves size — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm); [Image-Line GUI manual](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/basics_interface.htm).
- Loop region: set in Playlist via Shift+Left-click drag in bar-marker region; loop record repeats selection; Piano roll markers (time markers, time signature changes) — [Image-Line options menu](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/menu_options.htm); [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Two-dimensional scrolling: right-mouse on scrollbars or middle-mouse drag — [Image-Line GUI manual](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/basics_interface.htm).
- Multi-touch gestures: pinch zoom, two-finger rotate vertical zoom, two-finger tap zoom out full, zoom-to-selection rectangle — [Image-Line GUI manual](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/basics_interface.htm).

### Inferences
- Overview/fit is handled by Shift+4 and auto-zoom; no single "fit all notes" button beyond auto-zoom setting.

### Gaps
- "Follow playback" toggle naming not explicitly a piano roll option; auto-scroll covers it.

## Color coding (channel, velocity, custom, muted)

### Takeaway
Note color is a functional grouping system (MIDI channel/slides/mono), not cosmetic velocity coloring; 16 customizable colors saved as .ncp; velocity only subtly darkens notes; muted notes selectable; channel-based ghost notes.

### Cited Findings
- 16 color groups; each locked to a MIDI channel for plugins hosted by Wrapper; slides only bend notes of same color; mono mode applies per color group; recording MIDI channel through maps incoming channels to colors — [flstudiopro.com colors](https://flstudiopro.com/fl-studio-colors/); [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Customize colors: right-click note color selector; View > Note colors > Edit palette; save/load .ncp presets — [flstudiopro.com colors](https://flstudiopro.com/fl-studio-colors/).
- Change color of selected notes: Alt+C (applies to all if none selected) — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Select by color: Shift+C selects all notes/slides of current color — [Image-Line manual shortcuts page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm).
- Velocity does not color notes; lower velocity slightly darkens notes — [Image-Line forums color-by-velocity thread](https://forum.image-line.com/viewtopic.php?t=204569).
- Muted notes: mute tool (T) / Alt+M; select muted notes via Select menu; muted notes don't play — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Ghost notes shown as grey semi-transparent notes from other channels in same pattern — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Community scripts: velocity-to-color Python script maps velocity ranges to note colors — [Reddit r/FL_Studio](https://www.reddit.com/r/FL_Studio/comments/1px11z6/how_to_automaticlly_set_all_piano_roll_note_color/).
- Color groups appear to plugins as separate MIDI channels — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).

### Inferences
- Users wanting Logic-style velocity coloring must use scripts or manual recolor; native support absent.

### Gaps
- No native "color by velocity/pitch" toggle as of 2026; feature requests exist since 2016 — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=164816).

## Workflow integration (opening, piano roll only vs channel settings)

### Takeaway
Piano roll opens via F7, toolbar icon, right-click channel in channel rack, double-click pattern in playlist; targets channels within a pattern; separate from channel settings (F3 instrument interface); auto-locate channel; can switch target channel without closing.

### Cited Findings
- Open piano roll: F7, toolbar piano icon, right-click track in channel rack, double-click pattern in playlist — [Sweetwater how-to-use FL Studio](https://www.sweetwater.com/insync/how-to-use-fl-studio/).
- Target channel dropdown in title bar selects channel for editing; channels with notes in current pattern show tick; mouse-wheel cycles channels with notes; Ctrl+Mouse-wheel shows used/unused — [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Auto Locate Channel: piano roll displays first non-empty channel when switching patterns — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Target control selects automation/property data for event editor — [Image-Line piano roll menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm).
- Channel settings (instrument interface) opened separately via channel rack button; "Separate MIDI control and interface editing" option allows editing one channel while playing another — [Image-Line general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Pattern-based: piano roll edits notes for selected pattern; playlist patterns reference patterns — [Sweetwater how-to-use FL Studio](https://www.sweetwater.com/insync/how-to-use-fl-studio/).
- Typing keyboard to piano (Ctrl+T) enables keyboard-as-MIDI; step edit (Ctrl+E) step entry — [Image-Line options menu](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/menu_options.htm).
- Recording: blend recorded notes (Ctrl+B), overdub; MIDI channel through for multi-channel recording — [Image-Line options menu](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/menu_options.htm); [Image-Line piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).

### Inferences
- Clear separation between note editing (piano roll) and sound design (channel settings), reducing context switching.

### Gaps
- "Piano roll only" mode (detached window) vs channel settings interaction not specifically documented beyond detached windows.

## Docking/floating, multi-monitor, detach

### Takeaway
FL Studio uses floating windows; any window (including piano roll) can be detached via top-left menu > Detached and moved to another monitor; main desktop can be stretched across monitors; layouts saved via View > Arrange windows.

### Cited Findings
- Detach: component's top-left menu > Detached; drag title bar to other monitor; resize; works for Piano roll, Playlist, Mixer, Event editor, plugins — [Sweetwater how-to-use FL Studio](https://www.sweetwater.com/insync/how-to-use-fl-studio/); [flstudiopro.com multi-monitor](https://flstudiopro.com/fl-studio-multiple-monitors/).
- Multi-screen: two approaches—detached windows on external monitors, or stretch main desktop (Windows: right-click X > Maximize to all monitors) — [Image-Line GUI manual](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/basics_interface.htm).
- Detach all new plugins setting; Wrapper menu Detach for existing plugins — [Image-Line general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Browser undocks by dragging header; drag to left/right edge to dock; "Hide while dragging (when detached)" option — [flstudiopro.com multi-monitor](https://flstudiopro.com/fl-studio-multiple-monitors/).
- Save/recall layouts: View > Arrange windows > Save custom layout / Custom — [flstudiopro.com multi-monitor](https://flstudiopro.com/fl-studio-multiple-monitors/).
- Detached windows seen by Windows/macOS as separate applications; detached plugins may stop responding to MIDI if FL UI tweaked; use FL Studio ASIO driver multi-client — [Image-Line general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Multi-touch: two-finger tap/drag title bar moves window; maximize/restore/close/resize gestures — [Image-Line GUI manual](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/basics_interface.htm).

### Inferences
- Flexible for multi-monitor production; detached piano roll is common for large screens.

### Gaps
- No per-window "always on top" or tabbed docking documented.

## Performance with large note counts

### Takeaway
Performance issues stem more from plugins/polyphony/CPU than piano roll rendering; high PPQ and many notes can slow UI; optimizations include lowering PPQ, smart disable, consolidating, buffer tuning, closing plugin windows, disabling animations.

### Cited Findings
- PPQ above 192 can significantly impact CPU load; default 96 recommended — [Image-Line optimization](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/app_opt.htm).
- Plugin performance monitor (double-click CPU panel) identifies heavy plugins; multithreaded generator/mixer processing; smart disable — [Image-Line optimization](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/app_opt.htm); [flstudiopro.com high CPU](https://flstudiopro.com/fl-studio-high-cpu/).
- Close all plugin windows (Alt+F12); animations to "Don't distract me"; deselect Ultrasmooth; disable "Keep labels on screen" — [Image-Line optimization](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/app_opt.htm).
- Buffer length 10-20 ms Intel/AMD; 128 samples Apple Silicon — [Image-Line optimization](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/app_opt.htm).
- Consolidate track renders clips to audio, reducing CPU; preserve source project — [Image-Line optimization](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/app_opt.htm).
- Community: piano roll FPS drops with many notes in a single pattern; also voice/polyphony-related — [Reddit r/FL_Studio](https://www.reddit.com/r/FL_Studio/comments/q0bs9e/fl_studio_piano_roll_fps_drops_when_having_too/).
- Large projects (hundreds of patterns/channels) cause lag even with smart disable; rendering slows with many automation clips — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=301481); [Reddit rendering slowdown](https://www.reddit.com/r/FL_Studio/comments/fcjv0p/rendering_long_dj_mix_starts_fast_slows_then/).
- Interface lag with clean audio addressed by closing plugin windows, animation settings, playback tracking — [Image-Line optimization](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/app_opt.htm).

### Inferences
- Piano roll editing performance is generally acceptable; slowdowns are project/plugin-scale, not note-count rendering alone.

### Gaps
- No official max note count or rendering benchmark published.

## Accessibility (keyboard-only, screen reader)

### Takeaway
FL Studio is not screen-reader accessible; UI is icon-dense graphical; no accessibility APIs; keyboard-only editing possible for many operations but mouse still required for precise edits; community projects aim to add TTS feedback via Python API.

### Cited Findings
- Image-Line support: "FL Studio cannot be used with screen readers. The FL Studio user-interface is icon-dense and makes extensive use of visual/graphical controls... precludes FL Studio working successfully with screen readers." — [Image-Line support KB](https://support.image-line.com/action/knowledgebase?ans=606).
- NVDA issue: FL Studio uses no accessibility APIs (UIA/IAccessible2); Delphi app; screen review sees blank; closed as blocked/needs-external-fix — [NVDA GitHub issue #11859](https://github.com/nvaccess/nvda/issues/11859).
- Keyboard-only editing: many shortcuts exist, but precise selection/editing (lasso, drag) still mouse-based; typing keyboard can input notes (Ctrl+T) — [Image-Line options menu](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html/menu_options.htm).
- Community: blind users seek developers to leverage FL Studio Python API for TTS feedback; no official accessibility roadmap found — [Reddit r/FL_Studio](https://www.reddit.com/r/FL_Studio/comments/1c9v76m/totally_blind_looking_for_developers_to_work_on/).
- FL Studio Mobile notes "accessibility features not indicated" — [App Store listing](https://apps.apple.com/us/app/fl-studio-mobile/id432850619).

### Inferences
- Accessibility is a significant gap; production by visually impaired users relies on other DAWs or external tooling.

### Gaps
- No official plans to add accessibility APIs; not found in sources.
