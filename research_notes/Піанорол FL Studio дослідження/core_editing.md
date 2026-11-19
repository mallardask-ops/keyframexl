# FL Studio Piano Roll — Core Editing Model

## 1. Piano Roll Toolbar Tools

### Takeaway
FL Studio's Piano roll exposes a set of single-key tool modes (Draw, Paint, Delete, Mute, Slice, Select, Zoom, Play selected, plus drum-paint and Interpolate) plus a Tools menu (spanner) with advanced commands; the toolbar also hosts Snap-to-grid, Snap-to-scale, Chord Stamp, and Target Channel selectors.

### Cited Findings
- Toolbar tool icons and shortcuts: Draw (P), Paint (B), Paint drum sequencer mode (N), Delete (D), Mute (T), Interpolate (I), Slice (C), Select (E), Zoom to selection (Z), Play selected (Y). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Draw (P): Left-click adds a note; Left-click-and-drag repositions before release; Right-Click deletes; Right-Click-and-drag deletes multiple; Right-Click-and-hold opens slice tool (with F10 click-and-hold enabled). Modifiers: Shift = stretch mode, Ctrl = selection, Alt = draw with snap set to 'none'. — [Piano roll manual](https://cluster.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Paint (B): paints multiple notes as you drag in lines; can paint above/below existing notes for chords; Left-click adds single note; Right-Click deletes. Modifier Shift = monophonic paint (replaces overlapped notes). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Paint drum sequencer mode (N): tuned for drum sequencing; Left-click adds/mutes notes; Left-click on notes mutes/unmutes; Right-Click deletes; Alt paints at half the selected snap. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Delete (D): click or click-and-drag to delete; Ctrl makes a selection then Delete. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Mute (T): Left-click mutes; Left-click-and-drag mutes multiple; Right-Click deletes. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Slice (C): Left-click-and-drag vertically to slice notes; Shift = slice without dragging; Ctrl = selection; Shift+Alt = slice without dragging with snap none. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Select (E): click or click-and-drag selection box; Shift adds/removes notes. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Zoom (Z): Left-click-and-drag zooms to rectangle; Left-click on background zooms out; Right-Click toggles zoom/unzoom. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Play selected (Y): click to preview notes; drag to scrub. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Interpolate (I): only works when actual Event Data is shown in the lower Editor Panel; Left-click-and-drag horizontally applies interpolation (stepped → straight-line gradients). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Tools menu (spanner, Left-Click): opens Tools submenu; Right-Click opens Chord Progression Tool; Middle-Click opens Riff Machine. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Snap to grid (magnet): Left-Click opens Piano roll Snap settings; Right-Click sets default note length (also set by clicking any existing note). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Snap to scale (note icon): Left-Click toggles snap to key/scale; Right-Click chooses scale (tonic + scale). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Chord Stamp: choose chord type then click to place; "Only one" returns to single-note mode after placing; Automatic chord (top-down/bottom-up) modes; manual chord/scale presets. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- In piano roll, S key = toggle Slide note (portamento), not Slip edit (Slip edit S is a Playlist tool). — [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)
- Advanced Tools menu commands: Riff machine (Alt+E), Generate chord progression (Alt+P), Quick legato (Ctrl+L), Articulate (Alt+L), Quick quantize (Ctrl+Opt+Cmd+Q / Ctrl+Q), Quick quantize start times (Shift+Q), Quantize (Alt+Q), Quick chop (Ctrl+U), Chop (Alt+U), Glue (Ctrl+G), Arpeggiate (Alt+A), Strum (Alt+S), Flam (Alt+F), Claw machine (Alt+W), Limit (Alt+K), Flip (Alt+Y), Randomize (Alt+R), Scale levels (Alt+X), LFO (Alt+O), Stamp, Script. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)

### Inferences
- The tool set is intentionally mode-based (one active tool at a time) with modifier keys providing selection/snap overrides, mirroring FL Studio's Playlist tool model.
- "Slip" as a tool name only applies to audio clips in the Playlist; in the Piano roll the S key toggles slide/portamento on notes.

### Gaps
- I could not find an official exhaustive list of every Piano roll toolbar icon in FL Studio 24.x beyond the manual's tool list; minor version-specific icon additions (e.g., Note Repeat UI) are documented only in changelogs.

## 2. Note Creation

