# FL Studio Piano Roll — Advanced Composition Features

## Ghost notes: how they work, showing notes from other patterns/channels, opacity, editing ghost notes?

### Takeaway
Ghost notes are semi-transparent references to notes in other channels/patterns; FL Studio 24.2 added out-of-pattern ghost notes from overlapping Playlist clips, with a Ghost Note channel selector and editable-ghost mode.

### Cited Findings
- Ghost notes show note data from other channels (Piano rolls) and are helpful for composing melodies/rhythms relative to those notes. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- There are two types: within-pattern ghost notes (solid blocks) from other channels in the selected pattern, and out-of-pattern ghost notes (open blocks) shown when pattern clips overlap in the Playlist. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- FL Studio 24.2 added the ability to view ghost notes from other patterns when pattern clips overlap, with a new Ghost Note "Channels" selector in the upper-right of the Piano roll. — [Image-Line, What's new in 2024.2](https://image-line.com/blog/whats-new-in-2024-2-a8f5f167f44f4964)
- To edit a within-pattern ghost note, double right-click (or X1 button) on it to switch to that channel; out-of-pattern ghost notes cannot be edited that way. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- "Editable ghosts" (Ctrl/Cmd+Alt/Opt+V) allows direct interaction with ghost notes as if they were normal notes. — [Image-Line manual, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- The Ghost Note icon/panel can be left-clicked to enable/disable the channel selector; right-click menu includes Show All / Show None (Alt/Opt+V), ghost tracks, and ghost pattern clips. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Ghost notes are described as grey semi-transparent notes. — [Image-Line manual, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)

### Inferences
- Opacity is fixed by theme/UI rather than a user numeric control; no manual setting for ghost-note opacity was found.

### Gaps
- No documented per-note opacity/transparency control for ghost notes was found.

## Slide notes (portamento/glide): how slide events are created, target pitch, duration, curves, interaction with notes?

### Takeaway
Slide notes are proprietary FL Studio events that must overlap a sounding note; their overlap length sets slide time and the slide note's pitch is the target, with portamento as a faster note-transition variant.

### Cited Findings
- Slide Toggle (S) makes added notes "slide notes" (horizontal bar with triangle) that cause notes above/below to slide to the slide note's pitch; slide time is set by the length of the slide note. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Portamento Toggle (O) adds a portamento flag; it causes a short pitch slide from the end of one note to the note carrying the portamento flag (note transition effect). — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Slide notes must overlap the note they slide; a slide note alone produces no sound. The overlap length sets slide time; if the slid note ends before reaching the slide pitch, the slide terminates prematurely. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- For chords, the topmost note is the reference pitch offset; all notes in the chord slide so the top note reaches the slide note pitch, moving by the same interval. Note colors allow independent slides for up to 16 notes. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Slide events have the usual note properties (velocity, panning, cutoff, resonance, etc.); during slides all note properties move toward those set in the slide note. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Slide notes/portamento work only with native FL Studio instruments, not VSTs; channel settings Polyphony > SLIDE and Time > GATE interact with portamento/slide notes. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Click-and-hold when adding a note can cycle through slide/portamento modes (if enabled in General settings). — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Scripting API confirms "Note Slide toggles whether it is a slide note or not. Slide notes are like portamento, only the pitch transition happens more slowly." — [Image-Line, Piano roll Scripting API](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_scripting_api.htm)

### Inferences
- "Curves" are not exposed as an editable curve type; slide behavior is governed by overlap length and note properties moving toward slide-note values.

### Gaps
- No explicit documentation of bezier/curve editing for slide transitions was found; slide is effectively linear-ish toward target pitch.

## Articulations: what articulation types exist, how applied, note properties?

### Takeaway
FL Studio's articulation support is primarily the Articulate tool (legato, portato, staccato, etc.) plus per-note Slide/Portamento toggles; there is no broad orchestral articulation-switch system in the piano roll.

### Cited Findings
- Articulate tool (Alt/Opt+L) applies quick portamento, staccato, and legato layouts; presets include Legato, Portato, Staccato. — [Image-Line, Piano roll Articulate Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_articulate.htm)
- Articulate options: ensure small gap between notes; just chop chords; multiply note length (10–100%); variation (randomized length); seed; chop chords; use lengths; only with selection. — [Image-Line, Piano roll Articulate Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_articulate.htm)
- Quick legato (Ctrl/Cmd+L) is a quick preset of the Articulate tool. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Per-note Slide (on/off) and Porta (on/off, inverts global portamento state for that note) are available in the Note Properties dialog. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)

### Inferences
- Articulation is implemented as note-length/timing manipulation rather than sample-switch keyswitches; keyswitch articulations depend on the instrument.

### Gaps
- No dedicated piano-roll articulation keyswitch/mapping feature was found in official docs.

## Note properties panel: velocity, pan, release, fine pitch, mod X/Y, channel, portamento, etc.

### Takeaway
Per-note properties are edited via the Note Properties dialog (double-click a note) and the Event Editor target control; supported properties include pan, velocity, release, Mod X/Y, slide/porta, color, time, and duration, with fine pitch available as a target.

### Cited Findings
- Note Properties dialog (double-click a note) levels: PAN, VEL (note-on velocity), REL (release velocity), MODX (filter cutoff), MODY (filter resonance); MOD X/Y can link to custom parameters in native plugins (e.g., Harmor). Release velocity supported in Sytrus, Harmor, Harmless, Ogun. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Slide (on/off) and Porta (on/off, inverts global portamento state) toggles, Color (color group), Start Time (bar:step:tick), Duration (bar:step:tick), Reset, Accept. If the double-clicked note is part of a selection, properties apply to all selected notes; Time section unavailable in that case. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Event Editor target control displays note properties (panning, velocity, etc.) as vertical lines with circles; note properties move with their notes. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Target controls include velocity, pan, pitch, filter cutoff, and automation events; (Shift+F) cycles targets. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Fine Pitch is listed among note properties in tutorial sources (release, fine pitch, Mod X/Y). — [MusicTech tutorial](https://musictech.com/tutorials/fl-studio-tutorial-power-3-2)
- Scripting API exposes note pan (0.0–1.0), velocity (0.0–1.0), release (0.0–1.0), and pitch offset in tenths of a semitone, range -120 to 120. — [Image-Line, Piano roll Scripting API](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_scripting_api.htm)
- Channel/color group is selected per note; color groups map to MIDI channels for wrapped plugins. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)

### Inferences
- Fine pitch resolution in the UI is coarse (forum reports 10-cent steps); exact UI granularity is not documented in the manual.

### Gaps
- Official manual does not specify the exact cent resolution of Fine Pitch in the UI; community reports suggest 10-cent increments.

## Humanize: parameters, what gets humanized, randomization ranges?

### Takeaway
FL Studio has no single dedicated piano-roll "Humanize" tool; humanization is achieved via Randomizer levels, Articulate variation, Quantizer groove, and the Chord Progression Tool's Humanize performance mode.

### Cited Findings
- No dedicated piano-roll Humanize tool was found; forum threads ask for note-start humanization and suggest Randomize/Quantize/Strum. — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=132697)
- Randomizer Tool "Levels" section randomizes pan, velocity, etc. for selected notes/channels; wheels range from -100% to 100%; 0% leaves original values; Reset Before Processing and Bipolar options; seed navigation. — [Image-Line, Piano roll Randomizer Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_random.htm)
- Randomizer Pattern section can randomize portamento switch and generate notes based on key/scale; notes created have current snap length. — [Image-Line, Piano roll Randomizer Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_random.htm)
- Articulate tool "Variation" randomizes note length by a specified amount with a seed. — [Image-Line, Piano roll Articulate Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_articulate.htm)
- Quantizer supports groove templates (.fsc) with start-time/end-time mixing and sensitivity, allowing humanized timing. — [Image-Line, Piano roll Quantizer](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_qnt.htm)
- Chord Progression Tool Performance "Humanize" mode parameters: Timing depth, Dynamics depth, Strum time, Strum curve, Phrasing weight, Phrasing offset. — [Image-Line, Chord Progression Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_chordprogression.htm)

