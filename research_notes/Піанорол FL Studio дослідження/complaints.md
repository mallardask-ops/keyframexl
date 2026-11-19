# User Complaints and Criticisms of FL Studio's Piano Roll

## Most Common Complaints (Editing, Workflow, UI, Performance)

### Takeaway
FL Studio's piano roll is widely praised as a benchmark MIDI editor, yet the same communities that praise it report recurring friction around snap behavior, accidental slide notes, zoom/scroll, copy/paste placement, and performance in large projects; complaints cluster on forums and Reddit with a long tail of "new user confusion" issues.

### Cited Findings
- Users on r/FL_Studio frequently report the piano roll "only playing black keys" or refusing to place notes on white keys, caused by "Snap to scale" being enabled — a widespread confusion point, not a bug — [Reddit r/FL_Studio "Can't write music using white keys in piano roll"](https://www.reddit.com/r/FL_Studio/comments/169vg65/cant_write_music_using_white_keys_in_piano_roll); [Image-Line forum "Piano Roll Black Keys Problem"](https://forum.image-line.com/viewtopic.php?t=312834); [Image-Line forum "Notes refuse to go on the proper lines in Piano Roll"](https://forum.image-line.com/viewtopic.php?t=326281).
- A recurring beginner complaint: notes placed on the piano roll produce no sound or all sound the same, usually because an empty Sampler channel or wrong base pitch (C5) is set — [Reddit r/FL_Studio "Piano Roll making no sound?"](https://www.reddit.com/r/FL_Studio/comments/istien/piano_roll_making_no_sound); [Reddit r/FL_Studio "Why do all the notes I place on piano roll all sound the same?"](https://www.reddit.com/r/FL_Studio/comments/mlwhx9/why_do_all_the_notes_i_place_on_piano_roll_all).
- Accidentally dragging an audio file into the piano roll creates a persistent "background waveform" overlay that confuses users; it persists in memory until FL Studio restarts — [Reddit r/FL_Studio "Help pls 🙏 I dragged an audio file into my piano roll by accident..."](https://www.reddit.com/r/FL_Studio/comments/o7rlw1/help_pls_i_dragged_an_audio_file_into_my_piano).
- Image-Line's own manual notes that "scroll speed will slow when snap settings other than 'Line' are used," acknowledging a known performance characteristic of the editor — [Image-Line FL Studio manual, "Piano roll"](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- The piano roll is frequently cited as the strongest feature in FL Studio; even critics of FL's UX single it out as the best-in-class MIDI editor — [KVR Audio, "Another DAW with piano roll as great as FL Studio?"](https://www.kvraudio.com/forum/viewtopic.php?t=549681); [Born To Produce, "Cubase vs Ableton vs FL Studio vs Logic (2026)"](https://www.borntoproduce.com/blogs/blog/cubase-vs-ableton-vs-fl-studio-vs-logic).

### Inferences
- Many "complaints" are actually discoverability problems (snap-to-scale, channel routing) rather than defects, which suggests the learning curve, not the feature set, is the dominant pain point for newcomers.
- Power users' complaints tend to be about edge cases (ghost notes, slide notes, large projects), while beginners struggle with basics (sound, key labels, zoom).

### Gaps
- No quantitative ranking of complaint frequency exists; forum/Reddit volume is anecdotal and Image-Line does not publish issue statistics.

## Specific Pain Points: Note Selection, Zoom/Scroll, Snap, Ghost Notes, Slide Notes, Velocity Editing, Copy/Paste, Undo

### Takeaway
Specific, well-documented pain points include: accidental conversion of notes to slide notes via click-and-hold; copy/paste landing notes in unexpected positions; zoom/scroll friction and accidental zoom; and confusing, buggy ghost-note workflows (in-pattern vs. out-of-pattern, editable ghost notes blocking new note placement).

### Cited Findings
- Note selection / slide-note accidents: Holding the mouse button cycles through note types (normal → slide), causing unexpected slide notes; "Click and hold functions in piano roll" can be disabled to avoid this — [Image-Line forum "Unexpected switch to slide notes in piano roll"](https://forum.image-line.com/viewtopic.php?t=81215). New users report "every note I enter... is a slide note by default" — [Image-Line forum "All notes are slide notes"](https://forum.image-line.com/viewtopic.php?t=207926).
- Slide notes only work reliably with FL-native plugins; third-party VSTs do not support FL's slide/portamento note system — [Image-Line forum "Slide Notes for 3rd Party VST Instruments"](https://forum.image-line.com/viewtopic.php?t=219259). Slide notes can silently fail or behave inconsistently ("Toggle slide notes not working") — [Image-Line forum "Toggle slide notes not working"](https://forum.image-line.com/viewtopic.php?t=333433).
- Notes accidentally becoming slide notes (gray triangle) causes them to play silently — [Image-Line forum "Some of the piano notes aren't playing in the piano roll"](https://forum.image-line.com/viewtopic.php?t=228191).
- Zoom/scroll: Users report inability to zoom out vertically except via Ctrl+right-drag; accidental zoom makes the roll show velocity lanes instead of notes — [Reddit r/FL_Studio "Won't Let Me Zoom Out in Piano Roll"](https://www.reddit.com/r/FL_Studio/comments/w445tr/wont_let_me_zoom_out_in_piano_roll); [Reddit r/FL_Studio "I keep zooming in like this on accident..."](https://www.reddit.com/r/FL_Studio/comments/1aeopfr/i_keep_zooming_in_like_this_on_accident_and_i).
- Resizing the piano roll window changes zoom and does not remember zoom settings, a long-standing complaint since FL 9/10 — [Image-Line forum "annoyed with the zooming on the piano roll when u resize the window"](https://forum.image-line.com/viewtopic.php?t=72567).
- Copy/paste: Ctrl+V places notes at unexpected/random positions; users ask how to paste without moving the playhead — [Reddit r/FL_Studio "piano roll copy pasting issue ctrl+v and ctrl+b"](https://www.reddit.com/r/FL_Studio/comments/t6jvse/piano_roll_copy_pasting_issue_ctrlv_and_ctrlb); [Reddit r/FL_Studio "FL Studio: How to Copy and Paste in Piano Roll"](https://www.reddit.com/r/FL_Studio/comments/ntmey9/fl_studio_how_to_copy_and_paste_in_piano_roll).
- Ghost notes: confusion between in-pattern and out-of-pattern ghost notes; "All" only applies to in-pattern ghost notes while "None" clears everything — [Image-Line forum "Problems with global ghost notes"](https://forum.image-line.com/viewtopic.php?t=330138). Ghost notes invisible in dark themes — [Image-Line forum "Ghost notes color in piano roll"](https://forum.image-line.com/viewtopic.php?t=293586). Editable ghost notes block placing a new note directly on top of them — [Image-Line forum "Can't seem to place a note over an editable ghost note"](https://forum.image-line.com/viewtopic.php?t=338117); [Image-Line forum "Piano roll ghost note question"](https://forum.image-line.com/viewtopic.php?t=338938).
- Velocity editing: "Moving the note level sliders in the piano roll sometimes stops working" was a logged bug in FL 2026 RC — [Image-Line forum "FL Studio 2026.1.3 RC" fixes list](https://forum.image-line.com/viewtopic.php?t=341610).
- Undo: crashes when removing undo events on macOS logged; "Copy/paste shortcuts don't work in Gopher" — [Image-Line What's New changelog](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/WhatsNew.htm).

### Inferences
- Ghost notes are a powerful but inconsistently understood feature; the distinction between "ghost channels" (in-pattern) and "out-of-pattern ghost notes" (playlist overlap) is the root of most confusion.
- Slide notes are a FL-specific paradigm that third-party plugin users cannot use, making them a portability problem.

### Gaps
- No official documentation of the "click-and-hold cycles note type" behavior is prominent; users discover it accidentally.

## Performance Issues: Lag with Many Notes, Rendering, Large Projects

### Takeaway
FL Studio's GUI (including the piano roll) is reported to lag visually in large projects even when CPU usage is low; specific regressions appeared in FL 2024/2025 (slow horizontal scroll when dragging selected notes); users work around with buffer size, PPQ, and consolidation.

### Cited Findings
- FL Studio 2024 "lagging like hell (Visual)": GUI freezes 2–3 s when clicking in timeline/piano roll; audio unaffected; reported with NVIDIA GPUs; multiple users confirmed — [Image-Line forum "FL Studio 2024 is lagging like hell (Visual)"](https://forum.image-line.com/viewtopic.php?t=330642).
- FL Studio 25 regression: horizontal scrolling while dragging selected notes became "really slow," with General→Inputs scroll speed having no effect — [Image-Line forum "Piano Roll Slow Horizontal Scrolling when dragging selected notes"](https://forum.image-line.com/viewtopic.php?t=337753).
- Image-Line's manual acknowledges that PPQ settings above 192 "can have a significant impact on CPU load," and recommends 96 PPQ — [Image-Line FL Studio manual, "Optimizing FL Studio Performance"](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/app_opt.htm).
- Scroll speed slows when snap is set to anything other than "Line" — [Image-Line FL Studio manual, "Piano roll"](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- Third-party guides list "Large or complex projects" as a cause of piano roll malfunction and recommend splitting projects — [Solar Heavy Studios, "How to Fix Piano Roll in Fl Studio"](https://solarheavystudios.com/how-to-fix-piano-roll-in-fl-studio).
- Users report FL becoming slow/laggy as projects grow despite low CPU usage and powerful hardware — [Image-Line forum "FL Studio slow and laggy but low CPU usage"](https://forum.image-line.com/viewtopic.php?t=260116).

### Inferences
- Much of the perceived "piano roll lag" is actually whole-application GUI lag (rendering, plugin windows, analyzers) rather than piano-roll-specific rendering, but users attribute it to the editor they are working in.
- The FL 25 scroll regression suggests a specific code change affecting drag-scroll; it was still present in v25.1.6 per the report.

### Gaps
- No official benchmark of piano roll rendering performance (notes/second) exists; claims of "lag with many notes" are anecdotal.

## UI/UX Criticisms: Cluttered Toolbar, Small Targets, Hidden Options, Inconsistent Behavior

### Takeaway
Critics describe FL Studio's UI as cluttered and menu-heavy, with features hidden in right-click menus and deep submenus; the piano roll toolbar is dense, and behaviors (right vs. left modifiers, click-and-hold cycling) are non-obvious. Community opinion is split — many defend the design as efficient once learned.

### Cited Findings
- "Is FL Studio's UI/UX Falling Behind?" forum thread: complaints of "visual clutter and inconsistencies," "too many floating windows," "menu overload," "features hidden within deep right-click menus" — [Image-Line forum "Is FL Studio's UI/UX Falling Behind?"](https://forum.image-line.com/viewtopic.php?t=332780).
- Reddit users describe the interface as "packed with buttons, windows, submenus... doesn't even feel like a creative tool half the time" — [Reddit r/FL_Studio "Does FL Studio Ever Feel Too Mechanical..."](https://www.reddit.com/r/FL_Studio/comments/1kng5hy/does_fl_studio_ever_feel_too_mechanical_and).
- New users from GarageBand find FL "really confusing to get a hang around"; the disconnected windows (channel rack, piano roll, mixer, playlist) are the main friction — [Reddit r/FL_Studio "UI problems with FL studio"](https://www.reddit.com/r/FL_Studio/comments/1k6kgth/ui_problems_with_fl_studio).
- The piano roll toolbar packs Tools, Snap, Snap-to-scale, Chord Stamp, Chord Detection, etc.; the Tools icon's right-click/middle-click behaviors were changed in 2026 (right-click now opens Chord Progression Tool) — a discoverability change users must relearn — [Image-Line What's New changelog, 2026 betas](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/WhatsNew.htm).
- "Click and hold functions in piano roll" silently cycles note types; disabling it is buried in settings — [Image-Line forum "Unexpected switch to slide notes in piano roll"](https://forum.image-line.com/viewtopic.php?t=81215).
- Multi-monitor/resize zoom behavior has been complained about since FL 9/10 and persists — [Image-Line forum "annoyed with the zooming on the piano roll when u resize the window"](https://forum.image-line.com/viewtopic.php?t=72567).

### Inferences
- FL Studio's design philosophy (floating windows, modifier-heavy editing) is polarizing: power users praise speed; newcomers and switchers from docked-DAW paradigms find it chaotic.

### Gaps
- No accessibility audit (screen reader, keyboard-only) of the piano roll was found in public sources.

## Missing Features Users Request: MPE, Microtonal, Chord Tools, Arpeggiator, Humanize, Scale Lock, etc.

### Takeaway
The most prominent unmet requests are MPE (no native support as of 2026), reliable microtonal workflow, true scale-lock (forced quantization to scale when dragging), editable ghost notes in multi-instrument patterns, and per-note expression editing; FL Studio 2026 added Chord Panel/Chord Progression and voice-leading chord stamps, partially addressing chord tools.

### Cited Findings
- MPE: FL Studio has "no MPE support at all" in its piano roll or MIDI routing as of September 2026; Image-Line has not announced it; users rely on third-party plugins (MIDISketch, XenRoll) — [eliotonic.com, "MPE in FL Studio"](https://eliotonic.com/guides/mpe-fl-studio); [moForte, "What is MPE?"](https://www.moforte.com/mpe) (lists FL Studio as "Not MPE (as of July 2026)").
- Microtonal: FL Studio offers per-note fine-pitch control that works with internal plugins, but external VSTs need Scala/TUN files or external retuners; the DAW does not pass SysEx reliably — [Medium, "Finding the right DAW for microtonal music"](https://medium.com/@themicrotonefox/finding-the-right-daw-for-microtonal-music-a-comparative-guide-1aae5af4cbbc). Users resort to XenRoll/Entonal Studio plugins — [GitHub, "XenRoll"](http://github.itlym.cn/Ankalot/XenRoll); [Entonal Studio](https://entonal.studio/).
- Scale lock: Feature request for "Piano Roll Feature: Scale Locking" — notes forced into scale when dragged, not just highlighted; request has been open since at least 2022 — [Image-Line forum "Piano Roll Feature: Scale Locking"](https://forum.image-line.com/viewtopic.php?t=284269). FL Studio has "Snap to scale" (constrains placement/movement) but users ask for stronger locking and pitch-quantize tools.
- Chord tools: FL Studio 2026 added Chord Panel with real-time chord detection and voice-leading chord stamps (top-down/bottom-up) — [lukaswojcik.com, "Top 5 DAWs in 2026"](https://www.lukaswojcik.com/blog/en/music-production/top-5-daws-2026-fl-studio-ableton-logic-cubase-bitwig); [Image-Line What's New](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/WhatsNew.htm). Chord Progression Tool generates progressions; users still request better chord editing as units.
- Arpeggiator: FL Studio's piano roll Arpeggiate tool (Alt+A) and channel arpeggiator are praised; requests focus on better live arpeggiation and step-sequencer integration — [EDMProd, "25 Unique Tips For the FL Studio Piano Roll"](https://www.edmprod.com/fl-studio-piano-roll).
- Humanize: Riff Machine includes "humanizing effects" step; users request more accessible humanize/randomize — [Loopazon guide](https://www.loopazon.com/blog/how-to-use-piano-fl-studio-guide).
- Multi-instrument editing: feature request for color-coded, editable ghost notes across instruments within one pattern, to avoid moving notes one-by-one — [Image-Line forum "feature request: piano roll multiple instruments usability"](https://forum.image-line.com/viewtopic.php?t=290441).
- Key detection: 2026 feature request for a dedicated Key Detection Panel analyzing the whole project — [Image-Line forum "Feature Request: Integrated Key Detection Panel..."](https://forum.image-line.com/viewtopic.php?t=341388).
- Note renaming: added in 2026 ("Notes in the Piano Roll can be renamed") — [lukaswojcik.com](https://www.lukaswojcik.com/blog/en/music-production/top-5-daws-2026-fl-studio-ableton-logic-cubase-bitwig).

### Inferences
- FL Studio is closing chord-tool gaps rapidly (2026 Chord Panel), but MPE and microtonal remain the largest structural gaps vs. Bitwig/Ableton/Logic.

### Gaps
- No roadmap commitment from Image-Line on MPE or microtonal; "has not announced it" is the only verified statement.

## Controversial Changes Across Versions (Improved vs. Regressed)

### Takeaway
FL Studio's piano roll has gained features steadily (ghost notes, chord tools, snap-to-scale, editable notes), but specific regressions/changes have annoyed users: FL 2024 GUI lag, FL 25 slow drag-scroll, changed Tools-icon right-click behavior in 2026, and the long-standing click-and-hold note-type cycling.

### Cited Findings
- FL 2024: GUI lag regression affecting piano roll/timeline clicking, confirmed by multiple users on high-end hardware — [Image-Line forum "FL Studio 2024 is lagging like hell (Visual)"](https://forum.image-line.com/viewtopic.php?t=330642).
- FL 25: slow horizontal scroll when dragging selected notes (regression from earlier versions) — [Image-Line forum "Piano Roll Slow Horizontal Scrolling..."](https://forum.image-line.com/viewtopic.php?t=337753).
- FL 2026 betas: stretching selections now snaps by default (Alt for unsnapped), changing prior muscle memory — [Image-Line forum "FL Studio 2026 Beta 10"](https://forum.image-line.com/viewtopic.php?t=340926); "Right shift adjust notes position" behavior changed in FL 26 — [Image-Line forum "Right shift adjust notes position not working properly in FL Studio 26?"](https://forum.image-line.com/viewtopic.php?t=341751).
- FL 2026: Tools icon right-click now opens Chord Progression Tool (Riff Machine moved to middle-click) — [Image-Line forum "FL Studio 2026.2 BETA-3"](https://forum.image-line.com/viewtopic.php?t=343489).
- FL 2026.1.3 RC fixes include "Can't disable the 'White notes only' piano roll option" and "Moving the note level sliders in the piano roll sometimes stops working" — indicating recent bugs — [Image-Line forum "FL Studio 2026.1.3 RC"](https://forum.image-line.com/viewtopic.php?t=341610).
- Improvements: FL 2024/2025 introduced global/editable ghost notes; 2026 added Chord Panel, note renaming, key labels presets, Chord Stamp voice-leading — [Image-Line What's New](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/WhatsNew.htm).
- "Auto zoom in piano roll" and "Small scrollbars in editors" are opt-in settings that change feel; "Scroll speed will slow when snap settings other than 'Line' are used" is a long-standing behavior — [Image-Line manual, "System Settings"](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).

### Inferences
- Image-Line iterates fast (lifetime free updates), so regressions are usually fixed within a few builds, but the changelog shows piano-roll bugs recur each cycle.

### Gaps
- No user-facing "regression" tracker; determining which version introduced a behavior requires forum archaeology.

## Comparison Complaints vs. Ableton/Logic/Cubase/Bitwig/Studio One Piano Rolls

### Takeaway
FL Studio's piano roll is repeatedly rated the best for mouse-driven MIDI editing and beatmaking, but users switching to/from other DAWs complain about FL's lack of MPE/microtonal, non-docked windows, pattern-based workflow, and third-party-plugin slide-note incompatibility; conversely, FL users praise its one-click draw/delete and chord stamps vs. Ableton's tool switching.

### Cited Findings
- Ableton users petition for FL-like piano roll: "left click adds note, right click deletes, scroll wheel moves and zooms," instant chord stamps, arpeggiation; "Ableton's piano roll just feels neglected" — [Ableton Forum, "Upgrade Piano Roll to match FL's..."](https://forum.ableton.com/viewtopic.php?t=236339).
- KVR: FL Studio's piano roll is "like a paint program... you are simply painting in notes"; Reaper/Ableton/Studio One "overcomplicate this with toolbars" — [KVR Audio, "Best DAW for piano roll edit"](https://www.kvraudio.com/forum/viewtopic.php?t=593155).
- Cubase and Logic are preferred by orchestral/arrangement users for multi-track MIDI editing and expression maps; FL is preferred for beat-driven music — [Gearspace, "Best Piano Roll"](https://gearspace.com/board/electronic-music-instruments-and-electronic-music-production/1453629-best-piano-roll.html); [Born To Produce comparison](https://www.borntoproduce.com/blogs/blog/cubase-vs-ableton-vs-fl-studio-vs-logic).
- Bitwig praised for native MPE editing and microtonal support; FL Studio has neither natively — [Medium microtonal guide](https://medium.com/@themicrotonefox/finding-the-right-daw-for-microtonal-music-a-comparative-guide-1aae5af4cbbc); [Bitwish, "Select/highlight the scale..." (credits FL)](https://bitwish.top/t/select-highlight-the-scale-to-work-with-on-the-piano-roll/2611).
- FL users leaving for Reaper cite "better performance, stability and audio recording/editing" despite worse piano roll — [KVR Audio](https://www.kvraudio.com/forum/viewtopic.php?t=593155).
- FL's pattern-based workflow and detached windows are a friction point for users coming from timeline-based DAWs (Logic/Cubase/Ableton) — [Reddit r/FL_Studio "UI problems with FL studio"](https://www.reddit.com/r/FL_Studio/comments/1k6kgth/ui_problems_with_fl_studio).

### Inferences
- FL's piano roll dominance is strongest in beat-driven, mouse-programmed genres (trap/EDM/hip-hop); its weaknesses (MPE, multi-track MIDI, orchestral expression) matter most to other user segments.

### Gaps
- No controlled usability study comparing DAW piano rolls exists in public sources.

## Accessibility and Ergonomics Complaints

### Takeaway
Publicly documented accessibility/ergonomics complaints about FL's piano roll are sparse; most ergonomic complaints concern small click targets, dense toolbars, modifier-heavy shortcuts, and multi-touch/trackpad behavior. No screen-reader/keyboard-only audit was found.

### Cited Findings
- Trackpad users report inverted horizontal scroll/zoom behavior in FL Studio 21 — [Reddit r/FL_Studio "Scrolling left/right and zooming with trackpad is inverted?"](https://www.reddit.com/r/FL_Studio/comments/1glf3li/scrolling_leftright_and_zooming_with_trackpad_is).
- FL Studio supports multi-touch gestures and GUI scaling (Main/Pop-ups/Toolbars) for high-DPI displays — [Image-Line manual, "System Settings"](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm); "Multi-Touch Support" — [Image-Line manual, "FL Studio User Interface"](https://www.image-line.com/fl-studio-learning-content/fl-studio-online-manual/html//basics_interface.htm).
- "Small scrollbars in editors" option halves scrollbar size, implying default scrollbars may be considered large/cluttered — [Image-Line manual, "System Settings"](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Ergonomic friction: zoom/scroll shortcuts (Ctrl+wheel width, Alt+wheel height) are easy to trigger accidentally; resizing window changes zoom — [Reddit r/FL_Studio "I keep zooming in like this on accident..."](https://www.reddit.com/r/FL_Studio/comments/1aeopfr/i_keep_zooming_in_like_this_on_accident_and_i); [Image-Line forum zoom complaint](https://forum.image-line.com/viewtopic.php?t=72567).

### Inferences
- FL Studio's ergonomics are tuned for mouse + keyboard; trackpad and touch users are second-class citizens.

### Gaps
- No screen-reader, colorblind, or motor-impairment accessibility evaluation of the piano roll was found; this is a genuine research gap.

## Well-Known Bugs and Long-Standing Issues

### Takeaway
Long-standing or repeatedly reported issues include: slide notes silently failing or behaving inconsistently; ghost-note bugs (placement blocking, color invisibility in dark themes, out-of-pattern confusion); snap-to-scale causing "only black keys" confusion; GUI lag in large projects; and version-specific regressions (FL 2024 lag, FL 25 slow drag-scroll).

### Cited Findings
- Slide notes: silent notes (gray triangle) when accidentally converted; slide notes only work with FL-native plugins; inconsistent behavior across projects — [Image-Line forum "Some of the piano notes aren't playing..."](https://forum.image-line.com/viewtopic.php?t=228191); [Image-Line forum "Slide Notes for 3rd Party VST Instruments"](https://forum.image-line.com/viewtopic.php?t=219259); [Image-Line forum "Toggle slide notes not working"](https://forum.image-line.com/viewtopic.php?t=333433).
- Ghost notes: placement blocked on editable ghost notes; "All/None" semantics confusion; dark-theme invisibility — [Image-Line forum "Can't seem to place a note over an editable ghost note"](https://forum.image-line.com/viewtopic.php?t=338117); [Image-Line forum "Problems with global ghost notes"](https://forum.image-line.com/viewtopic.php?t=330138); [Image-Line forum "Ghost notes color in piano roll"](https://forum.image-line.com/viewtopic.php?t=293586).
- "Only black keys / wrong notes" caused by Snap to scale — extremely common, technically a feature not a bug — [Image-Line forum "Piano Roll Black Keys Problem"](https://forum.image-line.com/viewtopic.php?t=312834); [Image-Line forum "Certain Notes Won't Play in FL Studio"](https://forum.image-line.com/viewtopic.php?t=313201).
- Piano roll crashes: "Piano Roll can't be opened after a limited-time trial ends" (logged 2026 beta); crashes when closing piano roll window; crashes when zooming — [Image-Line forum "FL Studio 2026 Beta 10"](https://forum.image-line.com/viewtopic.php?t=340926); [Image-Line forum "FL Studio 2026.2 BETA-3"](https://forum.image-line.com/viewtopic.php?t=343489).
- Tracking cursor out of position in playlist/piano roll in FL 24.x — [Image-Line forum "[BUG] Playlist / Piano roll tracking cursor out of position (FL 24.x)"](https://forum.image-line.com/viewtopic.php?t=330727).
- "Piano roll keeps playing past end of music" (phantom pattern length) — [Image-Line forum "Piano Roll Keeps playing past end of music"](https://forum.image-line.com/viewtopic.php?t=217391).
- Piano roll crash on click (Rosetta/macOS) reported — [Reddit r/FL_Studio "HELP! FL Studio piano roll bug"](https://www.reddit.com/r/FL_Studio/comments/1p3nain/help_fl_studio_piano_roll_bug).
- "Piano roll key labels missing" on macOS Catalina — [Reddit r/FL_Studio "Piano Roll Key Labels Missing"](https://www.reddit.com/r/FL_Studio/comments/dfk2ub/piano_roll_key_labels_missing).

### Inferences
- Image-Line responds to forum bug reports (site admins/devs reply), so many issues are acknowledged; however, several (MPE, microtonal, scale-lock) remain unaddressed for years.

### Gaps
- No public bug tracker with status; "fixed" claims rely on changelog entries that are terse and sometimes version-specific.