### Takeaway
Notes are created primarily in Draw/Paint modes: Left-click places a note at default length (last-touched note length or Right-Click-set default), Left-click-and-drag sets length on the fly, Right-Click deletes; double-clicking a note opens Note Properties.

### Cited Findings
- "To add notes and draw the length, hold the (Left-Shift) Left-click and drag to the desired length." / "In draw or paint mode, Left-click in the Piano roll's grid to draw a note. The default length for notes will be equal to the last touched note." — [Piano roll manual (ww2)](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///pianoroll.htm)
- Draw tool: Left-click adds a note; Left-click-and-drag repositions the note before releasing; Right-Click deletes notes; Right-Click-and-drag deletes multiple. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "New notes normally take the last note length you worked with." / "Right-click the snap icon to set the default length for new notes." — [flstudiopro.com beginner guide](https://flstudiopro.com/fl-studio-piano-roll)
- Default note length can be set by Right-Clicking the Snap to grid icon, or by clicking any existing note (its length becomes default). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Left-Shift+Left-click and drag" adds and sizes a note; "Draw tool + Shift" extends note length. — [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)
- Double Left-click on a note opens Note Properties dialog (per-note velocity, pan, release, Mod X/Y, pitch). — [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)
- Note properties pop-up: Double-click a note to display it; Levels controls per-note PAN, VEL, REL, MODX, MODY. — [Piano roll manual (beta)](https://www.image-line.com/fl-studio-learning/fl-studio-beta-online-manual/html/pianoroll.htm)
- Click-and-hold functions (F10 > General): Copy note (hold on existing note), Cycle slide/porta (click and hold when adding a note), Glue notes (cursor between two neighbor notes, resize cursor, click and hold). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "New notes will use this length when added to the Piano roll. You can also set the default length by clicking any existing note." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)

### Inferences
- Length-setting while drawing is a two-mode gesture: default-length click vs Shift-drag sizing; the "last touched note" default makes repeated entry fast.
- Slide/porta cycling via click-and-hold lets a single draw gesture produce slide or portamento notes without switching tools.

### Gaps
- Exact default note-length values offered in the Right-Click snap menu (e.g., list of note values) are not enumerated in the sources I fetched; the manual only states it sets default length.

## 3. Grid, PPQ, Time Signature, Zoom

### Takeaway
The Piano roll grid resolution derives from Project Timebase (PPQ, default 96), Time Signature (numerator/denominator), and the local/Global Snap setting; zoom (horizontal/vertical) reveals finer subdivisions when Snap = Line, and the top ruler shows bars/beats.

### Cited Findings
- "Resolution / grid - The smaller grid each bar is divided into are set by the window's snap setting. Scroll speed will slow when snap settings other than 'Line' are used. Resolution can also change with Zoom when Snap is set to Line. Selecting Main will use the Global Snap value." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Duration - The horizontal length represents time. The number on the Piano roll's top-ruler are bars (groups of beats)." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Pitch is mapped from bottom (low) to top (high)." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Timebase (PPQ): "Sets the event resolution for the current project in Pulses Per Quarter-note (PPQ)... default setting is 96 with higher PPQ allowing finer control but it also uses more CPU." Affects Piano roll note placement, automation, internal controllers, Pattern Clip placement. — [Project Settings manual](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///songsettings_settings.htm)
- "Adjusting PPQ to a lower level mid-project, will alter the placement of notes and clips if these events fall between the new PPQ grid. Increasing PPQ mid-project won't affect event placement." — [Project Settings manual](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///songsettings_settings.htm)
- Time signature: Numerator (beats per bar), Denominator (beat length). Multiple time signatures per project via Piano roll/Playlist Time Markers (Shift+Alt+T). — [Project Settings manual](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///songsettings_settings.htm)
- "Bar - The number of beats per bar... you can change the Pattern Length on the Channel Rack to set a custom number of beats per bar for each pattern." — [Project Settings manual](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///songsettings_settings.htm)
- "Zoom resolution depends on the 'Timebase' (PPQ) settings... The higher the PPQ, the greater the resolution." — [Interface manual](http://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_interface.htm)
- "Playlist maximum zoom is limited by the PPQ settings." — [Project Settings manual](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///songsettings_settings.htm)
- Time markers: Shift+Alt+T inserts time signature changes; can be set in Piano roll and Playlist. — [Project Settings manual](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///songsettings_settings.htm)
- "Mini piano roll preview - Shows a context aware preview of Notes in the Piano roll, on the Scroll bar." / "Incremental scrolling" / "Precise time indicator." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Zoom shortcuts: Page Up/Down zoom in/out (centers on cursor); Quick-zoom 1/2/3 (Shift+1/2/3); Zoom out far (Shift+4); Ctrl+Mouse Wheel horizontal zoom; Ctrl+Shift+Mouse Wheel vertical zoom; Zoom to selection (Ctrl+0). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm), [Audeobox shortcuts](https://www.audeobox.com/learn/fl-studio/fl-studio-piano-roll-shortcuts-guide/)
- Middle-Mouse-Button drag scrolls vertically and horizontally simultaneously. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Left-Shift+Right-Click" pans view; "Middle-Mouse-Click" pans view (hold & drag). — [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)

### Inferences
- FL Studio uses a single project-wide PPQ (not per-track), so zoom ceiling and edit precision are global; default 96 PPQ gives 1/384-note placement precision at 4/4.
- When Snap = Line, zooming in effectively increases grid subdivision, letting users place notes at finer positions without changing PPQ.

### Gaps
- Exact mapping of zoom levels to PPQ subdivisions and the maximum zoom factor are not specified in the sources fetched.

## 4. Snapping Modes and Quantization

### Takeaway
Snapping is governed by a local Piano roll magnet (Line/Cell/none/Step/Beat/Bar fractions, or Main = Global Snap) plus an optional Snap-to-scale (pitch) constraint; quantization commands (Quick quantize, Quantize dialog with groove templates/strength) align notes to the current snap grid; Alt bypasses snap while dragging.

### Cited Findings
- Global Snap options: Line (snap to nearest grid-line, resolution changes with zoom), Cell (snap to start of grid-cell), (none) (no snapping, limited by PPQ), Steps 1/6 to 1 (step), Beats 1/6 to 1 (beat), Bar (1 bar). Mapping: 1/4 Step = 64th, 1/2 Step = 32nd, 1/4 Beat = 16th, 1/3 Beat = triplets, 1/2 Beat = 8th, 1 Beat = quarter, Bar = whole note. — [Toolbar panels manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)
- "Input quantizing - The settings on the Global snap determine MIDI input quantizing... (none), Line & Cell are not input quantized." — [Toolbar panels manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)
- "Snapping can be temporarily disabled by holding the Alt key when dragging events." — [Toolbar panels manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)
- Piano roll local Snap: Left-Click opens Snap settings; Right-Click sets default note length; selecting 'Main' uses Global Snap value. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Hold the ( Alt ) key to temporarily bypass snap." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Snap to scale (note icon): Left-Click toggles snap to Key/Scale; Right-Click chooses tonic + scale. "Notes won't automatically snap until they are moved, even if they fall out-of-scale after settings are changed." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Snap MIDI to Scale - (Right-Click) option on Piano roll > Snap to key menu to snap incoming MIDI to the same settings as the Piano roll." (FL Studio 24.2). — [What's new FL Studio 24](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html///basics_new_24.htm)
- Quick quantize (Ctrl+Q / Ctrl+Opt+Cmd+Q): "Applies basic quantize to all selected Notes/slides according to the global snap setting... if nothing selected, applies to all." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Quick quantize start times (Shift+Q): quantizes start times only. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Quantize (Alt+Q): opens Quantizer dialog. Controls: Groove template (.fsc), Strength (Start time mix, Sensitivity, Duration mix, Quantize mode: duration/end time/leave duration/leave end time), Levels knobs to mix original vs groove-preset properties. — [Quantizer manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_qnt.htm)
- "Swing" selection: "Select odd (Shift+O) - Selects 'swung' Notes. That is Notes off the main beat-grid." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Quantizing "auto-aligns Notes to the Piano roll grid." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Snap options in Paint drum mode: "Alt paints at half the selected snap." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)

### Inferences
- "Swing" is not a dedicated global swing percentage in the sources found; instead, off-grid notes are selectable via "Select odd," and groove templates (.fsc) provide swing/groove quantization.
- Groove templates let the Quantizer apply humanized timing (start/end offsets) beyond rigid grid snap.

### Gaps
- I found no reliable source describing a single global "swing amount" slider for the Piano roll; swing appears implemented via groove templates and "Select odd." If a dedicated swing control exists, it is not documented in the fetched manual pages.

## 5. Selection Model

### Takeaway
Selection is modifier-driven (Ctrl/Ctrl+Shift) or via Select tool; supports marquee rectangle, magic lasso, pitch/time-range selection, select-all, invert, color/pitch/velocity filtering, group, and stacked/overlapping-note selection.

### Cited Findings
- "Standard selection - Press and hold (Ctrl key) and either click a note to select it, or drag a rectangle to select all notes in enclosed area." — [Piano roll manual (ww2)](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///pianoroll.htm)
- "Add notes to selection - Hold (Ctrl+Shift) together while selecting to add notes to the existing selection." / "Remove notes from selection - (Ctrl+Shift) deselects individual notes." — [Piano roll manual (ww2)](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///pianoroll.htm)
- "Deselect all notes - While holding (Ctrl key), click empty space in Piano roll's grid." — [Piano roll manual (ww2)](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///pianoroll.htm)
- "Magic lasso - Move your mouse pointer in a loop that encloses the Notes you want to select (no clicking required). Make sure you have no Notes selected before performing the lasso action. Right-Click the empty Piano roll to deselect all Notes." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Selecting notes in a time or pitch range - Time: (Ctrl+Left-click) or double-click the time ruler and drag to select all notes in a specified time range. Pitch: (Ctrl+Left-Click) the Preview Keyboard to select a range of notes with the same pitch (dragging vertically on the Keyboard selects notes in the vertical pitch range)." — [Piano roll manual (ww2)](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///pianoroll.htm)
- Select menu commands: Deselect (Ctrl+D), Select all (Ctrl+A), Select one at random (Shift+R), Select more at random (Shift+M), Select by color (Shift+C), Select odd (Shift+O), Select muted, Select overlapping notes, Select stacked notes, Invert selection (Shift+I), Select time around selection (Ctrl+Enter), Select previous/next time (Ctrl+Left/Right arrow), Magic lasso. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Select (E) - Click notes or Click and drag to make group selections. Modifier: Shift add/remove notes from the selection." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Select stacked notes - Selects all but the lowest Notes that are 100% overlapped." / "Select overlapping notes - Selects all but the lowest of any overlapping Notes, up to and including 100% overlap." (FL Studio 24 added Select stacked clips/notes). — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm), [What's new FL Studio 24](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html///basics_new_24.htm)
- Group notes (Shift+G): "Grouping functions so that an action on any note in the Group will affect all other notes in the same way." Toggle via Note/Pattern Grouping switch on toolbar. — [Piano roll manual (ww2)](https://ww2.image-line.com/fl-studio-learning/fl-studio-online-manual/html///pianoroll.htm)
- "Ctrl+Shift adds notes to a selection or removes them from it." (also in Select tool). — [nameonthebeat.com](https://nameonthebeat.com/en/blogs/alles-over-beats/fl-studio-piano-roll)
- Invert selection shortcut listed as Ctrl+Shift+I in some references. — [Audeobox shortcuts](https://www.audeobox.com/learn/fl-studio/fl-studio-piano-roll-shortcuts-guide/)

### Inferences
- FL Studio's selection is note-centric (not event-centric); pitch/velocity filtering is done via Select menu commands rather than a dedicated filter dialog.
- "Select by color" enables independent editing of overlapping notes assigned to different color groups.

### Gaps
- I found no dedicated "filter by velocity range" command in the manual; velocity-based selection is not explicitly documented beyond select-all/invert.

## 6. Editing Operations

### Takeaway
Core edits include move (drag/arrow keys by snap or semitone), resize right/left edges, transpose (semitone/octave), stretch/compress time, glue, slice/chop, flip, randomize, articulate/legato, strum, flam, claw machine, limit, arpeggiate, plus insert/delete space.

### Cited Findings
- Move: "click the center of a note and drag to move it to a new position or pitch"; Shift+Up/Down moves selection by semitone; Ctrl+Up/Down moves by octave; Shift+Left/Right moves by current Snap unit; Alt+Arrow nudges freely. — [Audeobox](https://www.audeobox.com/learn/fl-studio/how-to-use-the-piano-roll-in-fl-studio/), [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)
- Resize: "Hover over the right edge of a note until the cursor changes to a resize arrow, then drag to extend or shorten"; "Edit > Allow resizing from left lets you drag the start of a note, not only its end." — [Audeobox](https://www.audeobox.com/learn/fl-studio/how-to-use-the-piano-roll-in-fl-studio/), [mixelite.com](https://mixelite.com/blog/fl-studio-piano-roll-guide/)
- Stretch/compress: "Select a group of notes and drag the pop-up handle OR hold Right-Shift and drag the right edge of a note in the selection. This changes both note length and sequence duration. (Alt + Right-Shift) stretches in 25% increments." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Alternative to the Drag handle, hold (Right-Shift) key then click on the right-side of a note in the group and drag." (stretch handle). — [Piano roll manual (beta)](https://www.image-line.com/fl-studio-learning/fl-studio-beta-online-manual/html/pianoroll.htm)
- Glue: "Glue contiguous (touching) Notes together... Place the cursor between 2 neighbor Notes, so the resize cursor appears, then click and hold to glue them" (with F10 click-and-hold) or Ctrl+G. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Slice: "Left-click and drag vertically to slice notes"; "Right-Shift+Left-click" slices; "Right-Shift+Right-Click" slices and deletes smallest part. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Quick chop (Ctrl+U): slices long notes into shorter based on Snap. Chop (Alt+U): opens Chopper dialog with rhythm pattern + Time mul. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Flip (Alt+Y): flips score vertically and/or horizontally. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Randomize (Alt+R): opens Randomizer; generates random note sequences based on scale and/or randomizes note levels. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Articulate (Alt+L): staccato/portato/legato; Quick legato (Ctrl+L). — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Strum (Alt+S): shifts timing and velocity of chord notes to simulate strumming. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Flam (Alt+F): two drum hits in close succession. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Claw machine (Alt+W): slices and re-times note sequences. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Limit (Alt+K): transpose and limit note range to a key range. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Arpeggiate (Alt+A): arpeggiator based on score presets. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Insert/Delete space: Insert space (Ctrl+Ins), Slice & insert space (Ctrl+Alt+Ins), Delete space (Ctrl+Del), Trim selection. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Duplicate (Ctrl+B): duplicates selection to the right; if no selection, all notes. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Copy/paste: Cut (Ctrl+X), Copy (Ctrl+C), Paste (Ctrl+V) starting from leftmost visible bar. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Nudge with mouse wheel: Shift+Mouse-Wheel moves notes (according to PPQ) when over a note. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Mouse-wheel property changes: Alt+Mouse-wheel changes currently selected note property over a note; Ctrl+Alt+Mouse-wheel fine-tunes. — [Piano roll manual (beta)](https://www.image-line.com/fl-studio-learning/fl-studio-beta-online-manual/html/pianoroll.htm)
- Reverse: not found as a standalone Piano roll command in sources; Flip provides horizontal flip (which reverses time order). — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)

### Inferences
- "Reverse" as a distinct operation is not documented in the Piano roll menu; time-reversal is achieved via Flip (horizontal) or Playlist audio-clip reverse (7/8/9 keys). This is a gap worth noting.
- Stretch handle + Right-Shift drag both implement time-scaling; Alt+Right-Shift gives quantized 25% steps.

### Gaps
- I could not find a dedicated "Scale time" percentage dialog separate from the stretch handle/Right-Shift drag; none documented in the fetched manual.

## 7. Pitch Axis

### Takeaway
The vertical axis is chromatic (bottom = low, top = high) with a preview keyboard on the left; supports octave labels/note names, key highlighting, scale highlighting (manual or auto-detect), note grid highlights, and snap-to-scale pitch constraint.

### Cited Findings
- "Pitch - is mapped from bottom (low) to top (high)." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Notes - are displayed as horizontal bars." / "Preview Channels by Left-clicking the Preview Keyboard keys." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Keyboard style: Classic, Modern, Flat, Dark, List. — [edmprod.com](https://www.edmprod.com/fl-studio-piano-roll)
- Keyboard labels: View > Keyboard labels > All notes prints a name on every key; options Highlighted notes, Root notes, White notes only. — [mixelite.com](https://mixelite.com/blog/fl-studio-piano-roll-guide/), [flstudiopro.com](https://flstudiopro.com/fl-studio-scale-highlighting/)
- Note grid highlights (Alt+B): extends keyboard key shading onto the edit area as background contrast. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Scale highlighting: choose scale root (C–B) and scale type (Major, Minor, etc.); background grid shows scale notes highlighted in lighter color. Options: Default (no shading), Black notes (shade non-scale notes), Automatic (detect scale from notes). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm), [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Automatic - Detect the score scale... performs an 'average' detection based on either the whole score (if nothing is selected) or on the selection. Several Notes are required before auto-detection can begin." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Scale list comes from .fsc files in Data\Patches\Scores\Reference scales; custom scales can be created by editing and saving .fsc. — [flstudiopro.com](https://flstudiopro.com/fl-studio-scale-highlighting/)
- Snap to scale toggles pitch constraint; "FL Studio will not automatically snap notes to a Scale unless you edit them (they are moved)." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Rename - Opens a dialog to rename a keyboard key." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Piano roll shading (View) options: Default, Black notes, Auto (shade according to selected key/scale). — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Chromatic rows" with "octave labels" implied by keyboard; note names on keys configurable. — [mixelite.com](https://mixelite.com/blog/fl-studio-piano-roll-guide/)

### Inferences
- The pitch axis is strictly chromatic (12-TET) with no microtonal row support in the standard Piano roll; scale highlighting is a visual overlay, not a different tuning.
- Custom note naming (FL Studio 26.1 beta) allows renaming keys in List view. — [WhatsNew change log](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/WhatsNew.htm)

### Gaps
- I did not find explicit documentation of octave-number labels (e.g., C4) on the keyboard beyond configurable key labels; default labeling scheme not specified.

## 8. Time Axis, Navigation, Overview

### Takeaway
Time runs left-to-right in bars/beats over a zoomable/scrollable grid; navigation uses scrollbars (with mini preview), middle-mouse pan, zoom presets, and the top ruler supports time-range selection; precise time indicator and incremental scrolling are options.

### Cited Findings
- "Duration - The horizontal length represents time. The number on the Piano roll's top-ruler are bars (groups of beats)." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Mini piano roll preview - Shows a context aware preview of Notes in the Piano roll, on the Scroll bar." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Incremental scrolling - The scroll-position will scroll incrementally, rather than jumping to the precise location, when you click on the Scroll Bar rail." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Precise time indicator - A vertical line is used to show playback and edit position." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "You can click the (Middle-Mouse-Button) and drag in the Piano roll to scroll vertically and horizontally at the same time." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Small scrollbars in editors" setting (F10 General) sets scrollbar width to 50%. — [Interface manual](http://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_interface.htm)
- "Zoom in/out (Page Up / Page Down) - Zoom centers on cursor." / "Quick-zoom 1, 2 & 3 (Shift+1, 2, 3)" / "Zoom out far (Shift+4)" / "On Selection (Shift+5) - Zoom to timeline selection." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Scroll zoom (horizontal): Ctrl+Mouse Wheel; (vertical): Ctrl+Shift+Mouse Wheel." — [Audeobox shortcuts](https://www.audeobox.com/learn/fl-studio/fl-studio-piano-roll-shortcuts-guide/)
- "Keep labels on screen - Note names/labels will slide along the Note as the Piano roll plays so as to remain visible." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Time segments shading: "Change the vertical Playlist light/dark background shading according to the option selected. Default is '4 Bars'." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Background waveform (Alt+N) - When a waveform has been dragged and dropped on the Piano roll, displays the waveform to aid Note alignment with audio events." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Keep labels on screen", "Shadow", "Rounded" note appearance options. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)

### Inferences
- The mini-preview on the scrollbar acts as an overview, compensating for the lack of a separate overview pane.
- Time-segment shading (4 bars) provides macro-level orientation for long patterns.

### Gaps
- I found no separate "overview" window for the Piano roll; navigation relies on scrollbars + zoom.

## 9. View Options

### Takeaway
View options include ghost notes (same-pattern grey notes, cross-pattern open blocks in 24.x, editable ghosts), scale highlighting, note colors (16 color groups), velocity and property lanes (pan, release, Mod X/Y, pitch, filter), note appearance (shadow/rounded/labels), and grid contrast/invert.

### Cited Findings
- Ghost channels (Alt+V): "The Piano roll shows Notes from all channels in the selected Pattern as grey semi-transparent Notes. Double Right-Click Ghost Notes to swap to their Channel." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Editable ghosts (Ctrl+Alt+V): "Allows direct interaction with Ghost Notes." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- FL Studio 24 Ghost Notes: "The Piano roll can now show Ghost Notes from other Patterns, if the Pattern clips overlap in the Playlist... Out-of-Pattern Ghost Notes are outlined notes, in-Pattern are solid notes." Ghost Note 'Channels' selector on upper-right; Playlist Track headers Right-Click > 'Show patterns as PR ghosts'. — [What's new FL Studio 24](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html///basics_new_24.htm)
- "Double Right-Click Ghost note swaps to Ghost Channel." — [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)
- Note colors: "To select the note/slide color, click the Color selector then one of the 16 colors on the color group selector." Change color of existing notes: Edit > Change color (Alt+C). "Select by color (Shift+C) selects all Notes and slides that have the same color." — [Piano roll manual (beta)](https://www.image-line.com/fl-studio-learning/fl-studio-beta-online-manual/html/pianoroll.htm), [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Color groups enable independent editing of overlapping notes; "Colors are used for independent processing of notes in the Piano roll." — [Piano roll manual (beta)](https://www.image-line.com/fl-studio-learning/fl-studio-beta-online-manual/html/pianoroll.htm)
- Note properties & Event Editor (lower lane): velocity, pan, release (where supported), Mod X & Y (where supported), pitch, filter cutoff/resonance. Target control selects data; Shift+F cycles targets; mouse-wheel while hovering selector. — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Note properties move with the notes they belong to. The editor can also display standard Event Automation associated with the pattern." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "To edit properties of a note when it has the same start time as others, make a selection first." / "To set each note's property to the same value, use (Shift+Left-Click) and drag." / "To reset a property, use (Alt+Left-Click)." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Velocity lane: "At the bottom of the Piano Roll, you will see a bar graph showing the velocity value for each note... value ranges from 0 to 127." / "Drawing Velocity Curves: with Draw tool active in velocity lane, click and drag across multiple velocity bars." — [Audeobox](https://www.audeobox.com/learn/fl-studio/how-to-use-the-piano-roll-in-fl-studio/)
- "Shift+D brings up the velocity section" (older versions). — [homemusicmaker.com](https://www.homemusicmaker.com/fl-studio-change-velocity-multiple-notes)
- Alt+Mouse-wheel over a note changes currently selected property; Ctrl+Alt+Mouse-wheel fine-tunes. — [Piano roll manual (beta)](https://www.image-line.com/fl-studio-learning/fl-studio-beta-online-manual/html/pianoroll.htm)
- "Note length in properties - Displays a horizontal line equal to the length of Notes in the Note properties window." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Grid options: Grid Color (Ctrl+Alt+G), Grid contrast (High/Medium/Low), Invert grid. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Note appearance: Shadow (subtle shadow beneath notes), Rounded (rounds note shape), Keep labels on screen. — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Velocity Editor - Improved responsiveness when making quick swipe-edits to note velocity" (FL Studio 25.2). — [What's new FL Studio 25](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html//basics_new_25.htm)

### Inferences
- The event editor doubles as both per-note property editor (moves with notes) and automation editor (time-based curve), distinguished by Target control selection.
- Color groups are the mechanism for independent editing of overlapping notes in the same roll.

### Gaps
- I did not find documentation of a dedicated "note colors by velocity" auto-coloring toggle; color is manually assigned per color group.

## 10. Undo/Redo, Copy/Paste, Duplicate, Clone, Drag to Other Channels

### Takeaway
Undo/redo (Ctrl+Z / Ctrl+Alt+Z step-by-step), cut/copy/paste (clipboard, paste starts at leftmost visible bar), duplicate (Ctrl+B), clone patterns via Pattern menu; cross-channel editing via Target Channel dropdown, ghost-note double-right-click swap, and editable ghosts.

### Cited Findings
- "Undo/Redo last edit (Ctrl/Cmd+Z)"; "Undo step-by-step (Ctrl/Cmd+Alt/Opt+Z)." — [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)
- Cut (Ctrl+X), Copy (Ctrl+C), Paste (Ctrl+V): "Notes are pasted starting from the leftmost bar that is visible in the Piano roll window." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Duplicate (Ctrl+B): "Duplicate the selection to the right of the current location. If no selection is made all Notes are duplicated." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- "Shift+drag copies notes" (copy-drag). — [mixelite.com](https://mixelite.com/blog/fl-studio-piano-roll-guide/)
- Target Channel: "Select a Channel with the drop-down menu, in the title bar, to select for editing. Channels with notes, in the currently selected Pattern, will display a tick. You can roll your mouse-wheel while hovering over the control to show Channels with notes OR hold (Ctrl+Mouse-wheel) to show used and unused Channels. Auto locate - When Patterns are changed the Piano roll will select the first Channel with notes." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Double Right-Click Ghost note swaps to Ghost Channel." — [Keyboard shortcuts](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_shortcuts.htm)
- "Editable ghosts (Ctrl/Cmd+Alt/Opt+V) - Allows direct interaction with Ghost Notes" (edits go to source channel). — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Drag-and-drop: a score can be saved/copied by "Left-clicking and dragging on this menu [Save score as] a score can be saved to the Browser or copied to another Piano roll." — [Piano roll menu manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Pattern clone: Pattern menu "Clone (Alt+C) - Makes a copy of the selected pattern after it." — [Toolbar panels manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)
- "Split by channel - Creates a new pattern for each active Channel in the selected pattern." — [Toolbar panels manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)

### Inferences
- There is no direct "drag notes to another channel" gesture in the sources; cross-channel work is done via Target Channel dropdown or ghost-note swap/editable ghosts.
- Clipboard paste is position-based (leftmost visible bar), not cursor-based, which affects workflow when pasting into a specific spot.

### Gaps
- I found no documentation of a "drag selection to another channel" operation; channel reassignment of notes appears to require ghost-note swap or manual copy/paste.

## 11. Piano Roll vs Patterns vs Clips

### Takeaway
The Piano roll edits Pattern data (note scores) for a selected Channel; Patterns are reusable MIDI sequences played by Pattern Clips placed in the Playlist timeline. Editing in the Piano roll changes Pattern data; moving Pattern Clips changes where that data plays. Double-clicking a Pattern Clip opens its Piano roll (depending on "Edit patterns in" setting).

### Cited Findings
- "FL Studio's Piano roll... sends note and automation data to plugin instruments associated with the Piano roll's Channel. This process is known as 'sequencing'." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Target Channel - Select a Channel with the drop-down menu, in the title bar, to select for editing. Channels with notes, in the currently selected Pattern, will display a tick." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Opening from the Playlist - If the Playlist Menu option 'Edit patterns in > Piano roll' is selected, when you (Double-Click) Pattern Clips they will open the Piano roll for that Pattern Clip." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "A four-bar melody in Pattern 3 isn't part of the song until Pattern 3 appears in the Playlist. Editing in the Piano Roll changes the Pattern data. Moving the Pattern Clip changes where that data plays in the arrangement." — [flstudiopro.com](https://flstudiopro.com/fl-studio-piano-roll)
- "In Pattern Mode, only the current pattern is played. In Song Mode the whole Playlist arrangement is played." — [Toolbar panels manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)
- "Pattern selector: A maximum of 999 individual patterns can be created." — [Toolbar panels manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)
- "Pattern Clips... can be placed on Playlist tracks; clips function similarly to notes in the Piano roll." — [Playlist manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/playlist.htm)
- "Edit patterns in" option (FL Studio 24.1): determines what happens when Double-Clicking a Pattern Clip (Piano roll). — [What's new FL Studio 24](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html///basics_new_24.htm)
- "Adjust start time" (FL Studio 24.2): Playlist option to enable/disable Piano roll time relative to where a Pattern Clip was double-clicked. — [What's new FL Studio 24](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html///basics_new_24.htm)
- "Ghost Notes from other Patterns, if the Pattern clips overlap in the Playlist" (24.x). — [What's new FL Studio 24](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html///basics_new_24.htm)
- Step sequencer vs Piano roll: "The Stepsequencer overlays the Piano roll and so switching to this mode allows you to edit step sequences in Piano roll mode, so long as you respect the note length (zero length) and positioning (on beat) layout." — [Channel Rack manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/channelrack.htm)
- "Generally note/score data is saved to a Piano roll. You may need to click on the channel and open the Piano roll to see your data. It is also possible to record the melody into the Step Sequencer." — [Recording manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/recording_scores.htm)
- "FL Studio's Piano roll sends note and automation data to plugin instruments associated with the Piano roll's Channel." — [Piano roll manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)

### Inferences
- The Piano roll is a "pattern editor" (not a song/arrangement editor): edits affect the Pattern's score for the selected channel; arrangement happens in the Playlist via Pattern Clips.
- Ghost notes across overlapping clips (24.x) bridge the gap between pattern-level editing and arrangement-level context.

### Gaps
- I did not find documentation of whether editing a Pattern Clip's notes in-place (vs the Pattern) is possible; the manual describes editing the Pattern, not the clip instance. "Make Unique" (Shift+V drag / Ctrl+Shift+V) creates unique clip instances. — [What's new FL Studio 24](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html///basics_new_24.htm)