### Inferences
- "Humanize" as a labeled feature is most fully implemented in the Chord Progression Tool's performance humanization, not as a standalone piano-roll command.

### Gaps
- No single piano-roll Humanize dialog with note-start randomization ranges was found in official docs.

## Strum and flam tools: how they work, parameters?

### Takeaway
Strum shifts timing/velocity/end-time of notes sharing a start time to simulate strumming; Flam adds a quick stroke before each note for drum-flam effects.

### Cited Findings
- Strum Tool treats overlapping notes with the same start time as one chord; controls Start (time offset, tension, velocity change, tension, preserve end, trigger ahead) and End (time shift, tension), plus Misc (chop chords, alternate direction). — [Image-Line, Piano roll Strum Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_strum.htm)
- Strum positive time shift strums low-to-high; negative strums high-to-low. — [Image-Line, Piano roll Strum Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_strum.htm)
- Flam Tool adds quick strokes before notes; controls Time (absolute or tempo-based), Before, Velocity, Group notes; includes presets. — [Image-Line, Piano roll Flam Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_flam.htm)

### Inferences
- Strum's "trigger ahead" lets the strum straddle the original start time; "chop chords" handles overlapping held chords.

### Gaps
- No default numeric ranges for strum time/velocity were found beyond UI knobs.

## Chord stamps / chord tool: predefined chords, custom chords, inversion?

### Takeaway
The Chord Stamp tool places chords/scales/percussion stamps on click; FL Studio 2026.2 beta added right-click creation of chords from a timeline selection or all notes.

### Cited Findings
- Chord Stamp tool lets you add chords without manual construction; choose a chord type from the Stamp menu then click to place; follows current Snap and Scale settings. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Options: Only one (revert to single-note mode after placing); Automatic chord (top-down) melody-driven; Automatic chord (bottom-up) harmony-driven; Manual selections: Melodic chords (standard + Advanced submenu), Melodic scales, Percussions, Special (includes Slide up). — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Stamp tool also allows selecting chords, scales, percussion patterns; "Only one" reverts to standard note entry. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- FL Studio 2026.2 BETA-1: right-click the Chord Stamp tool to Create Chords from a Timeline selection or all notes if nothing is selected. — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=343133)
- Inversions are exposed in the Chord Progression Tool (cycle main chord notes up/down; inversion state shown), not in the basic Stamp tool. — [Image-Line, Chord Progression Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_chordprogression.htm)

### Inferences
- The Stamp tool itself has no inversion control; inversions are handled in the Chord Progression Tool or by manual note editing.

### Gaps
- No documentation of custom chord creation in the Stamp tool itself; custom scales are created via .fsc reference scales.

## Scale highlighting / snap to scale: scale selection, root, modes?

### Takeaway
Scale highlighting and snapping are controlled via the Snap-to-scale icon and View > Helpers > Scale highlighting, with root note, scale type, multiple shading modes, and custom scales via .fsc files.

### Cited Findings
- Snap to scale icon: select/deselect snap to key/scale; right-click to choose scales. Root note sets tonic; "Snap PR notes to scale" forces edited notes to nearest valid scale note; "Snap incoming MIDI" constrains live/recorded MIDI; Piano roll shading Default/Black notes/Auto. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- View > Helpers > Scale highlighting: choose scale root (C–B) and scale type; Default (no shading), Black notes (shade non-scale), Automatic (detect score scale; name shown in hint bar). — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Automatic detection performs an average based on whole score or selection; several notes required before auto-detection begins. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Custom scales: based on .fsc files in ..\Data\Patches\Scores\Reference scales; drag/drop an .fsc, edit notes between C5 and B5, save back with a new name, restart FL Studio. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)

### Inferences
- "Modes" are represented as selectable scale types in the list rather than a separate modal system.

### Gaps
- Full list of built-in scale types/modes is not enumerated in the cited pages.

## Riff Machine / Score Log: what it does, generated patterns?

### Takeaway
Riff Machine is an 8-step pattern generator (note progression, chords, arpeggiation, mirroring, levels, articulation, groove, fit); Score Log is a rolling 3-minute buffer of external/typing-keyboard note activity dumpable to a pattern.

### Cited Findings
- Riff Machine generates melodies in the Piano roll; any/all of 8 steps can be enabled: Note progression, Chord progression, Arpeggiation, Mirroring, Levels & panning, Articulation, Groove, Fit. — [Image-Line, Riff Machine](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_riff.htm)
- Controls include Work on existing score, Length, Start over, Throw dice (new random seed), Accept, Preview up to current step. — [Image-Line, Riff Machine](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_riff.htm)
- Notes created by Riff Machine have the length of current piano-roll snap; Length/Variation won't work if snap is (none). — [Image-Line, Riff Machine](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_riff.htm)
- Score Logger is always on: a three-minute buffer records all note activity from external controllers and typing keyboard to piano; dump via Tools > Dump score log to selected pattern. — [Image-Line, Note/MIDI Recording](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/recording_scores.htm)

### Inferences
- "Score Log" is a capture/recovery feature rather than a composition generator; Riff Machine is the generative tool.

### Gaps
- No documentation of exporting/saving the score log beyond dumping to a selected pattern was found.

## Stamp tools and templates: note stamps, drum templates?

### Takeaway
Stamp tool provides chord/scale/percussion stamps; templates include arpeggiator/chopper .fsc score files and quantizer groove templates, with FPC percussion loops.

### Cited Findings
- Stamp tool adds chords and note-patterns from a menu; select chords, scales, percussion patterns; "Only one" reverts after use. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Arpeggiator/Chopper patterns are regular Piano roll scores (.fsc) in ..\Data\Patches\Scores\Arpeggiator; special color meanings (green boundary, blue unchopped, pink octave-immune, yellow length). — [Image-Line, Piano roll Arpeggiator](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_arpeggiate.htm)
- Quantizer uses .fsc groove templates. — [Image-Line, Piano roll Quantizer](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_qnt.htm)
- FPC includes a Pattern Manager with percussion loops (MIDI data) loadable into FPC as Piano roll. — [Image-Line, FPC](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/plugins/FPC.htm)

### Inferences
- "Drum templates" are primarily FPC percussion loops and percussion stamps rather than a standalone piano-roll template browser.

### Gaps
- No exhaustive list of built-in stamp presets was found.

## Arpeggiator integration? Randomize, shuffle?

### Takeaway
Piano roll has a dedicated Arpeggiator tool based on .fsc patterns, plus Riff Machine arpeggiation; Randomize generates notes/levels with seeds; "shuffle" is approximated by select/rotate/random tools.

### Cited Findings
- Arpeggiator Tool (Alt/Opt+A) applies arpeggio effects based on customizable .fsc scores; options pattern, transpose, time multiplication, range, range pattern, sync, gate, levels, group notes. — [Image-Line, Piano roll Arpeggiator](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_arpeggiate.htm)
- Riff Machine Step 3 Arpeggiation includes pattern modes (normal/flip/alternate), time mul, octave range, direction, sync, gate. — [Image-Line, Riff Machine](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_riff.htm)
- Randomizer Tool generates random notes based on key/scale/chord map and randomizes levels; seed navigation; population, stack, variation, length. — [Image-Line, Piano roll Randomizer Tool](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_random.htm)
- Selection tools include Select odd (swung notes), Select one/more at random, Rotate left/right, Shift left/right. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)

### Inferences
- There is no single "shuffle" command; rhythmic shuffling is done via rotate/quantize groove/select-odd/randomize.

### Gaps
- No dedicated shuffle/swing-percentage tool in the piano roll was found.

## Microtuning / tuning options?

### Takeaway
FL Studio supports per-note fine pitch (target control / note properties) in tenths of a semitone via scripting; UI resolution is coarse and full microtonal scales require external/plugin tuning.

### Cited Findings
- Fine Pitch is listed among note properties in tutorials (release, fine pitch, Mod X/Y). — [MusicTech tutorial](https://musictech.com/tutorials/fl-studio-tutorial-power-3-2)
- Scripting API pitch offset is in tenths of a semitone, range -120 to 120. — [Image-Line, Piano roll Scripting API](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_scripting_api.htm)
- Community reports fine-pitch adjustment limited to 10-cent steps in the piano roll. — [Image-Line forums](https://forum.image-line.com/viewtopic.php?t=328705)
- Per-note fine pitch works well with internal plugins (Flex, Harmor, DirectWave, etc.); external VST support varies. — [Medium microtonal guide](https://medium.com/@themicrotonefox/finding-the-right-daw-for-microtonal-music-a-comparative-guide-1aae5af4cbbc)

### Inferences
- Full microtonal scale authoring is not a native piano-roll feature; custom scales are 12-TET .fsc reference scales.

### Gaps
- Official manual does not document the exact UI cent resolution or a microtonal scale editor.

## Loop points, markers, time markers?

### Takeaway
Piano roll time markers (including Pattern length) define loop/section boundaries; time signatures can be set per pattern via markers.

### Cited Findings
- Time Markers: add in Pattern Mode at playhead; right-click to select type; set to "Pattern length" to define current pattern length. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- Time Marker menu: Add one (Alt/Opt+T), Add auto (Ctrl/Cmd+T), Add two, Add one every (bar/2/4/8 bars), Add time signature change, Add key marker, Delete. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Snap to "Markers" allows snapping to time markers. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Time signatures can be set per pattern and multiple time signatures stacked via patterns; they are visual helpers. — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)

### Inferences
- "Loop points" in the piano roll are effectively the Pattern length marker and playlist clip boundaries.

### Gaps
- No separate piano-roll loop-brace feature beyond Pattern length marker was found.

## Automation clips created from piano roll?

### Takeaway
Event data in the piano roll can be converted to an automation clip; interpolation, LFO, and scale-levels tools shape event data.

### Cited Findings
- Edit > Turn into automation clip converts currently selected Event data into an Automation clip. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Interpolate tool (I) transforms stepped event data into straight-line gradients (only genuine event data, not note-property lollipops). — [Image-Line manual, Piano roll](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)
- LFO tool (Alt/Opt+O) draws LFO shapes in the event editor. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Scale levels (Alt/Opt+X) multiply/offset/invert events and map to log/exp scales. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)
- Insert current controller value (Ctrl/Cmd+I) initializes the event editor with current control value. — [Image-Line, Piano roll menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_menu.htm)

### Inferences
- Automation clip creation requires selecting event data first; note-property lollipops are not directly convertible unless recorded as automation.

### Gaps
- No documentation on automation clip linkage/back-conversion to piano roll was found.